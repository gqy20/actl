"""Run the browser connection probe against a new headless, isolated local session."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from uuid import uuid4

from adapter import Adapter, Binding, ProbeError, check_stop, stop_path


def serve(marker, frame_url=None):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path != f"/{marker}":
                self.send_error(404)
                return
            frame = f'<iframe id="form" src="{frame_url}"></iframe>' if frame_url else ""
            html = (f'<meta name="actl-probe" content="{marker}"><title>ACTL isolated probe</title>'
                    '<button id="submit" onclick="this.dataset.n=Number(this.dataset.n||0)+1;'
                    'document.querySelector(\'#receipt\').textContent=\'saved:\'+this.dataset.n">Save</button>'
                    f'<output id="receipt">untouched</output>{frame}')
            content = html.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(content)))
            self.end_headers()
            self.wfile.write(content)

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}/{marker}"


def digest(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(browser_executable, output):
    output.mkdir(parents=True, exist_ok=False)
    report = {"version": 1, "passed": False, "checks": [], "cleanup": "not_started",
              "platform": sys.platform, "python": sys.version, "calls": [],
              "started_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
    process = owner = adapter = runtime = None
    servers, logs = [], []
    stop = None
    closing = False

    def call(name, function):
        try:
            data = function()
            report["calls"].append({"name": name, "ok": True, "data": data})
            return data
        except ProbeError as error:
            report["calls"].append({"name": name, "ok": False, "code": error.code, "evidence": error.evidence})
            raise

    def expect_error(name, function, allowed):
        try:
            call(name, function)
        except ProbeError as error:
            if error.code in allowed:
                report["checks"].append(name)
                return
            raise
        raise ProbeError("ASSERTION_FAILED", f"{name} unexpectedly succeeded")

    try:
        stop = stop_path()
        check_stop(stop)
        if sys.platform != "win32":
            raise ProbeError("PROTOCOL", "this prototype executes on Windows only")
        executable = Path(browser_executable).resolve(strict=True)
        # Optional dependency is loaded only after the real stop gate.
        from playwright.sync_api import sync_playwright
        from importlib.metadata import version
        report.update(browser=str(executable), browser_sha256=digest(executable), playwright=version("playwright"),
                      source_sha256={name: digest(Path(__file__).with_name(name)) for name in ("adapter.py", "probe.py")})
        marker = uuid4().hex
        frame_server, frame_url = serve(marker)
        servers.append(frame_server)
        main_server, main_url = serve(marker, frame_url)
        servers.append(main_server)
        profile = output / "profile"
        profile.mkdir()
        logs = [open(output / name, "wb") for name in ("browser.stdout", "browser.stderr")]
        check_stop(stop)
        process = subprocess.Popen([str(executable), "--headless=new", "--remote-debugging-address=127.0.0.1",
                                    "--remote-debugging-port=0", f"--user-data-dir={profile}",
                                    "--no-first-run", "--no-default-browser-check", "about:blank"],
                                   stdout=logs[0], stderr=logs[1], creationflags=subprocess.CREATE_NO_WINDOW)
        report.update(pid=process.pid, profile=str(profile), cleanup="pending")
        port_file = profile / "DevToolsActivePort"
        deadline = time.monotonic() + 15
        while not port_file.exists():
            check_stop(stop)
            if process.poll() is not None or time.monotonic() >= deadline:
                raise ProbeError("NOT_ACTIONABLE", "owned browser did not expose CDP")
            time.sleep(0.05)
        lines = port_file.read_text().splitlines()
        endpoint = f"ws://127.0.0.1:{int(lines[0])}{lines[1]}"
        runtime = sync_playwright().start()
        owner = runtime.chromium.connect_over_cdp(endpoint, timeout=5000)
        context = owner.contexts[0]
        check_stop(stop)
        page = context.new_page()
        check_stop(stop)
        page.goto(main_url, wait_until="load", timeout=5000)
        # Same URL in another tab: selecting the first URL match would be unsafe.
        check_stop(stop)
        decoy = context.new_page()
        check_stop(stop)
        decoy.goto(main_url, wait_until="load", timeout=5000)
        cdp = context.new_cdp_session(page)
        target_id = cdp.send("Target.getTargetInfo")["targetInfo"]["targetId"]
        cdp.detach()
        report.update(endpoint=endpoint, target_id=target_id, fixture_url=main_url, frame_url=frame_url)
        binding = Binding(endpoint, target_id, main_url, marker)
        adapter = Adapter(runtime, binding, stop)
        call("attach_exact_target", adapter.attach)
        report["checks"].append("session_and_target_binding")
        call("iframe_write_verify", lambda: adapter.click_verified("css", "#submit", "#receipt", "saved:1", frame="#form"))
        report["checks"].append("cross_origin_iframe_write_verified")
        value = call("xpath_read", lambda: adapter.read("xpath", '//output[@id="receipt"]', frame="#form"))
        if value != "saved:1" or adapter.read("css", "#receipt") != "untouched":
            raise ProbeError("ASSERTION_FAILED", "iframe isolation/readback mismatch")
        if decoy.frame_locator("#form").locator("#receipt").text_content() != "untouched":
            raise ProbeError("ASSERTION_FAILED", "decoy target changed")
        report["checks"].append("xpath_and_decoy_unchanged")
        call("detach", adapter.detach)
        expect_error("detached_read_rejected", lambda: adapter.read("css", "#receipt"), {"NOT_ACTIONABLE"})
        call("explicit_reattach", adapter.attach)
        call("recheck_without_replay", lambda: adapter.recheck("css", "#receipt", "saved:1", frame="#form"))
        report["checks"].append("reattach_preserves_single_write")
        # Graceful shutdown of our isolated browser causes actual transport loss.
        check_stop(stop)
        adapter.guard()
        if any(p.url not in ("about:blank", main_url) for p in context.pages):
            raise ProbeError("NOT_ACTIONABLE", "foreign page appeared; retain owned browser for review")
        close_session = owner.new_browser_cdp_session()
        closing = True
        try:
            close_session.send("Browser.close")
        except Exception:
            # A close command can succeed before its response is delivered.
            pass
        process.wait(timeout=10)
        report["cleanup"] = "owned_browser_exited"
        expect_error("transport_loss_rejected", lambda: adapter.read("css", "#receipt"), {"INTERNAL", "NOT_ACTIONABLE", "NOT_FOUND"})
        report["passed"] = True
    except Exception as error:
        report["error"] = {"code": error.code if isinstance(error, ProbeError) else "INTERNAL",
                           "message": str(error), "evidence": error.evidence if isinstance(error, ProbeError) else {}}
    finally:
        # Failure/uncertainty/stop preserves the browser. Never terminate/kill it or
        # automatically clear stop. A retained profile path is recorded for review.
        if process is not None and process.poll() is None:
            report["cleanup"] = "close_pending_retained" if closing else "retained_for_review"
        for client in (adapter,):
            if client is not None:
                try:
                    client.detach()
                except Exception as error:
                    report["detach_error"] = str(error)
        cleanup_actions = ([runtime.stop] if runtime is not None else [])
        cleanup_actions += [action for server in servers for action in (server.shutdown, server.server_close)]
        cleanup_actions += [stream.close for stream in logs]
        for action in cleanup_actions:
            try:
                action()
            except Exception as error:
                report.setdefault("cleanup_errors", []).append({"code": "INTERNAL", "message": str(error)})
                report["passed"] = False
        (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog="Example: probe.py --browser-executable C:/path/chrome.exe --output target/probe-unique\nErrors: ABORTED=77; other failure=1. Reports go to files, diagnostics to stderr.")
    parser.add_argument("--browser-executable", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        report = run(args.browser_executable, args.output.resolve())
        print(f"Browser probe report: {args.output / 'report.json'}", file=sys.stderr)
        return 0 if report["passed"] else 77 if report.get("error", {}).get("code") == "ABORTED" else 1
    except Exception as error:
        print(f"Browser probe setup failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
