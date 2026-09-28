"""Offline contract tests; fake browser only, no user profile or stop mutation."""
import json
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace

from adapter import Adapter, Binding, ProbeError, check_stop, loopback_url

MARKER = "test-task"
URL = "http://127.0.0.1:1234/test-task"
ENDPOINT = "ws://127.0.0.1:4321/devtools/browser/session-a"


class Locator:
    def __init__(self, page, kind):
        self.page, self.kind = page, kind

    def count(self):
        return self.page.counts.get(self.kind, 1)

    def get_attribute(self, name):
        return self.page.marker

    def text_content(self, **_):
        if self.page.read_error:
            raise RuntimeError("provider failed")
        return self.page.receipt

    def click(self, **_):
        self.page.writes += 1
        self.page.receipt = f"saved:{self.page.writes}"
        if self.page.write_error:
            raise RuntimeError("connection lost after dispatch")

    def element_handle(self, **_):
        return SimpleNamespace(content_frame=lambda: self.page.child, dispose=lambda: None)

    @property
    def content_frame(self):
        return self.page.child


class Page:
    def __init__(self, target_id, child=False):
        self.target_id, self.url, self.marker = target_id, URL, MARKER
        self.closed = self.read_error = self.write_error = False
        self.receipt, self.writes, self.counts = "untouched", 0, {}
        self.child = None if child else Page("frame", True)

    def locator(self, key):
        return Locator(self, key)

    def is_closed(self):
        return self.closed

    def set_default_timeout(self, _):
        pass


class Browser:
    def __init__(self):
        self.connected = True
        self.pages = [Page("decoy"), Page("target")]
        context = SimpleNamespace(pages=self.pages, new_cdp_session=self.session)
        self.contexts = [context]
        for page in self.pages:
            page.context = context

    def session(self, page):
        def send(_):
            if not self.connected:
                raise RuntimeError("connection lost")
            return {"targetInfo": {"targetId": page.target_id}}
        return SimpleNamespace(send=send, detach=lambda: None)

    def is_connected(self):
        return self.connected

    def close(self):
        self.connected = False


class AdapterTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.stop = Path(self.temp.name) / "stop-requested"
        self.browser = Browser()
        self.connects = 0
        def connect(*_, **__):
            self.connects += 1
            self.browser.connected = True
            return self.browser
        self.runtime = SimpleNamespace(chromium=SimpleNamespace(connect_over_cdp=connect))
        self.adapter = Adapter(self.runtime, Binding(ENDPOINT, "target", URL, MARKER), self.stop)

    def assert_code(self, code, function):
        with self.assertRaises(ProbeError) as raised:
            function()
        self.assertEqual(raised.exception.code, code)
        return raised.exception

    def test_exact_target_beats_same_url_and_iframe_is_scoped(self):
        self.adapter.attach()
        self.assertEqual(self.adapter.page.target_id, "target")
        result = self.adapter.click_verified("css", "#submit", "#receipt", "saved:1", frame="#form")
        self.assertEqual(result, {"verified": True, "action_attempts": 1, "actual": "saved:1"})
        self.assertEqual(self.browser.pages[0].child.writes, 0)
        self.assertEqual(self.browser.pages[1].writes, 0)
        self.assertEqual(self.adapter.read("xpath", '//output[@id="receipt"]', frame="#form"), "saved:1")

    def test_missing_bound_target_never_chooses_decoy(self):
        self.browser.pages.pop()
        self.assert_code("NOT_FOUND", self.adapter.attach)
        self.assertIsNone(self.adapter.page)

    def test_navigation_or_marker_change_refuses_write(self):
        self.adapter.attach()
        self.adapter.page.url = URL + "/changed"
        self.assert_code("STALE_REF", lambda: self.adapter.click_verified("css", "#submit", "#receipt", "saved:1"))
        self.assertEqual(self.browser.pages[1].writes, 0)
        self.adapter.page.url = URL
        self.adapter.page.marker = "another-task"
        self.assert_code("NOT_ACTIONABLE", lambda: self.adapter.read("css", "#receipt"))

    def test_remote_or_unmarked_frame_rejected(self):
        self.adapter.attach()
        self.adapter.page.child.url = "https://example.com/"
        self.assert_code("PROTOCOL", lambda: self.adapter.read("css", "#receipt", frame="#form"))
        self.adapter.page.child.url = URL
        self.adapter.page.child.marker = "other"
        self.assert_code("NOT_ACTIONABLE", lambda: self.adapter.read("css", "#receipt", frame="#form"))

    def test_ambiguity_prevents_dispatch(self):
        self.adapter.attach()
        self.adapter.page.counts["css=#submit"] = 2
        error = self.assert_code("AMBIGUOUS", lambda: self.adapter.click_verified("css", "#submit", "#receipt", "saved:1"))
        self.assertFalse(error.evidence["action_started"])
        self.assertEqual(self.adapter.page.writes, 0)

    def test_delivery_uncertainty_blocks_replay_and_recheck_does_not_write(self):
        self.adapter.attach()
        self.adapter.page.write_error = True
        error = self.assert_code("INTERNAL", lambda: self.adapter.click_verified("css", "#submit", "#receipt", "saved:1"))
        expected = json.loads(Path(__file__).with_name("uncertain.golden.json").read_text())
        self.assertEqual({"code": error.code, "evidence": error.evidence}, expected)
        self.assert_code("NOT_ACTIONABLE", lambda: self.adapter.click_verified("css", "#submit", "#receipt", "saved:1"))
        self.assertEqual(self.adapter.page.writes, 1)
        self.adapter.detach()
        self.adapter.attach()
        self.assertTrue(self.adapter.uncertain)
        self.assertEqual(self.adapter.recheck("css", "#receipt", "saved:1"), {"verified": True, "replayed": False})
        self.assertEqual(self.adapter.page.writes, 1)

    def test_failed_postcondition_stays_uncertain(self):
        self.adapter.attach()
        self.assert_code("ASSERTION_FAILED", lambda: self.adapter.click_verified("css", "#submit", "#receipt", "wrong"))
        self.assertTrue(self.adapter.uncertain)
        self.assert_code("ASSERTION_FAILED", lambda: self.adapter.recheck("css", "#receipt", "wrong"))
        self.assertTrue(self.adapter.uncertain)

    def test_disconnect_and_read_failure_are_not_empty_success(self):
        self.adapter.attach()
        self.adapter.page.read_error = True
        self.assert_code("INTERNAL", lambda: self.adapter.read("css", "#receipt"))
        self.browser.connected = False
        self.assert_code("NOT_ACTIONABLE", lambda: self.adapter.read("css", "#receipt"))
        self.assertEqual(self.connects, 1)

    def test_stop_is_preserved_and_blocks_attach_and_write(self):
        self.stop.write_text("do not clear")
        self.assert_code("ABORTED", self.adapter.attach)
        self.assertEqual(self.connects, 0)
        self.assertEqual(self.stop.read_text(), "do not clear")

    def test_local_endpoint_validation(self):
        for value in ["http://example.com:1234", "http://127.0.0.1", "http://user@127.0.0.1:1234", "file:///x"]:
            self.assert_code("PROTOCOL", lambda: loopback_url(value))
        self.assert_code("PROTOCOL", lambda: Binding("ws://127.0.0.1:1234/devtools/page/a", "a", URL, MARKER).validate())


if __name__ == "__main__":
    unittest.main()
