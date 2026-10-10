#!/usr/bin/env python3
"""Native Safari WebDriver PNG sampling, not WebKit or desktop capture."""
import base64
import socket
import struct
import math
import os
import urllib.request
from browser_demo_common import *


def webdriver(base, method, route, payload=None, *, timeout=20):
    data = None if payload is None else json.dumps(payload).encode()
    request = urllib.request.Request(base + route, data=data, method=method,
                                     headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        result = json.load(response)['value']
    if isinstance(result, dict) and result.get('error'):
        raise RuntimeError('Native WebDriver failed: ' + result['error'])
    return result


def validate_capabilities(value):
    if value.get('browserName', '').lower() != 'safari' or value.get('platformName', '').lower() not in {'mac', 'macos'}:
        raise RuntimeError('Native macOS Safari required; substitutes refused')


def recording_capabilities(value):
    # Keep source browser identity, not opaque vendor/session/window payloads.
    return {key: value[key] for key in ('browserName', 'browserVersion', 'platformName')
            if isinstance(value.get(key), str)}


VISIBILITY_NOTICE = ('Safari automation window ready: foreground the dedicated automation window '
                     'now; waiting up to 10 seconds for visible, focused content. '
                     'Do not foreground personal Safari tabs; no terminal acknowledgement required.')
VISIBILITY_SCRIPT = ('return {visible: document.visibilityState === "visible", '
                     'focused: document.hasFocus()};')


def wait_visible_owned_window(driver, metadata):
    """Read only the session's pinned current context; never enumerate/switch tabs."""
    print(VISIBILITY_NOTICE, flush=True)
    started = time.monotonic()
    deadline = started + 10
    evidence = {'source': 'owned-session-current-window/document-visibility-and-focus-v1',
                'timeoutSeconds': 10, 'attempts': 0, 'status': 'waiting'}
    metadata['visibilityPrecondition'] = evidence
    def call(method, route, payload=None):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise RuntimeError('Visibility wait exhausted')
        result = driver.call(method, route, payload, timeout=remaining)
        if time.monotonic() >= deadline:
            raise RuntimeError('Visibility wait exhausted')
        return result
    try:
        owned = call('GET', '/window')
        if not isinstance(owned, str) or not owned:
            raise RuntimeError('Owned window unavailable')
        while True:
            evidence['attempts'] += 1
            if call('GET', '/window') != owned:
                raise RuntimeError('Owned window changed')
            state = call('POST', '/execute/sync', {'script': VISIBILITY_SCRIPT, 'args': []})
            if call('GET', '/window') != owned:
                raise RuntimeError('Owned window changed')
            if not isinstance(state, dict) or set(state) != {'visible', 'focused'} or any(
                    type(state[key]) is not bool for key in state):
                raise RuntimeError('Visibility observation unavailable')
            evidence.update(visible=state['visible'], focused=state['focused'])
            if state['visible'] and state['focused']:
                evidence['status'] = 'ready'
                return
            time.sleep(max(0, min(.1, deadline - time.monotonic())))
    except Exception:
        evidence['status'] = 'failed'
        raise RuntimeError('Safari owned automation window visibility/focus unavailable within '
                           '10 seconds; no capture or storyboard started') from None
    finally:
        evidence['elapsedSeconds'] = time.monotonic() - started


def frame_durations(samples):
    if len(samples) < 2:
        raise ValueError('Insufficient actual screenshot samples')
    durations = [b['completed'] - a['completed'] for a, b in zip(samples, samples[1:])]
    if any(d <= 0 for d in durations):
        raise ValueError('Non-monotonic screenshot timestamps')
    return durations


def selection_context_point(rects, viewport=VIEWPORT):
    """Choose an integer point inside a visible selected glyph rectangle."""
    for rect in rects:
        values = [rect.get(k) for k in ('left', 'top', 'right', 'bottom')]
        if any(type(v) not in (int, float) or not math.isfinite(v) for v in values):
            raise ValueError('Malformed selection rectangle')
        left, top, right, bottom = values
        left, top = max(0, left), max(0, top)
        right, bottom = min(viewport['width'], right), min(viewport['height'], bottom)
        if right <= left or bottom <= top:
            continue
        x = max(math.ceil(left), math.floor(left + min(8, (right-left)/2)))
        y = max(math.ceil(top), math.floor((top+bottom)/2))
        if left <= x < right and top <= y < bottom:
            return {'x': x, 'y': y}
    raise ValueError('No visible selected text for context menu')


def native_drag_geometry(start, end):
    for point in (start, end):
        if not isinstance(point, (list, tuple)) or len(point) != 2:
            raise ValueError('Invalid native drag geometry')
        for value, limit in zip(point, (VIEWPORT['width'], VIEWPORT['height'])):
            if type(value) not in (int, float) or not math.isfinite(value) or not 0 <= value < limit:
                raise ValueError('Invalid native drag geometry')


def native_drag_ack(path, sequence, token):
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    except OSError as error:
        raise RuntimeError('Unsafe native drag acknowledgement refused') from error
    try:
        info = os.fstat(descriptor)
        if not stat.S_ISREG(info.st_mode) or info.st_size > 4096:
            raise RuntimeError('Unsafe native drag acknowledgement refused')
        raw = os.read(descriptor, 4097)
        if len(raw) > 4096:
            raise RuntimeError('Oversized native drag acknowledgement refused')
    finally:
        os.close(descriptor)
    value = json.loads(raw)
    if value != {'sequence': sequence, 'token': token, 'performedBy': 'CUA'}:
        raise RuntimeError('Native drag acknowledgement mismatch')


class Driver:
    def __init__(self, base, session, stage):
        self.base = base
        self.route = '/session/' + session
        self.stage = stage
        self.samples = []
        self.context_menu_inputs = []
        self.native_drag_handoff = None
        self.native_drag_inputs = []
        self.started = time.monotonic()
    def call(self, method, route, payload=None, *, timeout=20):
        return webdriver(self.base, method, self.route + route, payload, timeout=timeout)
    def evaluate(self, script, *args):
        return self.call('POST', '/execute/sync', {'script': script, 'args': list(args)})
    def click(self, selector):
        deadline = time.monotonic() + 10
        while True:
            elements = self.call('POST', '/elements', {'using': 'css selector', 'value': selector})
            if elements:
                element = elements[0]['element-6066-11e4-a52e-4f735466cecf']
                self.call('POST', '/element/' + element + '/click', {})
                return
            if time.monotonic() > deadline:
                raise RuntimeError('Required visible control missing')
            time.sleep(.1)
    def hover(self, selector):
        element=self.call("POST","/element",{"using":"css selector","value":selector})
        self.call("POST","/actions",{"actions":[{"type":"pointer","id":"public-hover","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","duration":150,"origin":element,"x":0,"y":0}]}]})
    def keys(self, keys):
        if keys == ['\ue008', '\ue03a']:
            # Safari did not generate contextmenu for Shift+F10 in the real
            # walkthrough. Use a genuine secondary-button pointer action.
            rects = self.evaluate('const s=getSelection();if(!s?.rangeCount||s.isCollapsed)return [];return Array.from(s.getRangeAt(0).getClientRects(),r=>({left:r.left,top:r.top,right:r.right,bottom:r.bottom}));')
            point = selection_context_point(rects)
            try:
                self.call('POST', '/actions', {'actions': [{'type': 'pointer', 'id': 'public-context-menu', 'parameters': {'pointerType': 'mouse'}, 'actions': [
                    {'type': 'pointerMove', 'duration': 100, 'origin': 'viewport', **point},
                    {'type': 'pointerDown', 'button': 2}, {'type': 'pointerUp', 'button': 2}]}]})
            finally:
                self.call('DELETE', '/actions')
            self.context_menu_inputs.append({'method': 'native W3C secondary-button click inside selection', **point})
            return
        keys=native_boundary_keys(keys,self.evaluate("return navigator.platform.includes(\"Mac\");"))
        actions = [{'type': 'keyDown', 'value': k} for k in keys]
        actions += [{'type': 'keyUp', 'value': k} for k in reversed(keys)]
        self.call('POST', '/actions', {'actions': [{'type': 'key', 'id': 'fixture-keys', 'actions': actions}]})
    def drag(self, start, end):
        if self.native_drag_handoff is not None:
            native_drag_geometry(start, end)
            state = self.evaluate('return {visible:document.visibilityState === "visible",focused:document.hasFocus(),width:innerWidth,height:innerHeight};')
            if state != {'visible': True, 'focused': True, **VIEWPORT}:
                raise RuntimeError('Owned Safari window not focused at native drag handoff')
            sequence = len(self.native_drag_inputs) + 1
            token = str(uuid.uuid4())
            request = {'sequence': sequence, 'token': token, 'viewport': VIEWPORT,
                       'start': start, 'end': end, 'method': 'native CUA drag; viewport coordinates require observed window mapping'}
            request['coordinationStatus'] = 'requested; not proof of output'
            self.native_drag_inputs.append(request)
            request_path = self.native_drag_handoff / f'request-{sequence}.json'
            acknowledgement = self.native_drag_handoff / f'ack-{sequence}.json'
            descriptor = os.open(request_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, 'w') as output:
                json.dump(request, output)
            print('Native CUA drag requested: ' + str(request_path), flush=True)
            deadline = time.monotonic() + 60
            while not acknowledgement.exists():
                if time.monotonic() >= deadline:
                    raise RuntimeError('Native CUA drag acknowledgement unavailable within 60 seconds')
                tick = time.monotonic()
                self.capture(timeout=max(.001, deadline - tick))
                time.sleep(max(0, min(.25 - (time.monotonic() - tick), deadline - time.monotonic())))
            if time.monotonic() >= deadline:
                raise RuntimeError('Native CUA drag acknowledgement expired')
            native_drag_ack(acknowledgement, sequence, token)
            state = self.call('POST', '/execute/sync', {'script': 'return {visible:document.visibilityState === "visible",focused:document.hasFocus(),width:innerWidth,height:innerHeight};', 'args': []}, timeout=max(.001, deadline - time.monotonic()))
            if time.monotonic() >= deadline or state != {'visible': True, 'focused': True, **VIEWPORT}:
                raise RuntimeError('Owned Safari window changed or native drag handoff expired')
            request['coordinationStatus'] = 'acknowledged; output checks remain independent'
            return
        moves=[{'type':'pointerMove','duration':0,'origin':'viewport','x':round(start[0]),'y':round(start[1])},
               {'type':'pointerDown','button':0},{'type':'pause','duration':500},
               {'type':'pointerMove','duration':200,'origin':'viewport','x':round(start[0]+12),'y':round(start[1])},
               {'type':'pointerMove','duration':700,'origin':'viewport','x':round(end[0]),'y':round(end[1])},
               {'type':'pause','duration':250},
               {'type':'pointerMove','duration':100,'origin':'viewport','x':round(end[0]+1),'y':round(end[1])},
               {'type':'pause','duration':100},{'type':'pointerUp','button':0}]
        try:
            self.call('POST','/actions',{'actions':[{'type':'pointer','id':'public-drag','parameters':{'pointerType':'mouse'},'actions':moves}]})
        finally:
            self.call('DELETE','/actions')
    def text(self, text):
        actions = []
        for c in text:
            actions += [{'type':'keyDown','value':c},{'type':'keyUp','value':c}]
        self.call('POST','/actions',{'actions':[{'type':'key','id':'public-demo-typing','actions':actions}]})
    def capture(self, *, timeout=20):
        started = time.monotonic() - self.started
        png = base64.b64decode(self.call('GET', '/screenshot', timeout=timeout), validate=True)
        completed = time.monotonic() - self.started
        if png[:8] != b'\x89PNG\r\n\x1a\n' or struct.unpack('>II', png[16:24]) != (1140, 720):
            raise RuntimeError('Actual viewport PNG must be exactly 1140x720')
        name = f'frame-{len(self.samples):06d}.png'
        (self.stage / name).write_bytes(png)
        self.samples.append({'name': name, 'started': started, 'completed': completed,
                             'sha256': hashlib.sha256(png).hexdigest()})
    def pause(self, seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            tick = time.monotonic()
            self.capture()
            time.sleep(max(0, min(.25 - (time.monotonic() - tick), deadline - time.monotonic())))


def main():
    arguments = parser()
    arguments.add_argument('--native-drag-handoff', type=pathlib.Path)
    args = arguments.parse_args()
    if args.native_drag_handoff is not None:
        handoff = args.native_drag_handoff
        if (not handoff.is_absolute() or handoff.is_symlink() or not handoff.is_dir()
                or handoff.resolve().parent != pathlib.Path('/private/tmp')
                or not handoff.name.startswith('safari-native-drag-') or any(handoff.iterdir())):
            raise ValueError('Native drag handoff requires a fresh dedicated /private/tmp/safari-native-drag-* directory')
    if not pathlib.Path('/usr/bin/safaridriver').is_file():
        raise RuntimeError('Installed native safaridriver required')
    with inputs(args, 'safari') as (stage, url, fixture, metadata, ffmpeg, ffprobe):
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        session = None
        driver = None
        with (stage / 'safaridriver.log').open('wb') as log:
            process = subprocess.Popen(['/usr/bin/safaridriver', '-p', str(port)], stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 15
                while True:
                    try:
                        webdriver(base, 'GET', '/status')
                        break
                    except (OSError, ValueError):
                        if process.poll() is not None or time.monotonic() > deadline:
                            raise RuntimeError('safaridriver unavailable; do not enable settings automatically')
                        time.sleep(.1)
                result = webdriver(base, 'POST', '/session', {'capabilities': {'alwaysMatch': {'browserName': 'safari', 'platformName': 'mac'}}})
                session = result['sessionId']
                validate_capabilities(result['capabilities'])
                metadata['capabilities'] = recording_capabilities(result['capabilities'])
                metadata['captureMethod'] = 'Actual Safari sampled viewport PNGs; genuine elapsed intervals duplicated at 15fps encoding'
                metadata['networkBoundary'] = 'Loopback asset server, NOT proven Safari network isolation'
                driver = Driver(base, session, stage)
                driver.native_drag_handoff = args.native_drag_handoff
                driver.call('POST', '/url', {'url': url})
                wait_visible_owned_window(driver, metadata)
                wait_ready(driver)
                for _ in range(3):
                    size = driver.evaluate('return {width:innerWidth,height:innerHeight};')
                    if size == VIEWPORT:
                        break
                    rect = driver.call('GET', '/window/rect')
                    driver.call('POST', '/window/rect', {'width': rect['width'] + 1140 - size['width'], 'height': rect['height'] + 720 - size['height']})
                if driver.evaluate('return {width:innerWidth,height:innerHeight};') != VIEWPORT:
                    raise RuntimeError('Native Safari viewport sizing unqualified')
                metadata['userAgent'] = driver.evaluate('return navigator.userAgent;')
                driver.capture()  # Fail before storyboard if screenshot endpoint unavailable.
                if getattr(args, 'full_functionality', False):
                    from full_demo import full_storyboard
                    full_storyboard(driver, fixture, args.duration, driver.pause, metadata)
                else:
                    storyboard(driver, fixture, args.duration, driver.pause)
                driver.capture()
                durations = frame_durations(driver.samples)
                cadence = (len(driver.samples) - 1) / sum(durations)
                metadata['sampling'] = {'samples': driver.samples, 'averageActualFramesPerSecond': cadence,
                                        'maximumIntervalSeconds': max(durations), 'minimumIntervalSeconds': min(durations)}
                # Preserve cadence raw JSON even when capture fails qualification.
                if cadence < 2:
                    raise RuntimeError('Actual Safari sampling below 2 frames/s; no final clip')
                concat = stage / 'frames.ffconcat'
                lines = ['ffconcat version 1.0']
                for sample, duration in zip(driver.samples, durations):
                    lines += [f"file '{sample['name']}'", f'duration {duration:.9f}']
                lines += [f"file '{driver.samples[-1]['name']}'"]
                concat.write_text('\n'.join(lines) + '\n')
                encode(ffmpeg, ffprobe, ['-safe', '1', '-f', 'concat', '-i', str(concat)], args.output, metadata)
            finally:
                if driver:
                    metadata['rawSamples'] = driver.samples
                    metadata['contextMenuInputs'] = driver.context_menu_inputs
                    metadata['nativeDragInputs'] = driver.native_drag_inputs
                try:
                    if session:
                        webdriver(base, 'DELETE', '/session/' + session)
                finally:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)


if __name__ == '__main__':
    main()
