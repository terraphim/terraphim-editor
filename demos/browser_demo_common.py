"""Fail-closed capture primitives; no browser side effects on import."""
import argparse
import contextlib
import functools
import hashlib
import http.server
import json
import pathlib
import re
import shutil
import subprocess
import tempfile
import threading
import time
import uuid
import stat
from datetime import date

ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRATCH = pathlib.Path('/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode')
VIEWPORT = {'width': 1140, 'height': 720}
PROMPTS = [
    'This synthetic draft contains Unicode before a local knowledge-graph term.',
    'Select the term and request its local alternatives with the visible controls.',
    'Choose eraser in the alternatives panel; only the selected term changes.',
    'One undo restores the exact draft, including all Unicode characters.',
]
CAPTION = """
let el = document.getElementById('demo-caption');
if (!el) { el = document.createElement('div'); el.id = 'demo-caption';
el.style.cssText = 'position:fixed;bottom:8px;right:12px;width:700px;pointer-events:none;'+
'background:#101820ee;color:white;padding:8px;font:16px system-ui;z-index:99999';
document.body.appendChild(el); } el.textContent = arguments[0]; return true;
"""
BODY = 'return window.terraphimEditor.surface.getText();'
READY = 'return !!(window.terraphimEditor && window.wasmBindings && window.terraphimEditor.surface);'
SETUP = """
const e = window.terraphimEditor;
e.loadThesaurus(JSON.stringify(arguments[0].role));
e.surface.setText(arguments[0].body);
e.rolegraph;
return true;
"""


def native_boundary_keys(keys, is_mac):
    """Use native document-boundary chords; Control shortcuts stay unchanged."""
    if is_mac and keys in (["\ue009","\ue011"],["\ue009","\ue010"],["\ue009","\ue008","\ue010"]):
        return ["\ue03d"] + (["\ue008"] if "\ue008" in keys else []) + (["\ue013"] if "\ue011" in keys else ["\ue015"])
    return list(keys)


def run(args, **kwargs):
    with tempfile.TemporaryFile(dir=SCRATCH) as stdout, tempfile.TemporaryFile(dir=SCRATCH) as stderr:
        subprocess.run(args, check=True, stdout=stdout, stderr=stderr, timeout=120, **kwargs)
        stdout.seek(0)
        content = stdout.read(262145)
        if len(content) > 262144:
            raise RuntimeError('Subprocess stdout exceeds bounded metadata limit')
        return content.decode()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def output_paths(output):
    """Only the validated, exact reservation pair may be excluded from source."""
    output = output.absolute()
    expected_parent = ROOT / 'demos/out'
    if output.parent != expected_parent or output.resolve() != output:
        raise ValueError('Output path must not escape through a symlink')
    for parent in [ROOT / 'demos', expected_parent]:
        if parent.is_symlink():
            raise ValueError('Symlinked evidence directory refused')
    sidecar = output.with_suffix('.json')
    if output.is_symlink() or sidecar.is_symlink():
        raise ValueError('Symlinked evidence path refused')
    return output, sidecar


def source_identity(reserved_output=None):
    scopes = ['src', 'public', 'crates', 'scripts', 'demos', 'Cargo.toml',
              'Cargo.lock', 'index.html', 'Trunk.toml']
    excluded = [] if reserved_output is None else [p.relative_to(ROOT).as_posix() for p in output_paths(reserved_output)]
    pathspecs = scopes + [':(top,exclude,literal)' + name for name in excluded]
    names = run(['git', 'ls-files', '-z', '--', *pathspecs], cwd=ROOT).split('\0')
    return {'head': run(['git', 'rev-parse', 'HEAD'], cwd=ROOT).strip(),
            'status': run(['git', 'status', '--porcelain', '-z', '--untracked-files=all', '--', *pathspecs], cwd=ROOT),
            'excludedReservationPaths': excluded,
            'files': {n: digest(ROOT / n) for n in names if n and (ROOT / n).is_file()}}


def safe_name(name):
    p = pathlib.PurePosixPath(name)
    if not name or '\\' in name or p.is_absolute() or any(x in {'', '.', '..'} for x in name.split('/')):
        raise ValueError('Unsafe manifest path')
    return p


def verified_assets(delivery, manifest):
    if manifest.get('schema') != 1 or not isinstance(manifest.get('assets'), dict):
        raise ValueError('Expected schema 1 with assets path -> sha256')
    assets = manifest['assets']
    required = {'index.html', 'terraphim_editor.js', 'terraphim_editor_bg.wasm',
                'terraphim-editor.min.js', 'terraphim-editor.min.css', 'js/rolegraph-worker.js',
                'rolegraph/terraphim_rolegraph_browser.js',
                'rolegraph/terraphim_rolegraph_browser_bg.wasm'}
    if not required <= assets.keys():
        raise ValueError('Incomplete production manifest')
    captured = {}
    for name, expected in assets.items():
        p = delivery / safe_name(name)
        if not p.resolve().is_relative_to(delivery.resolve()) or p.is_symlink():
            raise ValueError('Manifest symlink/escape refused')
        if p.suffix not in {'.html', '.js', '.wasm', '.css', '.json', '.woff', '.woff2', '.ttf', '.svg'} and not p.name.lower().startswith(('license', 'licence')):
            raise ValueError('Non-production manifest entry')
        if p.suffix == '.html' and name != 'index.html':
            raise ValueError('Test/example HTML refused')
        data = p.read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError('Asset hash mismatch: ' + name)
        captured[name] = data
    return captured


def verify_imports(captured):
    """Check static local imports in the exact stage, never the wider delivery."""
    import posixpath
    from html.parser import HTMLParser
    from urllib.parse import urlsplit
    def reference(owner, value):
        if value.startswith(('data:', '#')):
            return
        parsed = urlsplit(value)
        if parsed.scheme or parsed.netloc or value.startswith('/'):
            raise ValueError('Non-relative production dependency refused: ' + value)
        target = posixpath.normpath(posixpath.join(posixpath.dirname(owner), parsed.path))
        safe_name(target)
        if target not in captured:
            raise ValueError('Manifest omits production dependency: ' + target)
    class Parser(HTMLParser):
        def handle_starttag(self, tag, attrs):
            attrs = dict(attrs)
            for attribute in (['src'] if tag in {'script', 'img', 'source'} else ['href'] if tag == 'link' else []):
                if attrs.get(attribute):
                    reference('index.html', attrs[attribute])
    Parser().feed(captured['index.html'].decode())
    for name, content in captured.items():
        if name.endswith('.js'):
            text = content.decode()
            patterns = [r'''(?:from\s*|import\s*\(?\s*)["'](\.[^"']+)["']''',
                        r'''new\s+URL\(\s*["'](\.[^"']+)["']''']
            for pattern in patterns:
                for value in re.findall(pattern, text):
                    reference(name, value)
        elif name.endswith('.css'):
            for value in re.findall(r'''url\(\s*["']?([^\s"')]+)''', content.decode()):
                reference(name, value)


def tools():
    ffmpeg, ffprobe = shutil.which('ffmpeg'), shutil.which('ffprobe')
    if not ffmpeg or not ffprobe or 'libx264' not in run([ffmpeg, '-hide_banner', '-encoders']):
        raise RuntimeError('Existing FFmpeg libx264 and ffprobe required; no installation')
    return ffmpeg, ffprobe


def encoder_identity(registry_path):
    path = pathlib.Path(registry_path).absolute()
    target = path.resolve(strict=True)
    if not target.is_file():
        raise RuntimeError('Recording encoder must resolve to an existing binary')
    return {'registryPath': str(path), 'resolvedTarget': str(target),
            'symlinkTarget': str(path.readlink()) if path.is_symlink() else None,
            'binarySha256': digest(target),
            'version': run([str(target), '-version']).splitlines()[0],
            'provenance': 'externally supplied system encoder via symlink' if path.is_symlink() else 'existing registry executable; not an asserted bundled build'}


def parser(browser=False):
    p = argparse.ArgumentParser(description='Final-assets-only synthetic alternatives recording')
    if browser:
        p.add_argument('--browser', choices=['chrome', 'firefox'], required=True)
    p.add_argument('--delivery', type=pathlib.Path, required=True)
    p.add_argument('--manifest', type=pathlib.Path, required=True)
    p.add_argument('--output', type=pathlib.Path, required=True)
    p.add_argument('--duration', type=int, choices=range(60, 1801), default=64)
    p.add_argument('--full-functionality', action='store_true', help='Run approved chaptered output-checking walkthrough (180–1800 seconds)')
    p.add_argument('--authorised-final-recording', action='store_true', required=True)
    return p


@contextlib.contextmanager
def inputs(args, browser):
    ffmpeg, ffprobe = tools()
    if getattr(args, 'full_functionality', False) and args.duration < 180:
        raise ValueError('Full walkthrough requires at least 180 seconds')
    output = args.output.absolute()
    if output.parent != ROOT / 'demos/out' or not re.fullmatch(
            rf'{browser}-alternatives-guided-demo-{date.today().isoformat()}(?:-[a-z0-9-]+)?\.mp4', output.name):
        raise ValueError('Exact dated new filename in demos/out required')
    output_paths(output)
    if output.exists() or output.with_suffix('.json').exists():
        raise FileExistsError('Never overwrite evidence')
    identity = source_identity(output)
    if identity['status']:
        raise RuntimeError('Scoped source must be committed and clean before recording')
    manifest = json.loads(args.manifest.read_text())
    if manifest.get('sourceHead') != identity['head']:
        raise RuntimeError('Manifest sourceHead must identify current committed HEAD')
    data = verified_assets(args.delivery.resolve(), manifest)
    verify_imports(data)
    run(['python3', str(ROOT / 'scripts/ci/rolegraph-production-check.py'), 'assets', str(args.delivery.resolve())], cwd=ROOT)
    fixture_path = ROOT / 'demos/fixtures/alternatives.json'
    fixture = json.loads(fixture_path.read_text())
    metadata = {'browser': browser, 'recordingUUID': str(uuid.uuid4()), 'sourceIdentity': identity, 'manifest': manifest,
                'manifestSha256': digest(args.manifest), 'fixtureSha256': digest(fixture_path),
                'viewport': VIEWPORT, 'storyboard': PROMPTS, 'fullFunctionality': getattr(args, 'full_functionality', False), 'status': 'incomplete'}
    output.parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='demo-', dir=SCRATCH) as tmp:
        stage = pathlib.Path(tmp)
        for name, content in data.items():
            dest = stage / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(content)
        run(['python3', str(ROOT / 'scripts/ci/rolegraph-production-check.py'), 'graph-assets', str(stage / 'rolegraph')], cwd=ROOT)
        class Handler(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_GET(self):
                from urllib.parse import unquote, urlsplit
                name = unquote(urlsplit(self.path).path).lstrip('/') or 'index.html'
                if name not in data:
                    self.send_error(404)
                    return
                super().do_GET()
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(stage)))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        reserved = False
        try:
            # Exclusive reservation prevents collisions, even after preflight.
            with output.open('xb'):
                pass
            reserved = True
            reservation = output.lstat()
            metadata['outputReservation'] = {'device': reservation.st_dev, 'inode': reservation.st_ino}
            yield stage, f'http://127.0.0.1:{server.server_port}/', fixture, metadata, ffmpeg, ffprobe
            if source_identity(output) != identity or digest(args.manifest) != metadata['manifestSha256']:
                raise RuntimeError('Source/manifest drift during recording')
            if verified_assets(args.delivery.resolve(), manifest) != data:
                raise RuntimeError('Delivery drift')
            for name, content in data.items():
                if (stage / name).read_bytes() != content:
                    raise RuntimeError('Stage drift')
            current = output.lstat()
            if not stat.S_ISREG(current.st_mode) or (current.st_dev, current.st_ino) != (reservation.st_dev, reservation.st_ino):
                raise RuntimeError('Output reservation replaced during recording')
            metadata['status'] = 'complete'
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
            if reserved:
                try:
                    with output.with_suffix('.json').open('x') as evidence:
                        evidence.write(json.dumps(metadata, indent=2) + '\n')
                finally:
                    if metadata['status'] != 'complete':
                        if output.exists() or output.is_symlink():
                            current = output.lstat()
                            if stat.S_ISREG(current.st_mode) and (current.st_dev, current.st_ino) == (reservation.st_dev, reservation.st_ino):
                                output.unlink()


def wait_ready(driver):
    deadline = time.monotonic() + 30
    while not driver.evaluate(READY):
        if time.monotonic() > deadline:
            raise RuntimeError('Production editor failed to initialise')
        time.sleep(.1)


def storyboard(driver, fixture, duration, pause):
    """Setup API only; thereafter native keys/pointer controls, read-only assertions."""
    wait_ready(driver)
    driver.evaluate(SETUP, fixture)
    deadline = time.monotonic() + 30
    while not driver.evaluate('return window.terraphimEditor.graphClient.state === "ready";'):
        if time.monotonic() > deadline:
            raise RuntimeError('Actual production graph worker failed to become ready')
        time.sleep(.1)
    def body(expected):
        if driver.evaluate(BODY) != expected:
            raise AssertionError('Exact Unicode body assertion failed')
    body(fixture['body'])
    for i, text in enumerate(PROMPTS):
        driver.evaluate(CAPTION, f'{i+1}/{len(PROMPTS)} {text}')
        if i == 1:
            driver.click('.markdown-input')
            driver.keys(['\ue009', '\ue010'])  # Control+End
            for _ in fixture['term']:
                driver.keys(['\ue008', '\ue012'])  # Shift+Left
            selection = driver.evaluate('return window.terraphimEditor.surface.getSelectionOffsets();')
            end = len(fixture['body'].encode('utf-16-le')) // 2
            if selection.get('start') != end - len(fixture['term']) or selection.get('end') != end:
                raise AssertionError('Actual UTF-16 selection anchor mismatch')
            driver.keys(['\ue009', '\ue008', 'a'])  # visible alternatives panel
            driver.keys(['\ue00c'])  # Escape restores actual selected surface focus.
            driver.keys(['\ue009', '\ue008', 'g'])  # public local provider
            deadline = time.monotonic() + 5
            while True:
                lines = driver.evaluate('return Array.from(document.querySelectorAll(".te-alt-input:not(.te-alt-input--new)")).map(e => e.value);')
                if lines == [fixture['term'], fixture['replacement']]:
                    break
                if time.monotonic() > deadline:
                    raise AssertionError('Exact public alternatives order not observed')
                time.sleep(.1)
        elif i == 2:
            driver.click('.te-alt-input:not(.te-alt-input--new)')
            driver.keys(['\ue015'])  # Down chooses the next panel line
            body(fixture['body'][:-len(fixture['term'])] + fixture['replacement'])
        elif i == 3:
            driver.keys(['\ue009', 'z'])
            body(fixture['body'])
        pause(duration / len(PROMPTS))
    body(fixture['body'])


def verify_probe(probe, minimum=60, maximum=90):
    video = next(s for s in probe['streams'] if s['codec_type'] == 'video')
    seconds = float(probe['format']['duration'])
    if video['codec_name'] != 'h264' or video['pix_fmt'] != 'yuv420p' or (video['width'], video['height']) != (1140, 720) or video['r_frame_rate'] != '15/1' or not minimum <= seconds <= maximum:
        raise RuntimeError('Encoded video failed qualification')


def encode(ffmpeg, ffprobe, input_args, output, metadata):
    # Encode to fresh scratch; never ffmpeg -y over an evidence path.
    with tempfile.TemporaryDirectory(prefix='demo-encode-', dir=SCRATCH) as tmp:
        encoded = pathlib.Path(tmp) / 'encoded.mp4'
        run([ffmpeg, '-nostdin', '-v', 'error', *input_args, '-an', '-c:v', 'libx264',
             '-crf', '30', '-pix_fmt', 'yuv420p', '-r', '15', '-movflags', '+faststart', str(encoded)])
        probe = json.loads(run([ffprobe, '-v', 'error', '-show_format', '-show_streams', '-of', 'json', str(encoded)]))
        verify_probe(probe, 180, 1800) if metadata.get('fullFunctionality') else verify_probe(probe)
        content = encoded.read_bytes()
        if content.find(b'moov') < 0 or content.find(b'moov') > content.find(b'mdat'):
            raise RuntimeError('Faststart verification failed')
        metadata.update(probe=probe, bytes=len(content), sha256=hashlib.sha256(content).hexdigest(),
                        storageReviewRequired=len(content) > 2_000_000)
        import os
        # Do not follow a replaced symlink or truncate a different inode.
        fd = os.open(output, os.O_WRONLY | os.O_NOFOLLOW)
        with os.fdopen(fd, 'wb') as destination:
            current = os.fstat(destination.fileno())
            reservation = metadata['outputReservation']
            if (current.st_dev, current.st_ino) != (reservation['device'], reservation['inode']):
                raise RuntimeError('Refusing write to replaced output reservation')
            destination.truncate(0)
            destination.write(content)
