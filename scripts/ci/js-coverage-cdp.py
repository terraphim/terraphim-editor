#!/usr/bin/env python3
"""V8 coverage of public/js during one browser test run (issue #100).

usage: js-coverage-cdp.py collect PORT OUT.json JS_DIR
       js-coverage-cdp.py report JS_DIR RUN.json...

`collect` connects to Chrome's DevTools port (the wasm-bindgen test runner's
Chrome, started with --remote-debugging-port through webdriver.json),
attaches to every page, starts V8 precise block coverage and polls it until
the browser goes away (or SIGTERM). When the coverage build's final request
(the Rust profile posted to /__wasm_bindgen/coverage after the last test) is
seen, one last take is asked for, so the end of the run is not left to the
next poll. Scripts whose source is exactly one of
the files in JS_DIR (the tests inject them as inline <script> elements) are
recorded: for each file, the covered lines and every function with whether
it ran. `report` merges the runs and prints line and function coverage per
file.

Lines: a line counts when it holds code (not blank, not only a comment); it
is covered when a character of that code ran. Functions: as V8 reports them,
covered when called at least once.
Only the standard library is used (a minimal WebSocket client).
"""

import base64
import json
import os
import signal
import socket
import struct
import sys
import time
import urllib.request


# --- WebSocket (RFC 6455), client side, enough for CDP --------------------


class WebSocket:
    def __init__(self, url):
        assert url.startswith("ws://")
        hostport, _, path = url[5:].partition("/")
        host, _, port = hostport.partition(":")
        self.sock = socket.create_connection((host, int(port or 80)), timeout=30)
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall(
            (
                f"GET /{path} HTTP/1.1\r\nHost: {hostport}\r\nUpgrade: websocket\r\n"
                f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
            ).encode()
        )
        head = b""
        while b"\r\n\r\n" not in head:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("handshake closed")
            head += chunk
        if b" 101 " not in head.split(b"\r\n", 1)[0]:
            raise ConnectionError(head.split(b"\r\n", 1)[0].decode())
        self.buf = head.split(b"\r\n\r\n", 1)[1]

    def send(self, text):
        data = text.encode()
        n = len(data)
        if n < 126:
            header = struct.pack("!BB", 0x81, 0x80 | n)
        elif n < 1 << 16:
            header = struct.pack("!BBH", 0x81, 0x80 | 126, n)
        else:
            header = struct.pack("!BBQ", 0x81, 0x80 | 127, n)
        mask = os.urandom(4)
        self.sock.sendall(header + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _read(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(1 << 20)
            if not chunk:
                raise ConnectionError("closed")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def recv(self):
        message = b""
        while True:
            b0, b1 = self._read(2)
            n = b1 & 0x7F
            if n == 126:
                (n,) = struct.unpack("!H", self._read(2))
            elif n == 127:
                (n,) = struct.unpack("!Q", self._read(8))
            payload = self._read(n)
            opcode = b0 & 0x0F
            if opcode == 0x8:
                raise ConnectionError("closed by peer")
            if opcode in (0x9, 0xA):
                continue  # ping/pong: Chrome does not require a reply here
            message += payload
            if b0 & 0x80:
                return message.decode()


# --- source helpers ------------------------------------------------------


def utf16_len(text):
    return len(text.encode("utf-16-le")) // 2


def utf16_to_index(text):
    """Map each UTF-16 offset to a Python string index."""
    out = []
    for i, ch in enumerate(text):
        out.append(i)
        if ord(ch) > 0xFFFF:
            out.append(i)
    out.append(len(text))
    return out


def code_mask(text):
    """True for each character that is code (not whitespace, not a comment)."""
    mask = [False] * len(text)
    i, n = 0, len(text)
    quote = None
    while i < n:
        c = text[i]
        if quote:
            mask[i] = True
            if c == "\\":
                if i + 1 < n:
                    mask[i + 1] = True
                i += 2
                continue
            if c == quote:
                quote = None
            i += 1
            continue
        if c == "/" and text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        if c in "'\"`":
            quote = c
        if not c.isspace():
            mask[i] = True
        i += 1
    return mask


def line_starts(text):
    starts = [0]
    for i, c in enumerate(text):
        if c == "\n":
            starts.append(i + 1)
    return starts


# --- collect -------------------------------------------------------------


def collect(port, out_path, js_dir):
    files = {}
    for name in sorted(os.listdir(js_dir)):
        if name.endswith(".js"):
            with open(os.path.join(js_dir, name), encoding="utf-8") as f:
                files[name] = f.read()
    by_len = {}
    for name, text in files.items():
        by_len.setdefault(utf16_len(text), []).append(name)

    stop = {"now": False}
    signal.signal(signal.SIGTERM, lambda *_: stop.update(now=True))

    # Wait for Chrome's DevTools endpoint.
    url = None
    while not stop["now"]:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json/version", timeout=1) as r:
                url = json.load(r)["webSocketDebuggerUrl"]
            break
        except OSError:
            time.sleep(0.05)
    result = {"files": {}}
    if url:
        try:
            run(WebSocket(url), files, by_len, result, stop)
        except (ConnectionError, OSError):
            pass  # the browser closed: the run is over
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(result, f)


def run(ws, files, by_len, result, stop):
    ws.sock.settimeout(0.2)
    next_id = [0]
    pending = {}

    def send(method, params=None, session=None, on_reply=None):
        next_id[0] += 1
        msg = {"id": next_id[0], "method": method, "params": params or {}}
        if session:
            msg["sessionId"] = session
        pending[next_id[0]] = on_reply
        ws.send(json.dumps(msg))

    # Script ids belong to a page's current isolate, which every session
    # attached to that page shares, and restart in another page or after a
    # navigation. Keys are (page target id, script id); a page's keys are
    # dropped when its contexts are cleared (a navigation). scripts: key ->
    # file name, once its source is known to be one of the files; waiting:
    # key -> coverage entries that arrived before its source did (a busy page
    # answers getScriptSource late), replayed when it is matched.
    scripts = {}
    waiting = {}
    session_page = {}

    def key(session, script_id):
        return (session_page.get(session, session), script_id)
    # file -> covered characters, functions by start ([name, end, ran]) and
    # the (start, end) of every function seen
    state = {}
    sessions = set()
    targets = set()

    def on_source(k):
        def handle(reply):
            if k not in waiting:
                return  # the page navigated away meanwhile
            pending_entries = waiting.pop(k)
            src = reply.get("result", {}).get("scriptSource")
            if src is None:
                return
            for name in by_len.get(utf16_len(src), []):
                if files[name] == src:
                    scripts[k] = name
                    state.setdefault(name, {"covered": bytearray(len(src)), "functions": {}, "known": set(),
                                           "idx": utf16_to_index(src)})
                    for entry in pending_entries:
                        record(name, entry)

        return handle

    def on_coverage(session):
        def handle(reply):
            for script in reply.get("result", {}).get("result", []):
                k = key(session, script["scriptId"])
                if k in scripts:
                    record(scripts[k], script)
                elif k in waiting:
                    waiting[k].append(script)

        return handle

    def record(name, script):
        """Add one script entry of a takePreciseCoverage answer to `name`."""
        text = files[name]
        st = state[name]
        idx = st["idx"]
        last = len(idx) - 1
        # Every range of the reported functions, plus every function
        # seen before but not reported now (it did not run since the
        # last poll), painted outer first: each character ends with
        # its innermost range's count.
        ranges = []
        reported = set()
        for fn in script["functions"]:
            outer = fn["ranges"][0]
            lo, hi = idx[outer["startOffset"]], idx[min(outer["endOffset"], last)]
            reported.add((lo, hi))
            if fn["functionName"] or outer["startOffset"] > 0:
                entry = st["functions"].setdefault(str(lo), [fn["functionName"], hi, False])
                entry[2] = entry[2] or outer["count"] > 0
                st["known"].add((lo, hi))
            for r in fn["ranges"]:
                ranges.append((idx[r["startOffset"]], idx[min(r["endOffset"], last)], r["count"] > 0))
        ranges.extend((lo, hi, False) for lo, hi in st["known"] - reported)
        ranges.sort(key=lambda r: (r[0], -r[1]))
        take = bytearray(len(text))
        for a, b, ran in ranges:
            take[a:b] = (b"\x01" if ran else b"\x00") * (b - a)
        pos = take.find(1)
        while pos >= 0:
            end = take.find(0, pos)
            end = len(take) if end < 0 else end
            st["covered"][pos:end] = b"\x01" * (end - pos)
            pos = take.find(1, end)

    def final_take():
        # The coverage build's last request has been sent: ask every session
        # for a final take. The request is only observed, never paused
        # (pausing it, even briefly, made the runner lose the Rust profile).
        for s in list(sessions):
            send("Profiler.takePreciseCoverage", session=s, on_reply=on_coverage(s))

    def attach(session):
        sessions.add(session)
        # wasm-bindgen-test posts the Rust coverage to /__wasm_bindgen/coverage
        # once every test has run; seeing that request marks the end of the
        # run, where a final take is asked for (see final_take).
        send("Network.enable", session=session)
        send("Runtime.enable", session=session)
        send("Debugger.enable", session=session)
        send("Profiler.enable", session=session)
        send("Profiler.startPreciseCoverage", {"callCount": True, "detailed": True}, session=session)
        send("Runtime.runIfWaitingForDebugger", session=session)

    send("Target.setDiscoverTargets", {"discover": True})
    send("Target.setAutoAttach", {"autoAttach": True, "waitForDebuggerOnStart": True, "flatten": True})
    last_poll = 0.0
    while not stop["now"]:
        now = time.monotonic()
        if now - last_poll > 0.25:
            for s in list(sessions):
                send("Profiler.takePreciseCoverage", session=s, on_reply=on_coverage(s))
            last_poll = now
        try:
            raw = ws.recv()
        except socket.timeout:
            continue
        except (ConnectionError, OSError):
            break  # the browser closed: the run is over
        msg = json.loads(raw)
        if "id" in msg:
            if "error" in msg:
                result.setdefault("errors", []).append(msg["error"].get("message", ""))
            handler = pending.pop(msg["id"], None)
            if handler:
                handler(msg)
            continue
        method, params, session = msg.get("method"), msg.get("params", {}), msg.get("sessionId")
        if method == "Target.targetCreated" and params["targetInfo"]["type"] == "page":
            if params["targetInfo"]["targetId"] not in targets:
                targets.add(params["targetInfo"]["targetId"])
                send("Target.attachToTarget", {"targetId": params["targetInfo"]["targetId"], "flatten": True})
        elif method == "Target.attachedToTarget":
            targets.add(params["targetInfo"]["targetId"])
            session_page[params["sessionId"]] = params["targetInfo"]["targetId"]
            if params["targetInfo"]["type"] == "page" and params["sessionId"] not in sessions:
                attach(params["sessionId"])
        elif method == "Network.requestWillBeSent" and "/__wasm_bindgen/coverage" in params["request"]["url"]:
            final_take()
        elif method == "Target.detachedFromTarget":
            sessions.discard(params.get("sessionId"))
        elif method == "Debugger.scriptParsed" and params.get("length", 0) in by_len:
            k = key(session, params["scriptId"])
            if k not in scripts and k not in waiting:
                waiting[k] = []
                send("Debugger.getScriptSource", {"scriptId": params["scriptId"]}, session=session,
                     on_reply=on_source(k))
        elif method == "Runtime.executionContextsCleared" and session:
            page = session_page.get(session, session)
            for table in (scripts, waiting):
                for k in [k for k in table if k[0] == page]:
                    del table[k]
        elif method == "Runtime.executionContextCreated" and session:
            # A new document (a navigation, maybe to a new renderer
            # process): turn coverage on again for it.
            send("Profiler.enable", session=session)
            send("Profiler.startPreciseCoverage", {"callCount": True, "detailed": True}, session=session)

    for name, st in state.items():
        text = files[name]
        starts = line_starts(text)
        mask = code_mask(text)
        lines_code, lines_hit = set(), set()
        line = 0
        for k in range(len(text)):
            while line + 1 < len(starts) and starts[line + 1] <= k:
                line += 1
            if mask[k]:
                lines_code.add(line + 1)
                if st["covered"][k]:
                    lines_hit.add(line + 1)
        result["files"][name] = {
            "lines": sorted(lines_code),
            "hit": sorted(lines_hit),
            "functions": st["functions"],
        }


# --- report --------------------------------------------------------------


def report(js_dir, runs):
    names = sorted(n for n in os.listdir(js_dir) if n.endswith(".js"))
    merged = {n: {"lines": set(), "hit": set(), "functions": {}} for n in names}
    for path in runs:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        for name, d in data.get("files", {}).items():
            m = merged.setdefault(name, {"lines": set(), "hit": set(), "functions": {}})
            m["lines"].update(d["lines"])
            m["hit"].update(d["hit"])
            for key, (fname, end, ran) in d["functions"].items():
                e = m["functions"].setdefault(key, [fname, end, False])
                e[2] = e[2] or ran
    print(f"{'File':<34}{'Lines':>8}{'Missed':>8}{'Cover':>9}{'Funcs':>8}{'Missed':>8}{'Cover':>9}")
    print("-" * 84)
    tl = th = tf = tr = 0
    for name in names:
        m = merged[name]
        nl, nh = len(m["lines"]), len(m["hit"])
        nf = len(m["functions"])
        nr = sum(1 for e in m["functions"].values() if e[2])
        tl, th, tf, tr = tl + nl, th + nh, tf + nf, tr + nr
        if nl == 0:
            print(f"{'public/js/' + name:<34}{'-':>8}{'-':>8}{'not loaded':>9}")
            continue
        print(f"{'public/js/' + name:<34}{nl:>8}{nl - nh:>8}{100 * nh / nl:>8.2f}%{nf:>8}{nf - nr:>8}"
              f"{(100 * nr / nf if nf else 100):>8.2f}%")
    print("-" * 84)
    print(f"{'TOTAL':<34}{tl:>8}{tl - th:>8}{(100 * th / tl if tl else 0):>8.2f}%{tf:>8}{tf - tr:>8}"
          f"{(100 * tr / tf if tf else 0):>8.2f}%")


if __name__ == "__main__":
    if len(sys.argv) >= 5 and sys.argv[1] == "collect":
        collect(int(sys.argv[2]), sys.argv[3], sys.argv[4])
    elif len(sys.argv) >= 3 and sys.argv[1] == "report":
        report(sys.argv[2], sys.argv[3:])
    else:
        sys.exit(__doc__)
