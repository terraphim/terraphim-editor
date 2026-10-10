#!/usr/bin/env python3
"""Native macOS Safari local delivery under server-enforced external-resource CSP.

No settings changes, dependency installs, builds or WebKit substitution. Launch
only when the parent reserves the serial browser lane and freezes delivery.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import http.server
import importlib.util
import json
import re
from pathlib import Path, PurePosixPath
import socket
import subprocess
import sys
import tempfile
import threading
import time
from types import MappingProxyType
from urllib.parse import urlsplit
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]
CSP = ("default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; "
       "style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src 'self' data:; "
       "worker-src 'self'; img-src 'self' data:; base-uri 'self'; object-src 'none'")
URI_DIAGNOSTIC_JS = r'''
function classifyURI(value, base, allowRelative = false) {
  if (typeof value !== 'string' || value.length > 32768) return 'other';
  if (value === '') return 'empty';
  if (/^data(?::|$)/i.test(value)) return 'data';
  if (/^blob(?::|$)/i.test(value)) return 'blob';
  if (['inline','eval','wasm-eval'].includes(value.toLowerCase())) return value.toLowerCase();
  if (!allowRelative && !/^(?:https?|wss?):/i.test(value)) return 'other';
  try {
    const url = new URL(value, base), own = new URL(base);
    if (url.origin === own.origin) return 'self';
    if (['https://connect.denied.invalid','https://script.denied.invalid','https://style.denied.invalid'].includes(url.origin)) return 'negative-control';
    if (['ws:','wss:'].includes(url.protocol)) return 'websocket';
    if (['http:','https:'].includes(url.protocol)) return 'external-network';
  } catch (_) {}
  return 'other';
}
function negativeOrigin(value) {
  if (typeof value !== 'string' || value.length > 32768) return '';
  try {const origin = new URL(value).origin;
    return ['https://connect.denied.invalid','https://script.denied.invalid','https://style.denied.invalid'].includes(origin) ? origin : '';
  } catch (_) {return '';}
}
function instrumentFetch(window, document) {
  const original = window.fetch;
  const counts = {production:{},negative:{}};
  window.__offlineFetchCounts = counts;
  if (typeof original !== 'function') return;
  window.fetch = function(input, ...args) {
    let category = 'other';
    try {
      const value = typeof input === 'string' ? input :
        input instanceof URL ? input.href : input instanceof Request ? input.url : null;
      category = classifyURI(value, document.baseURI, true);
    } catch (_) {}
    const lane = ['negative-fetch','negative-module','negative-stylesheet'].includes(window.__offlineProbeStep) ? 'negative' : 'production';
    counts[lane][category] = Math.min(1000, (counts[lane][category] || 0) + 1);
    return original.call(this, input, ...args);
  };
}
'''
EARLY = '<script>(() => {' + URI_DIAGNOSTIC_JS + r'''
window.__offlineViolations = []; window.__offlineErrors = [];
addEventListener('securitypolicyviolation', e => {
  const blocked = classifyURI(e.blockedURI, document.baseURI);
  window.__offlineViolations.push({directive:e.effectiveDirective, blocked,
    // Only fixed public negative-control origins survive this compatibility
    // field. No raw blocked URI, query, payload or source location is retained.
    blockedURI:negativeOrigin(e.blockedURI), trusted:e.isTrusted});
});
addEventListener('error', () => window.__offlineErrors.push('error'));
addEventListener('unhandledrejection', () => window.__offlineErrors.push('unhandledrejection'));
instrumentFetch(window, document);
})();</script>'''
MIME = {'.html': 'text/html; charset=utf-8', '.js': 'application/javascript',
        '.css': 'text/css', '.wasm': 'application/wasm', '.svg': 'image/svg+xml',
        '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.json': 'application/json'}
COMPONENTS = {'input', 'icon', 'button', 'button-group', 'tooltip', 'split-panel', 'dialog', 'divider', 'badge'}
STEPS = {'font-glyphs', 'help-dialog', 'toolbar-formatting', 'formatted-persistence',
          'production-csp-runtime-check', 'negative-fetch', 'negative-module', 'negative-stylesheet',
          'positive-enter', 'positive-icons', 'positive-fonts', 'positive-dialog-show',
          'positive-dialog-show-resolved', 'positive-dialog-hide', 'positive-dialog-hide-resolved',
          'positive-fixture', 'positive-role', 'positive-persistence', 'positive-complete'}
PAGE_CODES = {'unknown-page-error', 'editor-not-ready', 'component-missing', 'tools-not-upgraded',
              'icon-timeout', 'icon-failed', 'icon-svg-missing', 'font-not-loaded', 'dialog-failed',
              'fixture-missing', 'save-export', 'reopen-failed', 'host-scope', 'page-semantics',
              'font-glyph-style', 'help-button', 'help-dialog-timeout', 'help-dialog', 'formatting-button',
              'formatting-result', 'formatted-export', 'formatted-reopen', 'production-policy-or-runtime',
              'external-fetch-allowed', 'external-resource-allowed', 'negative-timeout',
               'missing-csp-denial-event', 'missing-request-rejection', 'environment-window-ownership',
               'environment-document-hidden', 'environment-document-unfocused', 'environment-visibility-unavailable',
               'environment-visibility-budget-exhausted'}
PHASES = {'preflight-assets', 'capture', 'driver-version', 'driver-start', 'driver-ready',
          'session-create', 'capabilities', 'session-timeouts', 'setup-navigation', 'setup-ready',
          'navigation', 'page-ready', 'positive', 'positive-validate', 'formatting', 'negative-controls',
          'case-finalize', 'complete', 'session-delete', 'driver-stop', 'server-shutdown',
           'server-close', 'server-join', 'temporary-cleanup', 'window-ownership', 'visibility-precondition'}
DIRECTIVES = {'default-src', 'script-src', 'script-src-elem', 'script-src-attr', 'style-src',
              'style-src-elem', 'style-src-attr', 'connect-src', 'font-src', 'img-src',
              'worker-src', 'base-uri', 'object-src'}
BLOCKED = {'empty', 'self', 'negative-control', 'inline', 'eval', 'wasm-eval', 'data', 'blob', 'websocket', 'external-network', 'other'}


def sanitize_snapshot(value):
    if not isinstance(value, dict):
        return {}
    result = {}
    for key in ('editorReady', 'setupReady', 'runtimeAssetsPresent', 'dialogOpen', 'documentFocused', 'dialogPanelHidden', 'dialogBaseHidden'):
        if type(value.get(key)) is bool:
            result[key] = value[key]
    if isinstance(value.get('readyState'), str) and value['readyState'] in {'loading', 'interactive', 'complete'}:
        result['readyState'] = value['readyState']
    if isinstance(value.get('probeStep'), str) and value['probeStep'] in STEPS:
        result['probeStep'] = value['probeStep']
    for key, allowed in [('asyncBridge', {'entered', 'resolved', 'rejected'}),
                         ('visibility', {'visible', 'hidden'})]:
        if isinstance(value.get(key), str) and value[key] in allowed:
            result[key] = value[key]
    if isinstance(value.get('dialogEvents'), dict):
        result['dialogEvents'] = {key: min(1000, max(0, count)) for key, count in value['dialogEvents'].items()
                                  if key in {'show', 'afterShow', 'hide', 'afterHide'} and type(count) is int}
    for key in ('dialogAnimationStates', 'dialogOverlayAnimationStates'):
        if isinstance(value.get(key), list):
            result[key] = [state for state in value[key][:16]
                           if isinstance(state, str) and state in {'idle', 'running', 'paused', 'finished'}]
    if type(value.get('runtimeErrorCount')) is int:
        result['runtimeErrorCount'] = min(1000, max(0, value['runtimeErrorCount']))
    if isinstance(value.get('components'), dict):
        result['components'] = {key: flag for key, flag in value['components'].items()
                                if key in COMPONENTS and type(flag) is bool}
    if isinstance(value.get('csp'), list):
        result['csp'] = [{key: item[key] for key in ('directive', 'blocked', 'trusted')}
                         for item in value['csp'][:32] if isinstance(item, dict)
                         and isinstance(item.get('directive'), str) and item['directive'] in DIRECTIVES
                         and isinstance(item.get('blocked'), str) and item['blocked'] in BLOCKED
                         and type(item.get('trusted')) is bool]
    for key in ('fonts', 'icons'):
        if isinstance(value.get(key), list):
            allowed = {'solid', 'regular', 'brands'} if key == 'fonts' else {
                'type-bold', 'type-italic', 'code', 'link', 'type-h1', 'type-h2', 'type-h3', 'type-underline', 'question-circle'}
            result[key] = [{field: item[field] for field in ('name', 'svg', 'faceLoaded', 'familyMatches', 'weightMatches', 'glyphMatches', 'widthPositive')
                            if field == 'name' or type(item.get(field)) is bool}
                           for item in value[key][:32] if isinstance(item, dict) and isinstance(item.get('name'), str) and item['name'] in allowed]
    if isinstance(value.get('fetchCounts'), dict):
        result['fetchCounts'] = {lane: {key: min(1000, max(0, count)) for key, count in counts.items()
                                       if key in BLOCKED and type(count) is int}
                                 for lane, counts in value['fetchCounts'].items()
                                 if lane in {'production', 'negative'} and isinstance(counts, dict)}
    return result


class OracleError(RuntimeError):
    def __init__(self, code, diagnostics=None):
        self.code = code if isinstance(code, str) and code in PAGE_CODES else 'unknown-page-error'
        self.diagnostics = sanitize_snapshot(diagnostics)
        super().__init__(self.code)


class RequestError(RuntimeError):
    """Finite operation and failure code only; never persist driver text/route."""
    def __init__(self, request, error):
        self.request = request
        self.code = error_record(error, 'positive')['code']
        super().__init__(self.code)


def diagnostic_request(safari, base, route, suffix, payload, request, method='POST'):
    try:
        return safari.webdriver(base, method, route + suffix, payload)
    except Exception as error:
        raise RequestError(request, error) from None


def error_record(error, phase, case=None):
    known = {'driver-exited', 'driver-startup', 'page-readiness', 'case-budget', 'positive-counts',
             'negative-controls', 'driver-kill-required', 'server-thread', 'delivery-directory',
              'delivery-symlink', 'delivery-drift', 'delivery-incomplete', 'vendor-incomplete', 'index-head-required',
              'positive-diagnostic-source-contract'}
    code = error.code if isinstance(error, (OracleError, RequestError)) else (str(error) if str(error) in known else 'unclassified')
    match = re.match(r'^WebDriver(?: HTTP [0-9]{3})?: (script timeout|timeout|javascript error|invalid session id|session not created|unknown error|unsupported operation|unknown command|no such window)(?::|$)', str(error))
    if match:
        code = 'webdriver-' + match.group(1).replace(' ', '-')
    classes = {'OracleError', 'RuntimeError', 'TimeoutError', 'ValueError', 'CalledProcessError',
               'TimeoutExpired', 'OSError', 'URLError', 'HTTPError', 'RequestError'}
    record = {'phase': phase if phase in PHASES else 'unknown-phase',
            'case': case if case in {'fullPage', 'embed', 'setup'} else None,
             'errorClass': type(error).__name__ if type(error).__name__ in classes else 'Exception', 'code': code}
    if isinstance(error, RequestError) and error.request in {'execute-async', 'set-script-timeout',
            'get-window-handle', 'get-window-handles', 'switch-owned-window', 'set-owned-window-rect', 'visibility-snapshot'}:
        record['request'] = error.request
    return record


SNAPSHOT = '() => {' + URI_DIAGNOSTIC_JS + r'''
  const wrapper = window.offlineEmbed, editor = wrapper ? wrapper.markdownEditor : window.terraphimEditor;
  const scope = wrapper ? wrapper.container : document.getElementById('app');
  const dialog = scope && scope.querySelector('.shortcuts-dialog');
  const panel = dialog && dialog.shadowRoot && dialog.shadowRoot.querySelector('[part="panel"]');
  const overlay = dialog && dialog.shadowRoot && dialog.shadowRoot.querySelector('[part="overlay"]');
  const dialogBase = dialog && dialog.shadowRoot && dialog.shadowRoot.querySelector('[part="base"]');
  let dialogAnimationStates = [];
  let dialogOverlayAnimationStates = [];
  try {dialogAnimationStates = panel ? panel.getAnimations().slice(0,16).map(a => a.playState) : [];} catch (_) {}
  try {dialogOverlayAnimationStates = overlay ? overlay.getAnimations().slice(0,16).map(a => a.playState) : [];} catch (_) {}
  const components = Object.fromEntries(['input','icon','button','button-group','tooltip','split-panel','dialog','divider','badge']
    .map(name => [name, Boolean(customElements.get('sl-' + name))]));
  const icons = [...(scope ? scope.querySelectorAll('sl-icon') : [])].slice(0,32)
    .map(icon => ({name:icon.name, svg:Boolean(icon.shadowRoot && icon.shadowRoot.querySelector('svg'))}));
  const fonts = [];
  if (scope) for (const [name, classes, family, weight, glyph] of [
    ['solid','fa-solid fa-floppy-disk','Font Awesome 6 Free','900','\uf0c7'],
    ['regular','fa-regular fa-keyboard','Font Awesome 6 Free','400','\uf11c'],
    ['brands','fa-brands fa-markdown','Font Awesome 6 Brands','400','\uf60f']]) {
    const node = document.createElement('i'); node.className = classes; scope.appendChild(node);
    try { const style = getComputedStyle(node), pseudo = getComputedStyle(node,'::before');
      fonts.push({name, faceLoaded:document.fonts.check(weight + ' 16px "' + family + '"',glyph),
        familyMatches:style.fontFamily.includes(family), weightMatches:style.fontWeight === weight,
        glyphMatches:pseudo.content.replace(/^["']|["']$/g,'') === glyph, widthPositive:node.getBoundingClientRect().width > 0});
    } finally {node.remove();}
  }
  const csp = (window.__offlineViolations || []).slice(0,32).map(event => {
    const categories = ['empty','self','negative-control','inline','eval','wasm-eval','data','blob','websocket','external-network','other'];
    const blocked = categories.includes(event.blocked) ? event.blocked : classifyURI(event.blockedURI, document.baseURI);
    return {directive:event.directive, blocked, trusted:event.trusted === true};
  });
  return {readyState:document.readyState, editorReady:Boolean(editor && editor.surface),
    setupReady:window.__offlineSetup === true, runtimeAssetsPresent:Boolean(window.TerraphimRuntimeAssets),
    dialogOpen:Boolean(scope && scope.querySelector('.shortcuts-dialog') && scope.querySelector('.shortcuts-dialog').open),
    runtimeErrorCount:(window.__offlineErrors || []).length, probeStep:window.__offlineProbeStep,
    components, icons, fonts, csp, fetchCounts:window.__offlineFetchCounts,
    asyncBridge:window.__offlineAsyncBridge, visibility:document.visibilityState,
    documentFocused:document.hasFocus(), dialogPanelHidden:panel ? panel.hidden : undefined,
    dialogBaseHidden:dialogBase ? dialogBase.hidden : undefined,
    dialogEvents:window.__offlineDialogEvents, dialogAnimationStates, dialogOverlayAnimationStates};
}'''


DIALOG_DIAGNOSTIC_JS = r'''
function observeDiagnosticDialog(dialog) {
  const counts = {show:0, afterShow:0, hide:0, afterHide:0};
  window.__offlineDialogEvents = counts;
  for (const [event, key] of [['sl-show','show'],['sl-after-show','afterShow'],
                             ['sl-hide','hide'],['sl-after-hide','afterHide']]) {
    dialog.addEventListener(event, e => {
      if (e.target === dialog) counts[key] = Math.min(1000, counts[key] + 1);
    });
  }
}
'''


def instrument_positive_probe(probe):
    """Annotate a local copy; fail closed if reviewed shared operations drift."""
    edits = [
        ('async () => {', "async () => {\n  window.__offlineProbeStep = 'positive-enter';\n" + DIALOG_DIAGNOSTIC_JS),
        ('  const holders = icons.map', "  window.__offlineProbeStep = 'positive-icons';\n  const holders = icons.map"),
        ('  for (const [face, glyph]', "  window.__offlineProbeStep = 'positive-fonts';\n  for (const [face, glyph]"),
        ('  await dialog.show();', "  observeDiagnosticDialog(dialog);\n  window.__offlineProbeStep = 'positive-dialog-show';\n  await dialog.show();\n  window.__offlineProbeStep = 'positive-dialog-show-resolved';"),
        ('  await dialog.hide();', "  window.__offlineProbeStep = 'positive-dialog-hide';\n  await dialog.hide();\n  window.__offlineProbeStep = 'positive-dialog-hide-resolved';"),
        ('  const role = await fetch(', "  window.__offlineProbeStep = 'positive-fixture';\n  const role = await fetch("),
        ('  const roleSummary =', "  window.__offlineProbeStep = 'positive-role';\n  const roleSummary ="),
        ('  const saved =', "  window.__offlineProbeStep = 'positive-persistence';\n  const saved ="),
        ('  return {ok:true, components:', "  window.__offlineProbeStep = 'positive-complete';\n  return {ok:true, components:"),
    ]
    for before, after in edits:
        if probe.count(before) != 1:
            raise ValueError('positive-diagnostic-source-contract')
        probe = probe.replace(before, after, 1)
    return probe


def page_error_map():
    mapping = {code: code for code in PAGE_CODES}
    mapping.update({'real editor not initialized': 'editor-not-ready', 'tools not upgraded': 'tools-not-upgraded',
                    'missing real SVG': 'icon-svg-missing', 'dialog failed': 'dialog-failed', 'fixture missing': 'fixture-missing',
                    'save/export changed canonical text': 'save-export', 'reopen failed': 'reopen-failed',
                    'host scope mutated': 'host-scope', 'full page semantics': 'page-semantics'})
    for name in COMPONENTS:
        mapping['undefined ' + name] = 'component-missing'
    for name in ('type-bold', 'type-italic', 'code', 'link', 'type-h1', 'type-h2', 'type-h3', 'type-underline', 'question-circle'):
        mapping['icon timeout: ' + name] = 'icon-timeout'
        mapping['icon failed: ' + name] = 'icon-failed'
    for face in ('900 16px "Font Awesome 6 Free"', '400 16px "Font Awesome 6 Free"', '400 16px "Font Awesome 6 Brands"'):
        mapping['font not loaded: ' + face] = 'font-not-loaded'
    return mapping


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


def safe_path(path):
    # Exact in-memory routes only; never translate arbitrary client paths to FS.
    if '%' in path or '\\' in path or not path.startswith('/') or path.startswith('//'):
        return False
    return all(part not in ('', '.', '..') for part in path[1:].split('/'))


class PolicyServer(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, payload):
        self.payload = MappingProxyType(dict(payload))
        super().__init__(('127.0.0.1', 0), PolicyHandler)


class PolicyHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.respond(False)

    def do_HEAD(self):
        self.respond(True)

    def respond(self, head):
        path = urlsplit(self.path).path
        data = self.server.payload.get(path) if safe_path(path) else None
        self.send_response(200 if data is not None else 404)
        self.send_header('Content-Security-Policy', CSP)
        self.send_header('Cache-Control', 'no-store, max-age=0')
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.send_header('Content-Type', MIME.get(PurePosixPath(path).suffix, 'application/octet-stream'))
        self.send_header('Content-Length', str(len(data) if data is not None else 0))
        self.end_headers()
        if data is not None and not head:
            self.wfile.write(data)


def capture(delivery, token):
    """Capture immutable bytes, refusing links/drift; server serves only manifest keys."""
    if delivery.is_symlink() or not delivery.is_dir():
        raise ValueError('delivery-directory')
    if any(path.is_symlink() for path in delivery.rglob('*')):
        raise ValueError('delivery-symlink')
    vendor_manifest = json.loads((delivery / 'vendor/manifest.json').read_text())
    vendor_paths = {'vendor/manifest.json'} | {'vendor/' + item['path'] for item in vendor_manifest['files']}
    def selected():
        paths = []
        for path in delivery.rglob('*'):
            if path.is_symlink():
                raise ValueError('delivery-symlink')
            relative = path.relative_to(delivery).as_posix()
            if path.is_file() and (relative in vendor_paths or relative == 'index.html' or
                                  path.suffix in ('.js', '.wasm', '.css')):
                paths.append(path)
        return sorted(paths)
    def identity(path):
        stat = path.stat()
        return (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    paths = selected()
    before = {path: identity(path) for path in paths}
    raw = {path.relative_to(delivery).as_posix(): path.read_bytes() for path in paths}
    if paths != selected() or any(before[path] != identity(path) for path in paths):
        raise ValueError('delivery-drift')
    for name in ('index.html', 'terraphim-editor.min.js', 'terraphim-editor.min.css',
                 'terraphim_editor.js', 'terraphim_editor_bg.wasm'):
        if name not in raw:
            raise ValueError('delivery-incomplete')
    if not vendor_paths.issubset(raw):
        raise ValueError('vendor-incomplete')
    manifest = {name: {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
                for name, data in raw.items()}
    payload = {f'/{token}/nested/editor/{name}': data for name, data in raw.items()}
    index = raw['index.html'].decode()
    if '<head>' not in index:
        raise ValueError('index-head-required')
    payload[f'/{token}/nested/editor/index.html'] = index.replace('<head>', '<head>' + EARLY, 1).encode()
    payload[f'/{token}/fixture/thesaurus.json'] = (ROOT / 'tests/fixtures/kg/thesaurus.json').read_bytes()
    payload[f'/{token}/host/embed.html'] = f'''<!doctype html><html lang="fr"><head>{EARLY}
<link rel="stylesheet" href="../nested/editor/terraphim-editor.min.css"></head>
<body data-host="unchanged"><main><div id="editor" style="height:600px"></div></main>
<script src="../nested/editor/terraphim-editor.min.js"></script><script type="module">
window.offlineEmbed = await TeraphimEditor.create(document.getElementById('editor'));
</script></body></html>'''.encode()
    # Establish empty test-owned origin storage before either application route.
    payload[f'/{token}/setup.html'] = (f'''<!doctype html><html><head>{EARLY}</head><body><script>
(async () => {{
  localStorage.clear(); sessionStorage.clear();
  if ('serviceWorker' in navigator) {{
    for (const registration of await navigator.serviceWorker.getRegistrations()) await registration.unregister();
  }}
  if ('caches' in window) for (const key of await caches.keys()) await caches.delete(key);
  window.__offlineSetup = true;
}})();</script></body></html>''').encode()
    staged_manifest = {name: {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
                       for name, data in payload.items()}
    return payload, manifest, staged_manifest


FORMAT = r'''async () => {
  const wrapper = window.offlineEmbed;
  const editor = wrapper ? wrapper.markdownEditor : window.terraphimEditor;
  const scope = wrapper ? wrapper.container : document.getElementById('app');
  window.__offlineProbeStep = 'font-glyphs';
  for (const [classes, family, weight, glyph] of [
    ['fa-solid fa-floppy-disk', 'Font Awesome 6 Free', '900', '\uf0c7'],
    ['fa-regular fa-keyboard', 'Font Awesome 6 Free', '400', '\uf11c'],
    ['fa-brands fa-markdown', 'Font Awesome 6 Brands', '400', '\uf60f']]) {
    const node = document.createElement('i'); node.className = classes; scope.appendChild(node);
    try {
      const computed = getComputedStyle(node), pseudo = getComputedStyle(node, '::before');
      const content = pseudo.content.replace(/^["']|["']$/g, '');
      if (!computed.fontFamily.includes(family) || computed.fontWeight !== weight ||
          content !== glyph || node.getBoundingClientRect().width <= 0) throw new Error('font-glyph-style');
    } finally { node.remove(); }
  }
  window.__offlineProbeStep = 'help-dialog';
  const dialog = scope.querySelector('.shortcuts-dialog'), help = scope.querySelector('#show-help');
  if (!help) throw new Error('help-button');
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('help-dialog-timeout')), 3000);
    dialog.addEventListener('sl-after-show', () => {clearTimeout(timer); resolve();}, {once:true});
    help.click();
  });
  if (!dialog.open) throw new Error('help-dialog');
  await dialog.hide();
  window.__offlineProbeStep = 'toolbar-formatting';
  editor.surface.setText('format me'); editor.surface.focus();
  editor.surface.setSelectionOffsets(0, 6);
  const button = scope.querySelector('#formatting-toolbar sl-button');
  if (!button) throw new Error('formatting-button');
  button.click();
  if (editor.surface.getText() !== '**format** me') throw new Error('formatting-result');
  window.__offlineProbeStep = 'formatted-persistence';
  const saved = editor.saveDocument(), exported = editor.exportDocument();
  if (exported !== '**format** me') throw new Error('formatted-export');
  editor.openDocument(saved, 'safari-offline.md');
  if (editor.surface.getText() !== '**format** me') throw new Error('formatted-reopen');
  window.__offlineProbeStep = 'production-csp-runtime-check';
  if (window.__offlineViolations.length || window.__offlineErrors.length) throw new Error('production-policy-or-runtime');
  return {fontGlyphStyles:3, helpButton:true, toolbarFormatting:true,
    formattedSaveExportReopen:true, productionViolations:0, runtimeErrors:0};
}'''

NEGATIVE = r'''async () => {
  const controls = [
    {kind:'fetch', directive:'connect-src', url:'https://connect.denied.invalid/probe'},
    {kind:'module', directive:'script-src', url:'https://script.denied.invalid/probe.js'},
    {kind:'stylesheet', directive:'style-src', url:'https://style.denied.invalid/probe.css'}
  ];
  const results = [];
  for (const control of controls) {
    window.__offlineProbeStep = 'negative-' + control.kind;
    const before = window.__offlineViolations.length;
    let rejected = false, node;
    const request = control.kind === 'fetch'
      ? fetch(control.url).then(() => { throw new Error('external-fetch-allowed'); }, () => { rejected = true; })
      : new Promise((resolve, reject) => {
          node = document.createElement(control.kind === 'module' ? 'script' : 'link');
          if (control.kind === 'module') { node.type = 'module'; node.src = control.url; }
          else { node.rel = 'stylesheet'; node.href = control.url; }
          node.onerror = () => { rejected = true; resolve(); };
          node.onload = () => reject(new Error('external-resource-allowed'));
          document.head.appendChild(node);
        });
    let timer;
    try {
      await Promise.race([request, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('negative-timeout')), 3000); })]);
      // Safari may queue its violation event after the resource's error.
      const until = performance.now() + 1500;
      const observed = () => window.__offlineViolations.slice(before).find(e =>
        e.trusted === true && e.directive.startsWith(control.directive) &&
        e.blockedURI.startsWith(new URL(control.url).origin));
      while (!observed()) {
        if (performance.now() >= until) throw new Error('missing-csp-denial-event');
        await new Promise(resolve => setTimeout(resolve, 20));
      }
      if (!rejected) throw new Error('missing-request-rejection');
      results.push({kind:control.kind, effectiveDirective:observed().directive,
        blockedOrigin:new URL(control.url).origin, requestRejected:true, cspEvent:true});
    } finally { clearTimeout(timer); if (node) node.remove(); }
  }
  return results;
}'''


def async_function(safari, base, route, function):
    return diagnostic_request(safari, base, route, '/execute/async', {
        'script': 'window.__offlineAsyncBridge = "entered"; const done = arguments[arguments.length - 1]; const codes = new Map(Object.entries(' + json.dumps(page_error_map()) +
                  ')); const snapshot = (' + SNAPSHOT + '); (' + function +
                   ')().then(value => {window.__offlineAsyncBridge = "resolved"; done({ok:true,value});}, error => {window.__offlineAsyncBridge = "rejected"; let diagnostics = {}; '
                  'try {diagnostics = snapshot();} catch (_) {} '
                  'done({ok:false,code:codes.get(error && error.message) || "unknown-page-error",diagnostics:diagnostics});});',
        'args': []}, 'execute-async')


def checked_value(result):
    if not isinstance(result, dict) or result.get('ok') is not True:
        raise OracleError(result.get('code') if isinstance(result, dict) else 'unknown-page-error',
                          result.get('diagnostics') if isinstance(result, dict) else None)
    return result['value']


def bounded_probe(safari, base, route, function, deadline):
    remaining = int((deadline - time.monotonic()) * 1000)
    if remaining <= 0:
        raise TimeoutError('case-budget')
    diagnostic_request(safari, base, route, '/timeouts',
                       {'script': min(45000, remaining), 'implicit': 0}, 'set-script-timeout')
    return checked_value(async_function(safari, base, route, function))


def wait_ready(safari, base, route, variable, deadline):
    while time.monotonic() < deadline:
        result = safari.webdriver(base, 'POST', route + '/execute/sync', {
            'script': f'return Boolean(window.{variable});', 'args': []})
        if result is True:
            return
        time.sleep(.1)
    raise TimeoutError('page-readiness')


def validate_owned_window(current, handles, owned=None):
    # Session-only handles are held in memory, never persisted or enumerated by
    # title/URL. Refuse ambiguity rather than selecting another browsing context.
    if (not isinstance(current, str) or not 0 < len(current) <= 1024 or
            not isinstance(handles, list) or handles != [current] or
            (owned is not None and current != owned)):
        raise OracleError('environment-window-ownership')
    return current


def session_window(safari, base, route, owned=None):
    current = diagnostic_request(safari, base, route, '/window', None, 'get-window-handle', method='GET')
    handles = diagnostic_request(safari, base, route, '/window/handles', None, 'get-window-handles', method='GET')
    return validate_owned_window(current, handles, owned)


def select_owned_window(safari, base, route, owned):
    session_window(safari, base, route, owned)
    diagnostic_request(safari, base, route, '/window', {'handle': owned}, 'switch-owned-window')
    session_window(safari, base, route, owned)


VISIBILITY_SCRIPT = 'return {visibility: document.visibilityState, documentFocused: document.hasFocus()};'
VISIBILITY_NOTICE = ('SAFARI TEST WINDOW READY: owner may select ONLY the dedicated automation window TITLE BAR; '
                     'leave it foreground. No terminal acknowledgement. Waiting for visible and focused state.')
VISIBILITY_WAIT_SECONDS = 10


def visibility_request(base, route, timeout):
    """Read-only polling transport, capped by the remaining readiness budget.

    The shared driver transport has a fixed 65-second socket timeout, so using
    it for a ten-second readiness wait would defeat this bounded precondition.
    """
    request = urllib.request.Request(base + route + '/execute/sync',
        data=json.dumps({'script': VISIBILITY_SCRIPT, 'args': []}).encode(),
        method='POST', headers={'Content-Type': 'application/json'})
    try:
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                body = json.load(response)
        except urllib.error.HTTPError as error:
            body = json.loads(error.read())
        value = body.get('value')
        if isinstance(value, dict) and value.get('error'):
            raise RuntimeError('WebDriver: ' + str(value['error']))
        return value
    except (TimeoutError, socket.timeout):
        raise RequestError('visibility-snapshot', RuntimeError('WebDriver: timeout')) from None
    except Exception as error:
        raise RequestError('visibility-snapshot', error) from None


def visibility_code(value):
    if (not isinstance(value, dict) or value.get('visibility') not in ('visible', 'hidden') or
            type(value.get('documentFocused')) is not bool):
        return 'environment-visibility-unavailable'
    if value['visibility'] != 'visible':
        return 'environment-document-hidden'
    if not value['documentFocused']:
        return 'environment-document-unfocused'
    return None


def wait_visible_window(base, route, case_deadline):
    deadline = min(case_deadline, time.monotonic() + VISIBILITY_WAIT_SECONDS)
    print(VISIBILITY_NOTICE, flush=True)
    value = {}
    while time.monotonic() < deadline:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        try:
            value = visibility_request(base, route, min(5, remaining))
        except RequestError:
            if time.monotonic() >= deadline:
                break
            raise
        # A late response must not turn an exhausted precondition into success.
        if time.monotonic() >= deadline:
            break
        code = visibility_code(value)
        if code is None:
            return sanitize_snapshot(value)
        if code == 'environment-visibility-unavailable':
            raise OracleError(code, value)
        remaining = deadline - time.monotonic()
        if remaining > 0:
            time.sleep(min(.1, remaining))
    raise OracleError('environment-visibility-budget-exhausted', value)


def visible_window_precondition(safari, base, route, owned, set_window_rect, case_deadline):
    select_owned_window(safari, base, route, owned)
    # Resize only the verified session window when support was advertised. No
    # screen coordinates, maximize/fullscreen, OS activation or personal tabs.
    if set_window_rect is True:
        diagnostic_request(safari, base, route, '/window/rect', {'width': 1280, 'height': 900}, 'set-owned-window-rect')
    session_window(safari, base, route, owned)
    return wait_visible_window(base, route, case_deadline)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('delivery', type=Path)
    parser.add_argument('--result', type=Path, required=True)
    parser.add_argument('--profile-label', required=True)
    parser.add_argument('--scratch', type=Path, default=ROOT / 'target/safari-offline-check')
    args = parser.parse_args(argv)
    if args.result.exists():
        parser.error('result exists; choose a new report path')
    safari = module('native_safari_reuse', ROOT / 'scripts/ci/rolegraph-safari-check.py')
    probes = module('offline_probe_reuse', ROOT / 'scripts/ci/editor-offline-check.py')
    result = {'ok': False, 'engine': 'native macOS Safari', 'utc': datetime.now(timezone.utc).isoformat(),
              'buildProfileLabel': args.profile_label, 'policy': CSP, 'cases': [], 'errors': [], 'progress': [],
              'harnessSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'sharedProbeSourceSha256': hashlib.sha256((ROOT / 'scripts/ci/editor-offline-check.py').read_bytes()).hexdigest(),
              'claim': 'page/resource external-origin denial through server-enforced CSP; reachable local server',
              'notClaimed': ['browser-wide or OS network isolation', 'unique Safari profile',
                             'server-unavailable navigation', 'OS file pickers', 'iPhone', 'XSS audit']}
    server = thread = driver = session = None
    base = route = None
    phase, case = 'preflight-assets', None
    def checkpoint(next_phase, next_case=None):
        nonlocal phase, case
        phase, case = next_phase, next_case
        result['progress'].append({'phase': phase, 'case': case})
    args.scratch.mkdir(parents=True, exist_ok=True)
    temporary = tempfile.TemporaryDirectory(prefix='editor-safari-offline-', dir=args.scratch)
    try:
        checkpoint('preflight-assets')
        # Real integrity/completeness checks before driver creation; no download/build.
        for checker, arguments in [
            ('runtime-assets-check.py', ['--check', str(args.delivery / 'vendor')]),
            ('rolegraph-production-check.py', ['assets', str(args.delivery)])]:
            subprocess.run([sys.executable, str(ROOT / 'scripts/ci' / checker), *arguments],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
        token = uuid.uuid4().hex
        checkpoint('capture')
        payload, source, staged = capture(args.delivery, token)
        result.update({'sourceAssets': source, 'servedAssets': staged,
                       'inputManifestSha256': hashlib.sha256(json.dumps(source, sort_keys=True).encode()).hexdigest(),
                       'cacheControl': 'no-store; UUID paths; new ephemeral local port; test-origin storage cleared'})
        checkpoint('driver-version')
        result['driverVersion'] = subprocess.check_output(['/usr/bin/safaridriver', '--version'], text=True, timeout=10).strip()
        server = PolicyServer(payload)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        with socket.socket() as port_socket:
            port_socket.bind(('127.0.0.1', 0))
            port = port_socket.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        checkpoint('driver-start')
        with (Path(temporary.name) / 'driver.log').open('w') as log:
            driver = subprocess.Popen(['/usr/bin/safaridriver', '--port', str(port)], stdout=log, stderr=log)
        deadline = time.monotonic() + 15
        checkpoint('driver-ready')
        while True:
            if driver.poll() is not None:
                raise RuntimeError('driver-exited')
            try:
                safari.webdriver(base, 'GET', '/status')
                break
            except Exception:
                if time.monotonic() >= deadline:
                    raise TimeoutError('driver-startup')
                time.sleep(.1)
        checkpoint('session-create')
        created = safari.webdriver(base, 'POST', '/session', safari.session_request())
        session = created['sessionId']
        route = '/session/' + session
        checkpoint('capabilities')
        safari.validate_capabilities(created['capabilities'])
        result['browserVersion'] = created['capabilities'].get('browserVersion')
        result['platformName'] = created['capabilities']['platformName']
        rect_capability = created['capabilities'].get('setWindowRect')
        set_window_rect = rect_capability is True
        result['windowPrecondition'] = {'setWindowRectAdvertised': rect_capability if type(rect_capability) is bool else None,
                                      'handlePolicy': 'single-session-initial-handle', 'visibilityChecks': []}
        checkpoint('window-ownership')
        owned_window = session_window(safari, base, route)
        checkpoint('session-timeouts')
        safari.webdriver(base, 'POST', route + '/timeouts', {'pageLoad': 60000, 'script': 45000, 'implicit': 0})
        origin = f'http://127.0.0.1:{server.server_port}'
        checkpoint('setup-navigation', 'setup')
        safari.webdriver(base, 'POST', route + '/url', {'url': f'{origin}/{token}/setup.html'})
        checkpoint('setup-ready', 'setup')
        wait_ready(safari, base, route, '__offlineSetup', time.monotonic() + 15)
        probe = instrument_positive_probe(probes.PROBE).replace('/fixture/thesaurus.json', f'/{token}/fixture/thesaurus.json')
        for name, path, variable in [('fullPage', 'nested/editor/index.html', 'terraphimEditor'),
                                     ('embed', 'host/embed.html', 'offlineEmbed')]:
            case_deadline = time.monotonic() + 60
            checkpoint('window-ownership', name)
            select_owned_window(safari, base, route, owned_window)
            checkpoint('navigation', name)
            safari.webdriver(base, 'POST', route + '/url', {'url': f'{origin}/{token}/{path}'})
            checkpoint('page-ready', name)
            wait_ready(safari, base, route, variable, case_deadline)
            checkpoint('visibility-precondition', name)
            visibility = visible_window_precondition(safari, base, route, owned_window, set_window_rect, case_deadline)
            result['windowPrecondition']['visibilityChecks'].append({'case': name, **visibility})
            if time.monotonic() >= case_deadline:
                raise TimeoutError('case-budget')
            checkpoint('positive', name)
            positive = bounded_probe(safari, base, route, probe, case_deadline)
            checkpoint('positive-validate', name)
            # Do not export fixture contents or arbitrary page exception strings.
            if positive.get('ok') is not True or positive.get('components') != 9 or positive.get('icons') != 9 or positive.get('fonts') != 3:
                raise RuntimeError('positive-counts')
            checkpoint('formatting', name)
            formatting = bounded_probe(safari, base, route, FORMAT, case_deadline)
            checkpoint('negative-controls', name)
            negative = bounded_probe(safari, base, route, NEGATIVE, case_deadline)
            checkpoint('case-finalize', name)
            if len(negative) != 3 or any(not item.get('requestRejected') or not item.get('cspEvent') for item in negative):
                raise RuntimeError('negative-controls')
            if time.monotonic() > case_deadline:
                raise TimeoutError('case-budget')
            result['cases'].append({'name': name, 'components': 9, 'icons': 9, 'fontFaces': 3,
                                    'realFixtureRoleLoaded': True, 'saveExportReopen': True,
                                    'formatting': formatting, 'negativeControls': negative})
        checkpoint('complete')
        result['ok'] = True
    except Exception as error:
        result['errors'].append(error_record(error, phase, case))
        if isinstance(error, OracleError):
            result['failureDiagnostics'] = error.diagnostics
        elif session and phase in {'setup-ready', 'page-ready', 'positive', 'formatting', 'negative-controls', 'case-finalize'}:
            try:
                value = safari.webdriver(base, 'POST', route + '/execute/sync',
                                         {'script': 'return (' + SNAPSHOT + ')();', 'args': []})
                result['failureDiagnostics'] = sanitize_snapshot(value)
            except Exception:
                result['failureDiagnostics'] = {'snapshotUnavailable': True}
    finally:
        actions = []
        if session:
            actions.append(('session-delete', lambda: safari.webdriver(base, 'DELETE', route)))
        if driver:
            def stop_driver():
                if driver.poll() is None:
                    driver.terminate()
                try:
                    driver.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    driver.kill()
                    driver.wait(timeout=5)
                    raise RuntimeError('driver-kill-required')
            actions.append(('driver-stop', stop_driver))
        if server:
            actions.extend([('server-shutdown', server.shutdown), ('server-close', server.server_close)])
        if thread:
            def join_server():
                thread.join(timeout=5)
                if thread.is_alive():
                    raise RuntimeError('server-thread')
            actions.append(('server-join', join_server))
        actions.append(('temporary-cleanup', temporary.cleanup))
        for cleanup_phase, action in actions:
            try:
                action()
            except Exception as error:
                result['errors'].append(error_record(error, cleanup_phase))
        result['ok'] = result['ok'] and not result['errors'] and len(result['cases']) == 2
        args.result.parent.mkdir(parents=True, exist_ok=True)
        with args.result.open('x') as report:
            report.write(json.dumps(result, indent=2) + '\n')
        print(json.dumps({'ok': result['ok'], 'cases': result['cases'], 'errors': result['errors']}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
