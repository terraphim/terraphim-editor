#!/usr/bin/env python3
"""Fresh local-server offline delivery checks against already-built real bytes.

Uses existing Python Playwright infrastructure; does not install/build anything.
Installed Google Chrome/cached Chromium/Firefox supported. Native Safari uses separate infrastructure:
this runner explicitly refuses to substitute Playwright WebKit for Safari.
"""
import argparse
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import re
import subprocess
import sys
import tempfile
import threading
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]


def validate_executable(engine, path, expected_sha256):
    """Explicit Firefox only; refuse aliases, missing bytes, or checksum drift.

    The checksum identifies the caller-approved launcher, not every app library.
    No executable discovery, shell invocation, download or cache rewriting.
    """
    if engine != 'firefox' or path is None or not re.fullmatch(r'[0-9a-f]{64}', expected_sha256 or ''):
        raise ValueError('explicit Firefox executable requires path and lowercase SHA-256')
    path = Path(path)
    if not path.is_absolute() or path.name != 'firefox':
        raise ValueError('explicit Firefox executable must be an absolute firefox binary path')
    if any(part.is_symlink() for part in (path, *path.parents)) or path.resolve() != path:
        raise ValueError('explicit executable path must be canonical and have no symlink ancestors')
    if not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError('explicit executable must be an existing executable regular file')
    before = path.stat()
    digest = hashlib.sha256()
    with path.open('rb') as binary:
        for chunk in iter(lambda: binary.read(1024 * 1024), b''):
            digest.update(chunk)
    after = path.stat()
    identity = lambda stat: (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    if identity(before) != identity(after) or digest.hexdigest() != expected_sha256:
        raise ValueError('explicit executable checksum mismatch or read drift')
    return {'executablePath': str(path), 'executableSha256': expected_sha256}


def launch_browser(playwright, engine, executable=None):
    """Explicit selection; Chrome uses the installed channel, never a download."""
    if executable is not None:
        validate_executable(engine, executable['executablePath'], executable['executableSha256'])
        browser = playwright.firefox.launch(headless=True, executable_path=executable['executablePath'])
        try:
            validate_executable(engine, executable['executablePath'], executable['executableSha256'])
        except Exception:
            browser.close()
            raise
        return browser
    if engine == 'chrome':
        return playwright.chromium.launch(channel='chrome', headless=True)
    if engine in ('chromium', 'firefox'):
        return getattr(playwright, engine).launch(headless=True)
    raise ValueError('unsupported Playwright engine; native Safari requires its separate runner')


def browser_metadata(browser, page, engine, executable=None):
    """Record actual runtime identity, not a guessed/version-pinned label."""
    return {'engine': engine, 'browserChannel': 'explicit-executable' if executable else ('chrome' if engine == 'chrome' else 'cached'),
            'browserVersion': browser.version, 'userAgent': page.evaluate('navigator.userAgent'),
            **(executable or {})}


def launch_failure(engine, error):
    # Browser launch errors can contain local paths/environment; do not copy them.
    return {'status': 'NOT RUN', 'engine': engine, 'phase': 'browser-launch',
            'reason': 'Requested existing browser channel/cache unavailable or launch refused; no browser installation attempted.',
            'errorClass': type(error).__name__}


def write_report(path, report):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('x') as output:
        output.write(json.dumps(report, indent=2) + '\n')

# Shoelace emits sl-load when assigning its reactive SVG, before Lit commits it.
# Keep the event and rendered-DOM oracles separate, including cached promises.
ICON_READINESS = r"""(icon, mount) => new Promise((resolve, reject) => {
  let loaded = false;
  let settled = false;
  let poll;
  const cleanup = () => {
    settled = true;
    clearTimeout(deadline); clearTimeout(poll);
    icon.removeEventListener('sl-load', onLoad);
    icon.removeEventListener('sl-error', onError);
  };
  const fail = error => { cleanup(); reject(error); };
  const onLoad = () => { loaded = true; };
  const onError = () => fail(new Error('icon failed: ' + icon.name));
  const deadline = setTimeout(() => fail(new Error('icon timeout: ' + icon.name)), 10000);
  const check = () => {
    if (settled) return;
    const svg = icon.shadowRoot?.querySelector('svg');
    const bounds = svg?.getBoundingClientRect();
    if (loaded && svg instanceof SVGSVGElement && svg.namespaceURI === 'http://www.w3.org/2000/svg'
        && svg.querySelector('path') && bounds.width > 0 && bounds.height > 0) {
      cleanup(); resolve(); return;
    }
    poll = setTimeout(check, 16);
  };
  icon.addEventListener('sl-load', onLoad);
  icon.addEventListener('sl-error', onError);
  try { mount(); check(); } catch (error) { fail(error); }
})"""

PROBE = r"""async () => {
  const iconReady = """ + ICON_READINESS + r""";
  const wrapper = window.offlineEmbed;
  const editor = wrapper ? wrapper.markdownEditor : window.terraphimEditor;
  if (!editor || !editor.surface) throw new Error('real editor not initialized');
  const names = ['input','icon','button','button-group','tooltip','split-panel','dialog','divider','badge'];
  for (const name of names) if (!customElements.get('sl-' + name)) throw new Error('undefined ' + name);
  editor.chrome.setTools(true);
  const scope = wrapper ? wrapper.container : document.getElementById('app');
  if (scope.querySelector('sl-split-panel').constructor === HTMLElement) throw new Error('tools not upgraded');
  const icons = ['type-bold','type-italic','code','link','type-h1','type-h2','type-h3','type-underline','question-circle'];
  const holders = icons.map(() => document.createElement('sl-icon'));
  try {
    await Promise.all(holders.map((icon, index) => iconReady(icon, () => {
      icon.name = icons[index]; scope.appendChild(icon);
    })));
  } finally { holders.forEach(icon => icon.remove()); }
  for (const [face, glyph] of [['900 16px "Font Awesome 6 Free"','\uf0c7'],
    ['400 16px "Font Awesome 6 Free"','\uf11c'], ['400 16px "Font Awesome 6 Brands"','\uf60f']]) {
    if (!(await document.fonts.load(face, glyph)).length || !document.fonts.check(face, glyph)) throw new Error('font not loaded: ' + face);
  }
  const dialog = scope.querySelector('.shortcuts-dialog');
  await dialog.show();
  if (!dialog.open || !dialog.shadowRoot.querySelector('[role="dialog"]')) throw new Error('dialog failed');
  await dialog.hide();
  const role = await fetch('/fixture/thesaurus.json').then(r => {if (!r.ok) throw new Error('fixture missing'); return r.json();});
  const roleSummary = editor.loadThesaurus(role);
  const text = '# Local runtime\n\nKnowledge graph writing works without CDNs.\n';
  editor.surface.setText(text);
  const saved = editor.saveDocument();
  const exported = editor.exportDocument();
  if (!saved.includes(text.trim()) || exported.trim() !== text.trim()) throw new Error('save/export changed canonical text');
  editor.openDocument(saved, 'offline-check.md');
  if (editor.surface.getText().trim() !== text.trim()) throw new Error('reopen failed');
  if (wrapper) {
    if (document.documentElement.lang !== 'fr' || document.body.dataset.host !== 'unchanged') throw new Error('host scope mutated');
  } else {
    if (document.documentElement.lang !== 'en' || document.querySelectorAll('main').length !== 1) throw new Error('full page semantics');
  }
  return {ok:true, components:names.length, icons:icons.length, fonts:3, roleSummary, saveReopenExport:true};
}"""


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('distribution', type=Path)
    parser.add_argument('--engine', choices=['chrome', 'chromium', 'firefox', 'safari'], required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--executable-path', type=Path, help='Explicit existing Firefox binary only; absolute canonical path required')
    parser.add_argument('--executable-sha256', help='Required with --executable-path; caller-approved lowercase SHA-256')
    args = parser.parse_args(argv)
    if args.report.exists():
        parser.error('report exists; choose a new path to preserve prior evidence')
    executable = None
    if args.executable_path is not None or args.executable_sha256 is not None:
        try:
            executable = validate_executable(args.engine, args.executable_path, args.executable_sha256)
        except (ValueError, OSError) as error:
            report = {'status': 'NOT RUN', 'engine': args.engine, 'phase': 'executable-validation',
                      'reason': 'Explicit executable selection invalid or checksum mismatch; no browser launched.',
                      'errorClass': type(error).__name__}
            write_report(args.report, report)
            print(json.dumps(report))
            raise SystemExit(2)
    if args.engine == 'safari':
        write_report(args.report, {'status': 'NOT RUN', 'engine': 'native Safari',
            'reason': 'Use editor-safari-offline-check.py for native CSP qualification; this runner has no Safari route-denial hook and does not substitute WebKit.'})
        raise SystemExit(2)
    from playwright.sync_api import sync_playwright
    for name in ['index.html', 'example.html', 'vendor/manifest.json', 'terraphim_editor_bg.wasm']:
        if not (args.distribution / name).is_file():
            raise SystemExit(f'missing built asset: {name}')
    subprocess.run([sys.executable, str(ROOT / 'scripts/ci/runtime-assets-check.py'),
                    '--check', str(args.distribution / 'vendor')], check=True)
    with tempfile.TemporaryDirectory(prefix='te-offline-') as temp:
        root = Path(temp)
        shutil.copytree(args.distribution, root / 'nested/editor')
        (root / 'fixture').mkdir()
        shutil.copyfile(ROOT / 'tests/fixtures/kg/thesaurus.json', root / 'fixture/thesaurus.json')
        (root / 'host').mkdir()
        (root / 'host/embed.html').write_text('''<!doctype html><html lang="fr"><head>
<link rel="stylesheet" href="../nested/editor/terraphim-editor.min.css"></head>
<body data-host="unchanged"><main><div id="editor" style="height:600px"></div></main>
<script src="../nested/editor/terraphim-editor.min.js"></script><script type="module">
window.offlineEmbed = await TeraphimEditor.create(document.getElementById('editor'));
</script></body></html>''')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(root)))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        origin = f'http://127.0.0.1:{server.server_port}'
        results = []
        try:
            with sync_playwright() as playwright:
                try:
                    browser = launch_browser(playwright, args.engine, executable)
                except Exception as error:
                    report = launch_failure(args.engine, error)
                    write_report(args.report, report)
                    print(json.dumps(report))
                    raise SystemExit(2)
                try:
                    for route_path, variable in [('/nested/editor/index.html', 'terraphimEditor'), ('/host/embed.html', 'offlineEmbed')]:
                        # New context per scenario: empty cookies/cache/storage, SW blocked.
                        context = browser.new_context(service_workers='block')
                        external, failures, errors = [], [], []
                        def intercept(route):
                            request = route.request.url
                            parsed = urlsplit(request)
                            if f'{parsed.scheme}://{parsed.netloc}' != origin:
                                external.append(request)
                                route.abort()
                            else:
                                route.continue_()
                        context.route('**/*', intercept)
                        page = context.new_page()
                        # Capture identity before application actions, per isolated scenario.
                        identity = browser_metadata(browser, page, args.engine, executable)
                        page.on('pageerror', lambda error: errors.append(str(error)))
                        page.on('response', lambda response: failures.append(response.url) if response.status >= 400 else None)
                        try:
                            page.goto(origin + route_path)
                            page.wait_for_function(f'Boolean(window.{variable})', timeout=60000)
                            result = page.evaluate(PROBE)
                            if external or failures or errors:
                                raise RuntimeError(json.dumps(dict(external=external, failures=failures, errors=errors)))
                            results.append(dict(route=route_path, result=result, externalRequests=external,
                                                freshContext=True, browser=identity))
                        finally:
                            context.close()
                finally:
                    browser.close()
        finally:
            server.shutdown()
            server.server_close()
        report = dict(status='PASS', engine=args.engine, browserVersion=browser.version,
                      browserChannel='explicit-executable' if executable else ('chrome' if args.engine == 'chrome' else 'cached'),
                      **(executable or {}),
                      delivery='reachable local server; external origins denied', scenarios=results)
        write_report(args.report, report)
        print(json.dumps(report))


if __name__ == '__main__':
    main()
