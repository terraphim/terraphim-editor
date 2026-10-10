#!/usr/bin/env python3
"""Run built RoleGraph fixtures with real Chrome/Worker/IndexedDB, no mocks.

Build target/rolegraph-delivery using Trunk and scripts/build-rolegraph.sh first.
Requires agent-browser. Uses an isolated session and ephemeral localhost port;
never writes committed dist or uses a signed-in browser profile.
"""
import functools
import http.server
import json
import pathlib
import shutil
import subprocess
import sys
import threading
import uuid

root = pathlib.Path(__file__).resolve().parents[2]
output = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else root / "target/rolegraph-delivery").resolve()
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

session = "rolegraph-ci-" + uuid.uuid4().hex
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(output)))
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()


def browser(*args):
    return subprocess.check_output(["agent-browser", "--session", session, *args], text=True, timeout=120)


try:
    # Block external assets; fixtures and all graph bytes are served locally.
    browser("open", "about:blank")
    browser("network", "route", "https://*", "--abort")
    for name, variable in [("worker-rolegraph.html", "workerResult"), ("worker-rolegraph.html?fixture=review_scenario.json", "workerResult"), ("delivery-rolegraph.html", "deliveryResult")]:
        browser("open", f"http://127.0.0.1:{server.server_port}/{name}")
        browser("wait", "--fn", f"Boolean(window.{variable})")
        result = json.loads(browser("eval", f"window.{variable}"))
        (output / (name.replace("?fixture=", "-") + ".result.json")).write_text(json.dumps(result, indent=2) + "\n")
        if not result.get("ok"):
            raise SystemExit(json.dumps(result))
        print(f"PASS {name}: {result.get('steps', len(result.get('checks', [])))} checks")
finally:
    try:
        browser("close")
    finally:
        server.shutdown()
        server.server_close()
