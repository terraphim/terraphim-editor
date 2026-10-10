#!/usr/bin/env python3
"""Native Safari WebDriver PNG sampling, not WebKit or desktop capture."""
import base64
import socket
import struct
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


class Driver:
    def __init__(self, base, session, stage):
        self.base = base
        self.route = '/session/' + session
        self.stage = stage
        self.samples = []
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
    def keys(self, keys):
        actions = [{'type': 'keyDown', 'value': k} for k in keys]
        actions += [{'type': 'keyUp', 'value': k} for k in reversed(keys)]
        self.call('POST', '/actions', {'actions': [{'type': 'key', 'id': 'fixture-keys', 'actions': actions}]})
    def text(self, text):
        actions = []
        for c in text:
            actions += [{'type':'keyDown','value':c},{'type':'keyUp','value':c}]
        self.call('POST','/actions',{'actions':[{'type':'key','id':'public-demo-typing','actions':actions}]})
    def capture(self):
        started = time.monotonic() - self.started
        png = base64.b64decode(self.call('GET', '/screenshot'), validate=True)
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
    args = parser().parse_args()
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
