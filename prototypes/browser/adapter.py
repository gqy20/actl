"""Experimental synchronous CDP adapter, restricted to marked loopback probe pages.

No public actl command, shell evaluation, implicit target selection or write replay.
Only the probe runner may grant its own isolated test-page lease.
"""
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit
import os


class ProbeError(Exception):
    def __init__(self, code, message, **evidence):
        super().__init__(message)
        self.code, self.evidence = code, evidence


def loopback_url(value, schemes=("http",)):
    try:
        url = urlsplit(value)
        valid = (url.scheme in schemes and url.hostname in ("127.0.0.1", "localhost", "::1")
                 and url.port is not None and not url.username and not url.password)
    except ValueError:
        valid = False
    if not valid:
        raise ProbeError("PROTOCOL", "probe endpoints must be explicit loopback URLs")
    return url


def stop_path():
    root = os.environ.get("LOCALAPPDATA")
    if not root:
        raise ProbeError("PROTOCOL", "LOCALAPPDATA is required; do not guess stop state")
    return Path(root) / "actl" / "stop-requested"


def check_stop(path):
    try:
        path.stat()
    except FileNotFoundError:
        return
    except OSError as error:
        raise ProbeError("INTERNAL", "stop state could not be observed") from error
    raise ProbeError("ABORTED", "existing emergency stop remains set", reason="stop_active")


def selector(kind, value):
    if kind not in ("css", "xpath") or not isinstance(value, str) or not value or len(value) > 4096:
        raise ProbeError("PROTOCOL", "selector requires css/xpath and 1-4096 characters")
    return f"{kind}={value}"


@dataclass(frozen=True)
class Binding:
    endpoint: str
    target_id: str
    url: str
    marker: str

    def validate(self):
        endpoint = loopback_url(self.endpoint, ("ws",))
        loopback_url(self.url)
        if not endpoint.path.startswith("/devtools/browser/") or not self.target_id or not self.marker:
            raise ProbeError("PROTOCOL", "require browser-session endpoint, target ID and probe marker")


class Adapter:
    def __init__(self, playwright, binding, stop_file, timeout_ms=3000):
        binding.validate()
        if not 100 <= timeout_ms <= 10000:
            raise ProbeError("PROTOCOL", "timeout must be 100-10000 ms")
        self.playwright, self.binding, self.stop_file = playwright, binding, Path(stop_file)
        self.timeout = timeout_ms
        self.browser = self.page = self.cdp = None
        self.uncertain = False

    def attach(self):
        check_stop(self.stop_file)
        if self.browser is not None:
            raise ProbeError("PROTOCOL", "detach before explicitly reattaching")
        try:
            self.browser = self.playwright.chromium.connect_over_cdp(self.binding.endpoint, timeout=self.timeout)
            hits = []
            for context in self.browser.contexts:
                for page in context.pages:
                    session = context.new_cdp_session(page)
                    try:
                        target_id = session.send("Target.getTargetInfo")["targetInfo"]["targetId"]
                    finally:
                        session.detach()
                    if target_id == self.binding.target_id:
                        hits.append(page)
            if len(hits) != 1:
                raise ProbeError("NOT_FOUND" if not hits else "AMBIGUOUS", "bound target is not unique")
            self.page = hits[0]
            self.page.set_default_timeout(self.timeout)
            self.cdp = self.page.context.new_cdp_session(self.page)
            self.guard()
        except Exception as error:
            try:
                self.detach()
            except Exception as cleanup_error:
                if isinstance(error, ProbeError):
                    error.evidence["detach_error"] = str(cleanup_error)
            if isinstance(error, ProbeError):
                raise
            raise ProbeError("INTERNAL", "CDP attach failed", reason="connection_failed") from error

    def detach(self):
        # Playwright's connected Browser.close disconnects this client. This adapter
        # creates no contexts/pages; it never owns or closes the remote browser.
        browser = self.browser
        self.browser = self.page = self.cdp = None
        if browser is not None:
            browser.close()

    def guard(self):
        check_stop(self.stop_file)
        if self.browser is None or not self.browser.is_connected():
            raise ProbeError("NOT_ACTIONABLE", "browser is disconnected", reason="disconnected")
        if self.page is None or self.page.is_closed():
            raise ProbeError("NOT_FOUND", "bound page is closed")
        info = self.cdp.send("Target.getTargetInfo")["targetInfo"]
        if info["targetId"] != self.binding.target_id or self.page.url != self.binding.url:
            raise ProbeError("STALE_REF", "bound target or URL changed; explicitly rebind")
        meta = self.page.locator('meta[name="actl-probe"]')
        if meta.count() != 1 or meta.get_attribute("content") != self.binding.marker:
            raise ProbeError("NOT_ACTIONABLE", "page does not carry this probe's marker")

    @staticmethod
    def unique(locator):
        count = locator.count()
        if count != 1:
            raise ProbeError("NOT_FOUND" if count == 0 else "AMBIGUOUS", "locator must be unique", count=count)
        return locator

    def locate(self, kind, value, frame=None):
        self.guard()
        root = self.page
        if frame is not None:
            owner = self.unique(self.page.locator(selector("css", frame)))
            handle = owner.element_handle(timeout=self.timeout)
            if handle is None:
                raise ProbeError("STALE_REF", "iframe owner was detached")
            try:
                actual_frame = handle.content_frame()
                if actual_frame is None:
                    raise ProbeError("PROTOCOL", "frame selector does not point to an iframe")
                loopback_url(actual_frame.url)
            finally:
                handle.dispose()
            root = owner.content_frame
            frame_meta = self.unique(root.locator('meta[name="actl-probe"]'))
            if frame_meta.get_attribute("content") != self.binding.marker:
                raise ProbeError("NOT_ACTIONABLE", "iframe does not carry this probe's marker")
        return self.unique(root.locator(selector(kind, value)))

    def read(self, kind, value, frame=None):
        try:
            result = self.locate(kind, value, frame).text_content(timeout=self.timeout)
            self.guard()
            return result
        except ProbeError:
            raise
        except Exception as error:
            raise ProbeError("INTERNAL", "browser observation failed", reason="observation_failed") from error

    def click_verified(self, kind, value, expected_selector, expected_text, frame=None):
        if self.uncertain:
            raise ProbeError("NOT_ACTIONABLE", "previous write requires explicit recheck", reason="needs_review")
        # Validate all syntax and resolve before sending any write.
        selector(kind, value)
        selector("css", expected_selector)
        started = False
        try:
            action = self.locate(kind, value, frame)
            self.locate("css", expected_selector, frame)
            self.guard()
            check_stop(self.stop_file)
            started = True
            action.click(timeout=self.timeout)
            actual = self.read("css", expected_selector, frame)
            if actual != expected_text:
                raise ProbeError("ASSERTION_FAILED", "postcondition did not match")
            return {"verified": True, "action_attempts": 1, "actual": actual}
        except Exception as error:
            if started:
                self.uncertain = True
            code = error.code if isinstance(error, ProbeError) else "INTERNAL"
            raise ProbeError(code, "browser write did not complete verification",
                             action_started=started, uncertain=started, retry="never") from error

    def recheck(self, kind, value, expected, frame=None):
        actual = self.read(kind, value, frame)
        if actual != expected:
            raise ProbeError("ASSERTION_FAILED", "recheck failed", uncertain=self.uncertain)
        self.uncertain = False
        return {"verified": True, "replayed": False}
