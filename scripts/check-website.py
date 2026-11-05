#!/usr/bin/env python3
"""Optional browser check: pip install playwright; playwright install chromium.

Renders the real APT template at /klicky/ without signing or publishing anything.
Screenshots go to ignored target/website-review/. No production dependencies.
"""
import importlib.util
import mimetypes
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("apt", ROOT / "scripts/build-apt-repository.py")
apt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apt)
PAGE = apt.render_index(
    "https://muneer320.github.io/klicky", ["0.3.0-1", "0.4.0-1"],
    "DBB67AE478D2FFCEC6637B559899E554D3580D8F",
).encode("utf-8")


class Preview(BaseHTTPRequestHandler):
    def do_GET(self):
        path = unquote(urlsplit(self.path).path)
        content = PAGE
        content_type = "text/html; charset=utf-8"
        if path != "/klicky/":
            asset = None
            if path in ("/klicky/install.sh", "/klicky/install.ps1"):
                asset = ROOT / "scripts" / path.rsplit("/", 1)[1]
            elif path == "/klicky/assets/social-preview.png":
                asset = ROOT / "assets/branding/social-preview.png"
            elif path.startswith("/klicky/assets/"):
                folder = (ROOT / "assets/website").resolve()
                candidate = (folder / path.removeprefix("/klicky/assets/")).resolve()
                if candidate.is_relative_to(folder):
                    asset = candidate
            if asset is None or not asset.is_file():
                self.send_error(404)
                return
            content = asset.read_bytes()
            content_type = mimetypes.guess_type(asset)[0] or "application/octet-stream"
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(content)))
        self.end_headers()
        self.wfile.write(content)

    def log_message(self, *args):
        pass


def main():
    output = ROOT / "target/website-review"
    output.mkdir(parents=True, exist_ok=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Preview)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        if "--serve" in sys.argv:
            print(f"Preview: http://127.0.0.1:{server.server_port}/klicky/", flush=True)
            print("Press Ctrl+C to stop. Media and installer scripts are served; archive downloads are not.", flush=True)
            try:
                thread.join()
            except KeyboardInterrupt:
                pass
            return
        from playwright.sync_api import sync_playwright

        with sync_playwright() as p:
            browser = p.chromium.launch()
            page = browser.new_page()
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("console", lambda message: errors.append(message.text)
                    if message.type == "error" else None)
            url = f"http://127.0.0.1:{server.server_port}/klicky/"
            for theme in ["light", "dark"]:
                page.emulate_media(color_scheme=theme, reduced_motion="reduce")
                for width in [320, 360, 768, 1440]:
                    page.set_viewport_size({"width": width, "height": 1000})
                    assert page.goto(url).status == 200
                    assert page.title() == "Klicky / Give your keys a voice"
                    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                    assert page.locator("h1").count() == 1
                    assert page.locator("script").count() == 0
                    assert "{{" not in page.content()
                    for link in page.locator('a[href^="#"]').all():
                        assert page.locator(link.get_attribute("href")).count() == 1
                    page.keyboard.press("Tab")
                    assert page.locator(".skip").evaluate("e => e === document.activeElement")
                    assert page.locator(".skip").bounding_box()["y"] >= 0
                    page.keyboard.press("Enter")
                    key = page.locator(".instrument summary")
                    key.focus()
                    assert key.evaluate("e => getComputedStyle(e).outlineStyle") != "none"
                    page.keyboard.press("Enter")
                    assert page.locator(".instrument").get_attribute("open") is not None
                    assert page.locator(".trace").evaluate("e => getComputedStyle(e).animationName") == "none"
                    page.keyboard.press("Enter")
                    for summary in page.locator(".pack summary").all():
                        summary.focus()
                        page.keyboard.press("Space")
                    assert page.locator(".pack[open]").count() == 6
                    assert page.locator(".pack code").count() == 10
                    axe = output / "axe.min.js"
                    if axe.exists():
                        page.add_script_tag(path=str(axe))
                        violations = page.evaluate("async () => (await axe.run(document, {runOnly: {type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21aa']}})).violations")
                        assert not violations, [(v["id"], [n["target"] for n in v["nodes"]]) for v in violations]
                        print(f"PASS axe WCAG A/AA {theme} {width}px")
                    page.locator(".pack summary").last.evaluate("e => e.blur()")
                    page.evaluate("window.scrollTo(0, 0)")
                    page.screenshot(path=str(output / f"{theme}-{width}.png"), full_page=True)
                    print(f"PASS {theme} {width}px: overflow, anchors, focus, key/pack disclosure, reduced motion")
            page.emulate_media(reduced_motion="no-preference")
            page.goto(url)
            page.locator(".instrument summary").click()
            assert page.locator(".trace").evaluate("e => getComputedStyle(e).animationName") == "signal"
            assert not errors, errors
            browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    print("PASS: no browser console/runtime errors; /klicky/ base path; CSS animation")
    print(f"Screenshots: {output}")


if __name__ == "__main__":
    main()
