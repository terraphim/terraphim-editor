#!/usr/bin/env python3
"""Real Firefox/WebKit checks of already-built local RoleGraph delivery.

Requires Python Playwright and browser binaries. WebKit is not actual Safari
or iPhone evidence. The agent-browser runner separately qualifies Chrome.
"""
import argparse
import functools
import http.server
import json
import pathlib
import shutil
import threading

from playwright.sync_api import sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--engine", choices=["firefox", "webkit"], default="firefox")
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    for asset in ["terraphim_editor_bg.wasm", "rolegraph/terraphim_rolegraph_browser_bg.wasm", "js/rolegraph-worker.js"]:
        if not (output / asset).is_file():
            raise SystemExit(f"Missing built asset: {output / asset}")
    for source, name in [
        ("tests/browser/delivery-rolegraph.html", "delivery-rolegraph.html"),
        ("tests/browser/worker-rolegraph.html", "worker-rolegraph.html"),
        ("crates/terraphim_rolegraph_browser/tests/fixtures/scenario.json", "scenario.json"),
        ("crates/terraphim_rolegraph_browser/tests/fixtures/review_scenario.json", "review_scenario.json"),
    ]:
        shutil.copyfile(root / source, output / name)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(output)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.engine).launch(headless=True)
            try:
                context = browser.new_context()
                context.route("https://**", lambda route: route.abort())
                page = context.new_page()
                for name, variable in [("worker-rolegraph.html", "workerResult"), ("worker-rolegraph.html?fixture=review_scenario.json", "workerResult"), ("delivery-rolegraph.html", "deliveryResult")]:
                    page.goto(f"http://127.0.0.1:{server.server_port}/{name}")
                    page.wait_for_function(f"Boolean(window.{variable})", timeout=60000)
                    result = page.evaluate(f"window.{variable}")
                    (output / f"{name.replace('?fixture=', '-')}.{args.engine}.result.json").write_text(json.dumps(result, indent=2) + "\n")
                    if not result.get("ok"):
                        raise SystemExit(json.dumps(result))
                    print(f"PASS {args.engine} {name}: {result.get('steps', len(result.get('checks', [])))} checks")
            finally:
                browser.close()
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
