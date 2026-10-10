#!/usr/bin/env python3
"""Prepared final UI acceptance; launch requires explicit serial-freeze authority.

No builds, downloads, clipboard access or workspace server. Only approved manifest
bytes are served. Setup APIs are disclosed, not substituted for tested UI input.
"""
import argparse
from datetime import datetime, timezone
from dataclasses import dataclass
import hashlib
import http.server
import importlib.metadata
import json
from pathlib import Path, PurePosixPath
import re
import signal
import subprocess
import threading
import traceback
from urllib.parse import unquote, urlsplit

BODY = 'First paragraph.\n\nSecond paragraph.\n\nThird paragraph.'
HEADING = '# The Obvious, the Easy, and the Possible\n\nPass me a paperclip.'
TRIM_BODY = ('# Public fixture\n\nDecision and choice matter. Decision matters again. A large language model uses an llm. '
             'The coffee shop has café. Honour the paperclip and use it.\n\n'
             'Maybe perhaps basically this is really rather quite very unnecessary filler.\n\n'
             'This final public sentence and an eraser stay.')
TRIM_SAFE_BODY=TRIM_BODY.replace('Maybe perhaps basically this is really rather quite very unnecessary filler.','Perhaps this brief aside can wait.')
ROLE = {'name': 'writing-fixture', 'data': {}}
CANDIDATE_EDITOR_SHA256 = '1f7985c4ab85972bd09eea10a6d587e1b7359be451b4b510fe03acb027f5c87f'
EMBED_PATH = '__public_interaction_embed.html'
REPO = Path(__file__).absolute().parents[2]
# Exact public build/interaction cohort; no glob of user data or private corpora.
SOURCE_INPUTS = frozenset(
    ['Cargo.toml', 'Cargo.lock', 'Trunk.toml', 'index.html', 'editor_config.toml',
     'templates/editor.html', 'public/styles.css', 'public/vendor/manifest.json',
     'scripts/build-dist.sh', 'scripts/build-rolegraph.sh', 'scripts/ci/runtime-assets-check.py',
     'scripts/ci/rolegraph-production-check.py', 'scripts/ci/node/package.json',
     'scripts/ci/node/package-lock.json', 'scripts/ci/editor-interaction-check.py']
    + ['crates/terraphim_lab/kg/lab-style.json', 'crates/terraphim_lab/kg/typos.json']
    + ['public/js/' + x + '.js' for x in ['runtime-assets', 'config', 'chrome', 'indicators',
       'selection-menu', 'trim', 'lab', 'blocks', 'alternatives-panel', 'overflow', 'persistence',
       'rolegraph-storage', 'rolegraph-client', 'rolegraph-worker', 'editor', 'terraphim-editor']]
    + ['public/css/' + x + '.css' for x in ['tokens', 'write-on', 'selection-menu', 'lab', 'trim',
       'blocks', 'alternatives-panel', 'overflow', 'persistence']]
    + ['src/' + x + '.rs' for x in ['lib', 'document', 'kg', 'lab', 'protection', 'providers', 'text']]
    + ['crates/' + crate + '/Cargo.toml' for crate in ['terraphim_alternatives', 'terraphim_lab', 'terraphim_rolegraph_browser']]
    + ['crates/terraphim_alternatives/src/' + x + '.rs' for x in ['lib', 'article', 'block',
       'context', 'document', 'model', 'offset', 'reanchor', 'text', 'words']]
    + ['crates/terraphim_lab/src/' + x + '.rs' for x in ['lib', 'actions', 'cuts', 'lists',
       'offset', 'rolegraph', 'text', 'trim', 'weak']]
    + ['crates/terraphim_rolegraph_browser/src/' + x + '.rs' for x in ['lib', 'bounded_json',
       'checkpoint', 'session', 'validation', 'wire']])
for number, terms in [(1, ['decision', 'choice']), (2, ['eraser', 'rubber']),
                      (3, ['llm', 'large language model']), (4, ['café', 'coffee shop']),
                      (5, ['honour', 'privilege']), (7, ['paperclip']), (8, ['use', 'employ'])]:
    for term in terms:
        ROLE['data'][term] = {'id': number, 'nterm': terms[0]}


def embed_payload(payload):
    """Generated public fixture, not a release member or substituted editor."""
    for member in ['terraphim-editor.min.js', 'terraphim-editor.min.css',
                   'terraphim_editor.js', 'terraphim_editor_bg.wasm']:
        if member not in payload:
            raise ValueError('required real embed delivery member absent')
    if EMBED_PATH in payload:
        raise ValueError('generated public fixture would overwrite delivery member')
    html = '''<!doctype html><html lang="fr"><head><meta charset="utf-8">
<title>Public synthetic embedded acceptance</title>
<link rel="stylesheet" href="terraphim-editor.min.css"></head><body>
<nav id="public-host-nav" aria-label="Public host"><a href="#public-host-title">Public host</a></nav>
<main id="public-host-main"><h1 id="public-host-title">Public synthetic host</h1>
<div id="public-host-editor" style="height:80vh"></div></main>
<script>window.__publicHostBefore={lang:document.documentElement.lang,
main:document.getElementById('public-host-main'),nav:document.getElementById('public-host-nav'),
bodyMode:document.body.getAttribute('data-mode'),bodyOverflow:document.body.getAttribute('data-te-overflow')};</script>
<script src="terraphim-editor.min.js"></script><script type="module">
window.__publicEmbedded=await TeraphimEditor.create(document.getElementById('public-host-editor'),{value:FIXTURE});
window.terraphimEditor=window.__publicEmbedded.editor;
</script></body></html>'''.replace('FIXTURE', json.dumps(HEADING))
    return {**payload, EMBED_PATH: html.encode()}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def relative(name):
    if not isinstance(name, str) or not name or '\\' in name or '%' in name:
        raise ValueError('invalid manifest path')
    p = PurePosixPath(name)
    if p.is_absolute() or any(x in ('', '.', '..') for x in name.split('/')):
        raise ValueError('invalid manifest path')
    return name


def confined(root, name):
    root = Path(root).absolute()
    path = root.joinpath(*relative(name).split('/'))
    if any(p.is_symlink() for p in (path, *path.parents)):
        raise ValueError('symlink paths refused before reading')
    return path


def records(value):
    if not isinstance(value, list) or not value or len(value) > 1024:
        raise ValueError('invalid admitted member list')
    result = {}
    for item in value:
        if not isinstance(item, dict) or set(item) != {'path', 'sha256', 'bytes'}:
            raise ValueError('unexpected member fields')
        name = relative(item['path'])
        if name in result or not isinstance(item['sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', item['sha256']):
            raise ValueError('duplicate path or invalid digest')
        if type(item['bytes']) is not int or not 0 <= item['bytes'] <= 128 * 1024 * 1024:
            raise ValueError('invalid admitted byte size')
        result[name] = {'path': name, 'sha256': item['sha256'], 'bytes': item['bytes']}
    return result


def fingerprint(items):
    return digest(json.dumps(sorted(items.values(), key=lambda x: x['path']), sort_keys=True,
                             separators=(',', ':'), ensure_ascii=True).encode())


@dataclass(frozen=True)
class Admission:
    root: Path
    manifest: Path
    manifest_sha256: str
    source_head: str
    files: dict
    sources: dict
    source_fingerprint: str


def admit(value, delivery, manifest, head, repo=REPO):
    """Parent's independently reviewed INLINE public registration; no file reads/stat.

    This is a trust anchor, not auto-discovery from the untrusted delivery manifest.
    All paths/member sets/finite metadata are validated before ancestry inspection.
    """
    keys = {'schema', 'root', 'manifest', 'manifestSha256', 'sourceHead', 'files', 'sourceInputs', 'provenance'}
    if not isinstance(value, dict) or set(value) != keys or type(value['schema']) is not int or value['schema'] != 1:
        raise ValueError('unexpected public registration fields')
    def canonical(raw):
        if not isinstance(raw, str) or not raw.startswith('/') or '\\' in raw or '\x00' in raw:
            raise ValueError('invalid registered absolute path')
        if any(x in ('', '.', '..') for x in raw[1:].split('/')):
            raise ValueError('noncanonical registered path')
        return Path(raw)
    root, meta = canonical(value['root']), canonical(value['manifest'])
    repo = Path(repo).absolute()
    # Dedicated public staging slot; neither workspace roots nor arbitrary scratch.
    stage_parent = repo / 'target/editor-acceptance-public'
    if root.parent != stage_parent or meta.parent != stage_parent or meta.name != root.name + '.manifest.json':
        raise ValueError('unregistered or inappropriate public stage/metadata root')
    if root != Path(delivery).absolute() or meta != Path(manifest).absolute():
        raise ValueError('registered stage/manifest location mismatch')
    if value['sourceHead'] != head or not isinstance(head, str) or not re.fullmatch(r'[0-9a-f]{40}', head):
        raise ValueError('registered source identity mismatch')
    if not isinstance(value['manifestSha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', value['manifestSha256']):
        raise ValueError('invalid registered manifest digest')
    files, sources = records(value['files']), records(value['sourceInputs'])
    vendors = {'public/' + name for name in files if name.startswith('vendor/')}
    if set(sources) != SOURCE_INPUTS | vendors:
        raise ValueError('public source cohort incomplete or contains unapproved members')
    provenance = value['provenance']
    if not isinstance(provenance, dict) or set(provenance) != {'sourceFingerprint', 'files'}:
        raise ValueError('unexpected delivery provenance fields')
    source_fingerprint = fingerprint(sources)
    if provenance['sourceFingerprint'] != source_fingerprint or records(provenance['files']) != files:
        raise ValueError('approved source/output provenance association mismatch')
    # Return only validated finite fields; never serialize arbitrary caller metadata.
    return Admission(root, meta, value['manifestSha256'], head, files, sources, source_fingerprint)


def read_verified(path, record):
    before = path.stat()
    data = path.read_bytes()
    after = path.stat()
    identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if identity(before) != identity(after) or len(data) != record['bytes'] or digest(data) != record['sha256']:
        raise ValueError('admitted bytes drifted')
    return data


def admission_paths(admission, repo=REPO):
    """Validate ALL source/manifest/delivery ancestors before ANY content read."""
    sources = {name: confined(repo, name) for name in admission.sources}
    members = {name: confined(admission.root, name) for name in admission.files}
    meta = confined(admission.manifest.parent, admission.manifest.name)
    return sources, members, meta


def verify_sources(admission, paths):
    # Includes real native bindings, dependency model/Lab, UI, styles, vendor and runner.
    vendor_name = 'public/vendor/manifest.json'
    vendor_bytes = read_verified(paths[vendor_name], admission.sources[vendor_name])
    vendor = json.loads(vendor_bytes)
    if not isinstance(vendor, dict) or type(vendor.get('schema')) is not int or vendor['schema'] != 1:
        raise ValueError('invalid admitted public vendor inventory')
    vendor_records = records([{key: item[key] for key in ['path', 'sha256', 'bytes']}
                              for item in vendor['files']])
    expected = {vendor_name} | {'public/vendor/' + name for name in vendor_records}
    if {name for name in admission.sources if name.startswith('public/vendor/')} != expected:
        raise ValueError('public vendor source closure incomplete')
    if admission.files.get('vendor/manifest.json') != {**admission.sources[vendor_name], 'path': 'vendor/manifest.json'}:
        raise ValueError('public vendor manifest delivery association mismatch')
    for name, record in vendor_records.items():
        source = admission.sources['public/vendor/' + name]
        delivered = admission.files.get('vendor/' + name)
        if source != {**record, 'path': 'public/vendor/' + name} or delivered != {**record, 'path': 'vendor/' + name}:
            raise ValueError('public vendor source/delivery association mismatch')
    for name, path in paths.items():
        read_verified(path, admission.sources[name])
    return admission.source_fingerprint


def validate_manifest(root, manifest, source_head, admission):
    if not isinstance(admission, Admission) or Path(root).absolute() != admission.root:
        raise ValueError('public delivery root not registered')
    if not isinstance(manifest, dict) or set(manifest) != {'schema', 'sourceHead', 'files'}:
        raise ValueError('unexpected delivery manifest fields')
    if type(manifest.get('schema')) is not int or manifest.get('schema') != 1 or manifest.get('sourceHead') != source_head:
        raise ValueError('schema/source identity mismatch')
    if not re.fullmatch(r'[0-9a-f]{40}', source_head):
        raise ValueError('sourceHead must be full git identity')
    files = records(manifest.get('files'))
    if source_head != admission.source_head or files != admission.files:
        raise ValueError('unapproved delivery declarations')
    return files


def immutable_payload(root, manifest, source_head, admission):
    files = validate_manifest(root, manifest, source_head, admission)
    # Validate ALL declarations first: never open an earlier member of a bad list.
    declarations = {}
    for name, item in files.items():
        declarations[name] = (confined(root, name), item)
    payload = {}
    for name, (path, item) in declarations.items():
        payload[name] = read_verified(path, item)
    return payload


def handler_for(payload):
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            path = unquote(urlsplit(self.path).path)
            prefix = '/acceptance/nested/'
            name = path[len(prefix):] if path.startswith(prefix) else ''
            data = payload.get(name)
            if data is None:
                self.send_error(404)
                return
            suffix = PurePosixPath(name).suffix
            mime = {'.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css',
                    '.wasm': 'application/wasm', '.json': 'application/json',
                    '.svg': 'image/svg+xml', '.woff2': 'font/woff2'}.get(suffix, 'application/octet-stream')
            self.send_response(200)
            self.send_header('Content-Type', mime)
            self.send_header('Cache-Control', 'no-store')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
    return Handler


READ_STATE = """() => {const e=window.terraphimEditor; return {
 body:e.surface.getText(), annotations:e.annotations(), counts:e.counts(),
 exported:e.exportDocument(),nativeBody:e.requireDocumentApi().document_body()};}"""
FOCUS = """() => {let e=document.activeElement; while(e?.shadowRoot?.activeElement)e=e.shadowRoot.activeElement;
 return {tag:e?.tagName,role:e?.getAttribute('role'), hidden:!!e?.closest('[hidden],[inert]'),
 visible:!!e?.getClientRects().length, named:!!(e?.getAttribute('aria-label')||
 e?.getAttribute('aria-labelledby')||e?.textContent?.trim()||e?.labels?.length)};}"""
DRAG_OBSERVER = """() => {window.__publicDragReceipts=[];
 window.__publicDragAbort?.abort();window.__publicDragAbort=new AbortController();
 for(const type of ['dragstart','drop'])document.addEventListener(type,e=>{
 const t=e.target?.nodeType===3?e.target.parentElement:e.target;
 if(type==='drop' && t.closest?.('[contenteditable]') && window.terraphimEditor.overflow.panelDrag){
 const editor=window.terraphimEditor,drag=editor.overflow.panelDrag;
 const at=editor.surface.dropOffset(e);
 window.__publicDragReceipts.push({type:'observed-pull-anchor',dropOffset:at});
 }
 const payload=type==='drop' && e.dataTransfer ? e.dataTransfer.getData('text/plain') : null;
 const expected=window.__publicDragExpected;
 window.__publicDragReceipts.push({type,trusted:e.isTrusted,
 target:t.closest?.('.te-overflow')?'overflow':t.closest?.('[contenteditable]')?'surface':'other',
 payload:payload===expected?payload:null,payloadMatches:payload===expected,
 transfer:!!e.dataTransfer,types:e.dataTransfer?Array.from(e.dataTransfer.types):[]});},
 {capture:true,signal:window.__publicDragAbort.signal});}"""
RENDERED = r"""()=>{const root=window.terraphimEditor.surface.root;
 return Array.from(root.querySelectorAll('[data-te-decoration]'),el=>{
 const r=document.createRange();r.selectNodeContents(root);r.setEndBefore(el);
 const start=r.toString().length,style=getComputedStyle(el);
 const alpha=style.color.match(/\/\s*([\d.]+)\s*\)$/)||style.color.match(/^rgba\([^,]+,[^,]+,[^,]+,\s*([\d.]+)\)$/);
 return {start,end:start+el.textContent.length,tokens:(el.getAttribute('data-te-decoration')||'').split(/\s+/),
 ghost:el.classList.contains('te-ghost'),trim:el.classList.contains('te-trim-faded'),
 opacity:Number(style.opacity),colorAlpha:alpha?Number(alpha[1]):1};});}"""


def require(condition):
    if not condition:
        raise AssertionError('controlled public fixture invariant failed')


def utf16_slice(text, start, end):
    require(type(start) is int and type(end) is int and 0 <= start < end)
    encoded = text.encode('utf-16-le')
    require(end * 2 <= len(encoded))
    return encoded[start * 2:end * 2].decode('utf-16-le')


def protected_occurrences(text):
    """Independent enumeration of this public corpus, not a unique concept set."""
    matches = []
    for term, concept in ROLE['data'].items():
        for match in re.finditer(r'(?<!\w)' + re.escape(term) + r'(?!\w)', text, re.IGNORECASE):
            matches.append({'start': len(text[:match.start()].encode('utf-16-le')) // 2,
                            'end': len(text[:match.end()].encode('utf-16-le')) // 2,
                            'id': concept['id'], 'text': match.group()})
    return sorted(matches, key=lambda item: (item['start'], item['end'], item['id']))


def assert_protected(text, ranges):
    occurrences = protected_occurrences(text)
    require(bool(occurrences))
    for candidate in ranges:
        utf16_slice(text, candidate['start'], candidate['end'])
        require(all(candidate['end'] <= item['start'] or candidate['start'] >= item['end'] for item in occurrences))
    return occurrences


def expected_export(text, ghosts):
    """Independent exclusion and boundary-whitespace projection; no product calls."""
    ranges=[]
    for ghost in sorted(ghosts,key=lambda g:g['anchor']['start']):
        anchor=ghost['anchor']
        require(utf16_slice(text,anchor['start'],anchor['end'])==anchor['text'])
        start=0 if anchor['start']==0 else len(utf16_slice(text,0,anchor['start']))
        end=len(utf16_slice(text,0,anchor['end']))
        if ranges and start<=ranges[-1][1]:ranges[-1]=(ranges[-1][0],max(end,ranges[-1][1]))
        else:ranges.append((start,end))
    out='';cursor=0
    for start,end in ranges:
        if end<=cursor:continue
        out+=text[cursor:start] if cursor<start else ''
        cursor=end
        following=text[cursor:cursor+1]
        if out.endswith(' ') and (not following or following in ' \n.,;:!?)'):out=out[:-1]
        if not out or out.endswith('\n'):
            while text[cursor:cursor+1]==' ':cursor+=1
        if not out or out.endswith('\n\n'):
            while text[cursor:cursor+1]=='\n':cursor+=1
    return out+text[cursor:]


def promotion_ranges(active):
    """Project unique containing ranges only after refusing all crossing pairs."""
    ranges=sorted(set((c['start'],c['end']) for c in active),key=lambda r:(r[0],-r[1]))
    for i,(start,end) in enumerate(ranges):
        require(type(start) is int and type(end) is int and 0 <= start < end)
        for other_start,other_end in ranges[i+1:]:
            require(not (start < other_start < end < other_end))
    outer=[]
    for start,end in ranges:
        if outer and start < outer[-1][1]: continue
        outer.append((start,end))
    return outer


def verify_done(before, after, active, receipt):
    """Validate every independent native ghost receipt/range, not merely counts."""
    require(before['body'] == after['body'] == receipt['body'])
    require(before['counts'] == after['counts'])
    old, new = before['annotations'], after['annotations']
    require(receipt['before']['live'] == old['ghosts'])
    require(receipt['after']['live'] == new['ghosts'])
    require(receipt['before']['setAside'] == old['setAside']['ghosts'])
    require(receipt['after']['setAside'] == new['setAside']['ghosts'])
    for stage in ['before', 'after']:
        reserved = receipt[stage]['reservedIds']
        require(isinstance(reserved, list) and len(reserved) == len(set(reserved)))
        require(all(isinstance(item, str) and bool(item) for item in reserved))
        require(all(g['id'] in reserved for g in receipt[stage]['live']))
    require(all(g in new['ghosts'] for g in old['ghosts']))
    for key in ['spans', 'overflow', 'setAside', 'preservedBlock']:
        require(new[key] == old[key])
    added = [g for g in new['ghosts'] if g not in old['ghosts']]
    projected=promotion_ranges(active)
    require(len(added) == len(projected) and len(added) > 0)
    require(len({g['id'] for g in new['ghosts']}) == len(new['ghosts']))
    actual = sorted((g['anchor']['start'], g['anchor']['end']) for g in added)
    require(actual == projected)
    require(after['exported'] == expected_export(after['body'], new['ghosts']))
    assert_protected(after['body'], active)
    identity = lambda text: sorted((item['id'], item['text'].casefold()) for item in protected_occurrences(text))
    require(identity(after['exported']) == identity(before['body']))
    return added


def merged_ranges(ranges):
    merged = []
    for start, end in sorted(ranges):
        require(type(start) is int and type(end) is int and 0 <= start < end)
        if merged and start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(end, merged[-1][1]))
        else:
            merged.append((start, end))
    return merged


def verify_rendered_ghosts(segments, ghosts):
    expected = {'ghosts:' + ghost['id']: ghost['anchor'] for ghost in ghosts}
    actual = {}
    for segment in segments:
        tokens = [t for t in segment['tokens'] if t.startswith('ghosts:')]
        if segment['ghost'] or tokens:
            require(segment['ghost'] and bool(tokens))
            require(segment['opacity'] < 0.5 or segment['colorAlpha'] < 0.5)
        for token in tokens:
            actual.setdefault(token, []).append((segment['start'], segment['end']))
    require(set(actual) == set(expected))
    for token, anchor in expected.items():
        require(merged_ranges(actual[token]) == [(anchor['start'], anchor['end'])])


def verify_rendered_preview(text, segments, active):
    rendered = []
    for segment in segments:
        tokens = [t for t in segment['tokens'] if t.startswith('lab-trim:')]
        if segment['trim'] or tokens:
            require(segment['trim'] and bool(tokens))
            require(segment['opacity'] < 0.5 or segment['colorAlpha'] < 0.5)
            rendered.append(segment)
    assert_protected(text, rendered)
    require(merged_ranges((s['start'], s['end']) for s in rendered) ==
            merged_ranges((s['start'], s['end']) for s in active))


def overflow_insertion(body, at, text):
    """Independent UTF-16 insertion oracle from the paragraph-spacing contract."""
    raw=body.encode('utf-16-le')
    before=raw[:at*2].decode('utf-16-le')
    after=raw[at*2:].decode('utf-16-le')
    if before.endswith('\r') and after.startswith('\n'):
        before+='\n';after=after[1:]
    if before and not before.endswith('\n') and after and not after.startswith(('\r','\n')):
        return before+text+after
    eol='\r\n' if '\r\n' in body else '\n'
    lead=0 if not before else max(0,2-before[len(before.rstrip('\r\n')):].count('\n')) if before.endswith('\n') else 2
    trail=0 if not after else max(0,2-after[:len(after)-len(after.lstrip('\r\n'))].count('\n')) if after.startswith(('\r','\n')) else 2
    return before+eol*lead+text+eol*trail+after


def verify_drop(receipts, expected, source, target):
    starts = [x for x in receipts if x['type'] == 'dragstart']
    drops = [x for x in receipts if x['type'] == 'drop']
    require(len(starts) == len(drops) == 1)
    for event, location in [(starts[0], source), (drops[0], target)]:
        require(event['target'] == location and event['trusted'] and event['transfer'] and 'text/plain' in event['types'])
    require(drops[0]['payloadMatches'] is True and drops[0]['payload'] == expected)


def open_fixture(page, text):
    page.evaluate('(text)=>{window.__publicFixtureIndex=(window.__publicFixtureIndex||0)+1;window.terraphimEditor.openDocument(text,"public-acceptance-"+window.__publicFixtureIndex+".md");}', text)
    require(page.evaluate(READ_STATE)['body'] == text)


def surface(page):
    # Actual editor root, not an invented CSS selector or alternate test controller.
    page.evaluate('()=>window.terraphimEditor.surface.root.setAttribute("data-public-probe-surface","")')
    return page.locator('[data-public-probe-surface]')


def snapshot(page, output, label):
    page.evaluate('()=>document.fonts.ready')
    page.add_style_tag(content='*{animation:none!important;transition:none!important;caret-color:transparent!important}')
    page.screenshot(path=str(output / (label + '.png')))


def native_drag(page, start, end, expected, source='overflow', target='surface'):
    page.evaluate('()=>window.__publicDragReceipts.length=0')
    page.evaluate('(text)=>window.__publicDragExpected=text', expected)
    page.mouse.move(*start)
    page.mouse.down()
    page.wait_for_timeout(500)  # Native selected-text drag hold before movement.
    # Deliberate bounded native pointer transition; no synthetic DragEvent/transfer.
    page.mouse.move(start[0] + 12, start[1], steps=6)
    page.mouse.move(*end, steps=18)
    page.wait_for_timeout(250)
    page.mouse.move(end[0]+1,end[1],steps=2)
    page.wait_for_timeout(100)
    page.mouse.up()
    page.wait_for_timeout(100)
    receipts = page.evaluate('()=>window.__publicDragReceipts')
    if not any(r['type']=='drop' for r in receipts):
        page.keyboard.press('Escape')  # Cancel failed native drag before subsequent scenes.
    verify_drop(receipts, expected, source, target)
    return receipts


def run_embed(page, url, output):
    """New real navigation through the production public embed loader."""
    page.goto(url)
    page.wait_for_function('()=>window.__publicEmbedded?.editor?.surface')
    editor_surface = surface(page)
    def observe():
        return page.evaluate('''()=>{const b=window.__publicHostBefore;return {
lang:document.documentElement.lang,mainCount:document.querySelectorAll('main,[role=main]').length,
mainIdentity:document.getElementById('public-host-main')===b.main,
navIdentity:document.getElementById('public-host-nav')===b.nav,
bodyUnchanged:document.body.getAttribute('data-mode')===b.bodyMode &&
document.body.getAttribute('data-te-overflow')===b.bodyOverflow,
body:window.terraphimEditor.surface.getText()};}''')
    original = observe()
    require(original == {'lang': 'fr', 'mainCount': 1, 'mainIdentity': True,
                         'navIdentity': True, 'bodyUnchanged': True, 'body': HEADING})
    editor_surface.click()
    page.keyboard.press('Meta+ArrowDown' if page.evaluate('()=>navigator.platform.includes("Mac")') else 'Control+End')
    snapshot(page, output, 'embedded-plain-heading')
    page.locator('.te-chrome-counter').click()
    require(observe() == original)
    page.locator('[data-control="overflow"]').click()
    page.locator('.te-overflow-text').wait_for(state='visible')
    require(observe() == original)
    page.locator('.te-overflow-text').press('Escape')
    require(observe() == original)
    page.locator('.te-chrome-counter').click()
    editor_surface.click()
    page.keyboard.press('Control+Shift+L')
    require(page.evaluate('()=>window.terraphimEditor.chrome.toolsVisible()'))
    require(observe() == original)
    snapshot(page, output, 'embedded-tools-heading')
    return {'case': 'embed-host-preservation', 'status': 'PASS', 'host': original,
            'setup': 'generated public host lang=fr, existing main/nav identities, real TeraphimEditor.create and real delivery glue/WASM; no stub bindings',
            'mechanism': 'trusted count/Overflow controls, Escape and tools shortcut; host language/landmarks/body flags unchanged'}


def run_scenes(page, output, results=None):
    results = [] if results is None else results
    def case(name, fn):
        try:
            detail = fn() or {}
            results.append({'case': name, 'status': 'PASS', **detail})
        except Exception as error:
            if type(error) is TimeoutError:
                results.append({'case': name, 'status': 'FAIL', 'errorClass': 'TimeoutError',
                                'reason': 'Total browser-phase deadline reached'})
                raise  # Global deadline must not be swallowed by case reporting.
            frames=[{'line':f.lineno,'function':f.name} for f in traceback.extract_tb(error.__traceback__) if f.filename==__file__]
            try:
                diagnostic=page.evaluate('''()=>({state:window.terraphimEditor?{body:window.terraphimEditor.surface.getText().slice(0,12000),trimAnnouncement:window.terraphimEditor.lab.trim.live.textContent.slice(0,2000),mode:window.terraphimEditor.chrome.mode,labOpen:window.terraphimEditor.lab.isOpen}:null,drag:(window.__publicDragReceipts||[]).slice(-20),overflowSelection:(()=>{const a=document.querySelector('.te-overflow-text');return a?{start:a.selectionStart,end:a.selectionEnd,length:a.value.length}:null})(),notice:document.querySelector('.te-files-notice')?.getAttribute('data-kind')||null})''')
            except Exception as diagnostic_error:
                diagnostic={"status":"unavailable","errorClass":type(diagnostic_error).__name__}
                if type(diagnostic_error) is TimeoutError:
                    results.append({"case":name,"status":"FAIL","errorClass":type(error).__name__,"publicCheckpoints":frames,"publicDiagnostic":diagnostic})
                    raise

            results.append({'case': name, 'status': 'FAIL', 'errorClass': type(error).__name__,'publicCheckpoints':frames,'publicDiagnostic':diagnostic})
    editor_surface = surface(page)
    def mode(write_on):
        counter = page.locator('.te-chrome-counter')
        if (counter.get_attribute('aria-pressed') == 'true') != write_on:
            counter.click()
        require((counter.get_attribute('aria-pressed') == 'true') == write_on)

    def overflow():
        mode(True)
        if not page.locator('.te-overflow').is_visible():
            editor_surface.click()
            page.keyboard.press('Control+Shift+D')
        page.locator('.te-overflow-text').wait_for(state='visible')
        return page.locator('.te-overflow-text')

    def lab():
        mode(True)
        if not page.locator('.te-lab').is_visible():
            page.locator('[data-control="lab"]').click()
        page.locator('.te-lab').wait_for(state='visible')

    def headings():
        open_fixture(page, HEADING)
        mode(False)
        editor_surface.click()
        page.keyboard.press('Meta+ArrowDown' if page.evaluate('()=>navigator.platform.includes("Mac")') else 'Control+End')
        require(page.evaluate(READ_STATE)['body'] == HEADING)
        page.wait_for_function('()=>!window.terraphimEditor.surface.root.querySelector(".te-heading-marker--shown")')
        require(page.evaluate('()=>Array.from(window.terraphimEditor.surface.root.querySelectorAll(".te-heading-marker")).every(e=>getComputedStyle(e).fontSize==="0px" && e.getBoundingClientRect().width===0)'))
        require(page.locator('.te-heading-marker').count() > 0)
        snapshot(page, output, 'plain-heading')
        mode(True)
        require(page.evaluate(READ_STATE)['body'] == HEADING)
        snapshot(page, output, 'write-on-heading')
        return {'canonicalSha256': digest(HEADING.encode())}
    case('headings-and-mode', headings)

    def keyboard():
        open_fixture(page, BODY)
        area = overflow()
        area.fill('Public overflow note.')
        area.press('Control+A')
        area.press('Control+Enter')
        state = page.evaluate(READ_STATE)
        require(state['body'].count('Public overflow note.') == 1)
        require(state['annotations']['overflow'] == 'Public overflow note.')
        require(state['exported'].count('Public overflow note.') == 1)
        editor_surface.click()
        page.keyboard.press('Control+z')
        require(page.evaluate(READ_STATE)['body'] == BODY)
        page.keyboard.press('Control+Shift+z')
        require(page.evaluate(READ_STATE)['body'] == state['body'])
        area.click()
        area.press('Escape')
        require(not page.locator('.te-overflow').is_visible())
        require(page.evaluate(FOCUS)['visible'])
        return {'mechanism': 'trusted Control keyboard; fill is disclosed synthetic fixture input'}
    case('overflow-keyboard-history', keyboard)

    def drag():
        open_fixture(page, BODY)
        area = overflow()
        area.fill('Public note.')
        area.press('Meta+A' if page.evaluate('()=>navigator.platform.includes("Mac")') else 'Control+A')
        selection=area.evaluate('(a)=>({start:a.selectionStart,end:a.selectionEnd,text:a.value.slice(a.selectionStart,a.selectionEnd)})')
        require(selection == {'start':0,'end':12,'text':'Public note.'})
        before = page.evaluate(READ_STATE)
        index = page.evaluate('()=>window.terraphimEditor.surface.historyIndex')
        page.evaluate(DRAG_OBSERVER)
        a = area.bounding_box()
        glyph=editor_surface.evaluate('root=>{const n=document.createTreeWalker(root,NodeFilter.SHOW_TEXT).nextNode(),r=document.createRange();r.setStart(n,0);r.setEnd(n,1);const b=r.getBoundingClientRect();return {x:b.x,y:b.y,width:b.width,height:b.height};}')
        require(glyph['width']>0 and glyph['height']>0)
        receipts = native_drag(page, (a['x'] + 12, a['y'] + 12), (glyph['x']+glyph['width']/2,glyph['y']+glyph['height']/2), expected='Public note.')
        state = page.evaluate(READ_STATE)
        require(state['body'].count('Public note.') == 1)
        require(state['body'] == state['nativeBody'])
        anchors = [item for item in receipts if item['type'] == 'observed-pull-anchor']
        require(len(anchors) == 1)
        expected = overflow_insertion(BODY,anchors[0]['dropOffset'],'Public note.')
        require(state['body'] == expected)
        require(state['annotations']['overflow'] == 'Public note.')
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == index + 1)
        editor_surface.click()
        page.keyboard.press('Control+z')
        require(page.evaluate(READ_STATE) == before)
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == index)
        page.keyboard.press('Control+Shift+z')
        require(page.evaluate(READ_STATE) == state)
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == index + 1)
        snapshot(page, output, 'overflow')
        return {'receipts': receipts, 'mechanism': 'native mouse down/move/up; no handler substitution'}
    case('overflow-native-selected-text-drag-out', drag)

    def drag_in():
        open_fixture(page, BODY)
        area = overflow()
        area.fill('Existing public note.')
        editor_surface.click()  # blur flushes synthetic setup into actual model
        page.keyboard.press('Meta+ArrowDown' if page.evaluate('()=>navigator.platform.includes("Mac")') else 'Control+End')
        for _ in 'Third paragraph.':
            page.keyboard.press('Shift+ArrowLeft')
        selected = page.evaluate('()=>{const e=window.terraphimEditor,s=e.surface.getSelectionOffsets();return {...s,text:e.surface.getText().slice(s.start,s.end)};}')
        require(selected['text'] == 'Third paragraph.')
        before = page.evaluate(READ_STATE)
        index = page.evaluate('()=>window.terraphimEditor.surface.historyIndex')
        geometry = page.evaluate('()=>{const r=document.getSelection().getRangeAt(0).getClientRects()[0];return {x:r.x,y:r.y,width:r.width,height:r.height};}')
        require(geometry['width'] > 0 and geometry['height'] > 0)
        target = area.bounding_box()
        page.evaluate(DRAG_OBSERVER)
        receipts = native_drag(page, (geometry['x'] + geometry['width'] / 2, geometry['y'] + geometry['height'] / 2),
                               (target['x'] + 20, target['y'] + 20), expected=selected['text'], source='surface', target='overflow')
        after = page.evaluate(READ_STATE)
        expected = BODY[:selected['start']] + BODY[selected['end']:]
        require(after['body'] == after['nativeBody'] == expected)
        require(after['annotations']['overflow'] == 'Existing public note.\n\nThird paragraph.')
        require(after['annotations']['spans'] == before['annotations']['spans'])
        require(after['annotations']['ghosts'] == before['annotations']['ghosts'])
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == index + 1)
        editor_surface.click()
        page.keyboard.press('Control+z')
        require(page.evaluate(READ_STATE) == before)
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == index)
        page.keyboard.press('Control+Shift+z')
        require(page.evaluate(READ_STATE) == after)
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == index + 1)
        saved = page.evaluate('()=>window.terraphimEditor.saveDocument()')
        page.evaluate('(text)=>window.terraphimEditor.openDocument(text,"public-drag-reopen.md")', saved)
        require(page.evaluate(READ_STATE) == after)
        snapshot(page, output, 'overflow-reverse-drag')
        return {'receipts': receipts, 'selection': selected,
                'mechanism': 'trusted keyboard selected last paragraph, native selected-glyph drag to Overflow; exact native body/metadata undo/redo/reopen'}
    case('overflow-native-drag-in', drag_in)

    def no_role():
        open_fixture(page, TRIM_BODY)
        page.evaluate('()=>window.terraphimEditor.clearThesaurus()')
        lab()
        levels = page.locator('.te-trim-level')
        require(levels.count() == 5)
        require(page.locator('.te-trim-level:not([data-level="original"]):disabled').count() == 4)
        require(not page.locator('.te-trim-level[data-level="original"]').is_disabled())
        before = page.evaluate(READ_STATE)
        require(page.evaluate('()=>typeof window.terraphimEditor.requireDocumentApi().trim_plan_json==="function"'))
        refused = page.evaluate('()=>{try{window.terraphimEditor.requireDocumentApi().trim_plan_json();return false;}catch(_error){return true;}}')
        require(refused and page.evaluate(READ_STATE) == before)
        snapshot(page, output, 'lab-no-role')
        return {'setup': 'explicit clearThesaurus API; actual Lab button and DOM disabled state'}
    case('no-role-disabled-trim', no_role)

    def protected_done(text, crossing):
        open_fixture(page, text)
        summary = page.evaluate('(role)=>window.terraphimEditor.loadThesaurus(role)', ROLE)
        require(summary['name'] == 'writing-fixture' and summary['concepts'] == 7)
        mode(True)
        # A real manual ghost over the Markdown heading, disjoint from body cuts.
        # No arbitrary-range ghost model setup and no revision of native APIs.
        editor_surface.click()
        page.keyboard.press('Meta+ArrowUp' if page.evaluate('()=>navigator.platform.includes("Mac")') else 'Control+Home')
        page.keyboard.press('ArrowRight')
        page.keyboard.press('ArrowRight')
        for _ in 'Public fixture':
            page.keyboard.press('Shift+ArrowRight')
        selection = page.evaluate('()=>window.terraphimEditor.surface.getSelectionOffsets()')
        require(utf16_slice(text, selection['start'], selection['end']) == 'Public fixture')
        page.keyboard.press('Control+/')
        manual = page.evaluate(READ_STATE)
        require(len(manual['annotations']['ghosts']) == 1)
        require(manual['annotations']['ghosts'][0]['anchor']['text'] == 'Public fixture')
        verify_rendered_ghosts(page.evaluate(RENDERED), manual['annotations']['ghosts'])
        area = overflow()
        area.fill('Public saved overflow sentinel.')
        editor_surface.click()
        area.press('Escape')
        before = page.evaluate(READ_STATE)
        require(before['annotations']['overflow'] == 'Public saved overflow sentinel.')
        levels = []
        for level in ['slight', 'tighten', 'sharper', 'half']:
            lab()
            page.locator('.te-trim-level[data-level="' + level + '"]').click()
            page.locator('.te-trim-card').wait_for(state='visible')
            status = page.evaluate('()=>window.terraphimEditor.lab.trim.status')
            require(status['kgProtected'] is True)
            matches = assert_protected(text, status['active'])
            verify_rendered_preview(text, page.evaluate(RENDERED), status['active'])
            verify_rendered_ghosts(page.evaluate(RENDERED), before['annotations']['ghosts'])
            require(status['protectionCount'] == len(matches))
            require(page.evaluate(READ_STATE) == before)
            levels.append({'level': level, 'activeRanges': [{'start': c['start'], 'end': c['end']} for c in status['active']],
                           'protectionCount': len(matches), 'cardText': status['cardText']})
        active = status['active']
        require(len(active) > 0)
        if crossing:
            refused=False
            try: promotion_ranges(active)
            except AssertionError: refused=True
            require(refused)
            refusal_before=page.evaluate(READ_STATE)
            page.locator('.te-trim-done').click()
            announcement=page.evaluate('()=>window.terraphimEditor.lab.trim.live.textContent')
            require('unsafe_promotion: crossing active fragments' in announcement)
            require(page.locator('.te-trim-card').is_visible())
            require(page.evaluate(READ_STATE)==refusal_before)
            verify_rendered_preview(text,page.evaluate(RENDERED),active)
            snapshot(page,output,'crossing-Done-refusal')
            return {'expectedRefusal':announcement,'literalBodyPreserved':True,'levels':levels,'fixtureSha256':digest(text.encode())}
        lab()
        page.locator('.te-trim-level[data-level="slight"]').click()
        status=page.evaluate('()=>window.terraphimEditor.lab.trim.status')
        active=status['active']
        require(len(active)>0)
        promotion_ranges(active)
        history_before = page.evaluate('()=>window.terraphimEditor.surface.historyIndex')
        require(page.evaluate('()=>{const a=window.terraphimEditor.requireDocumentApi();return ["begin_trim_ghost_transaction","begin_document_history_replay","finish_document_history_transaction","abort_document_history_transaction"].every(n=>typeof a[n]==="function");}'))
        page.evaluate('()=>{window.__publicDoneTrusted=[];document.addEventListener("click",e=>{if(e.target.closest?.(".te-trim-done"))window.__publicDoneTrusted.push(e.isTrusted);},true);}')
        snapshot(page, output, 'protected-trim')
        page.locator('.te-trim-done').click()
        page.locator('.te-trim-card').wait_for(state='hidden')
        require(page.locator('.te-trim-faded').count() == 0)
        require(page.evaluate('()=>window.__publicDoneTrusted') == [True])
        native = page.evaluate('()=>{const e=window.terraphimEditor,s=e.surface,top=s.history[s.historyIndex];return {index:s.historyIndex,edits:top.edits,source:top.source,active:e.ghostHistoryActive,conflict:!!e.ghostHistoryConflict};}')
        require(native['index'] == history_before + 1 and len(native['edits']) == 1)
        require(native['active'] is False and native['conflict'] is False)
        require(native['edits'][0]['ghost']['undo'] is False)
        receipt = native['edits'][0]['ghost']['change']
        after = page.evaluate(READ_STATE)
        require(after['nativeBody'] == text)
        added = verify_done(before, after, active, receipt)
        verify_rendered_ghosts(page.evaluate(RENDERED), after['annotations']['ghosts'])
        verify_rendered_preview(text, page.evaluate(RENDERED), [])
        snapshot(page, output, 'done-persistent-ghosts')
        editor_surface.click()
        page.keyboard.press('Control+z')
        require(page.evaluate(READ_STATE) == before)
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == history_before)
        verify_rendered_ghosts(page.evaluate(RENDERED), before['annotations']['ghosts'])
        page.keyboard.press('Control+Shift+z')
        require(page.evaluate(READ_STATE) == after)
        require(page.evaluate('()=>window.terraphimEditor.surface.historyIndex') == history_before + 1)
        verify_rendered_ghosts(page.evaluate(RENDERED), after['annotations']['ghosts'])
        saved = page.evaluate('()=>window.terraphimEditor.saveDocument()')
        require(saved != text and 'Public saved overflow sentinel.' in saved)
        page.evaluate('(text)=>window.terraphimEditor.openDocument(text,"public-done-reopen.md")', saved)
        require(page.evaluate(READ_STATE) == after)
        verify_rendered_ghosts(page.evaluate(RENDERED), after['annotations']['ghosts'])
        require('Public saved overflow sentinel.' not in after['exported'])
        snapshot(page, output, 'done-reopened')
        return {'roleSummary': {'name': summary['name'], 'concepts': summary['concepts']},
                'manualGhostPreserved': True, 'nativeReceipt': receipt, 'addedGhosts': added,
                'levels': levels, 'protectedOccurrences': protected_occurrences(text),
                'savedSha256': digest(saved.encode()), 'cleanExportSha256': digest(after['exported'].encode()),
                'mechanism': 'actual manual ghost shortcut, four Lab level buttons, trusted Done click, one native-receipt undo/redo; disclosed API save/reopen/read'}
    case('protected-crossing-Done-refusal', lambda:protected_done(TRIM_BODY,True))
    case('protected-trim-Done-reopen', lambda:protected_done(TRIM_SAFE_BODY,False))

    def tools():
        open_fixture(page, HEADING)
        mode(False)
        editor_surface.click()
        page.keyboard.press('Meta+ArrowDown' if page.evaluate('()=>navigator.platform.includes("Mac")') else 'Control+End')
        if not page.evaluate('()=>window.terraphimEditor.chrome.toolsVisible()'):
            page.keyboard.press('Control+Shift+L')
        require(page.evaluate('()=>window.terraphimEditor.chrome.toolsVisible()'))
        page.wait_for_function('()=>!window.terraphimEditor.surface.root.querySelector(".te-heading-marker")')
        raw = page.evaluate('()=>window.terraphimEditor.surface.root.innerText')
        require(raw.startswith('# The Obvious, the Easy, and the Possible'))
        require(page.evaluate(READ_STATE)['body'] == HEADING)
        snapshot(page, output, 'tools-raw-heading')
        page.keyboard.press('Control+Shift+L')
        require(not page.evaluate('()=>window.terraphimEditor.chrome.toolsVisible()'))
        require(page.evaluate(READ_STATE)['body'] == HEADING)
        return {'mechanism': 'trusted Control+Shift+L in plain mode; literal raw # rendered; exact canonical body retained'}
    case('tools-raw-heading', tools)

    def focus():
        trace = []
        for _ in range(20):
            page.keyboard.press('Tab')
            point = page.evaluate(FOCUS)
            require(not point['hidden'])
            trace.append(point)
        structure = page.evaluate('()=>({lang:document.documentElement.lang,main:document.querySelectorAll("main,[role=main]").length})')
        require(bool(structure['lang']) and structure['main'] == 1)
        return {'scope': 'LIMITED DOM structure and 20 real Tab focus positions; not WCAG/screen-reader certification',
                'structure': structure, 'focus': trace}
    case('limited-accessibility', focus)
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', choices=['chrome', 'firefox', 'safari'], required=True)
    parser.add_argument('--delivery', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--admission-json', required=True,
                        help='inline independently reviewed public registration JSON, never a file to read')
    parser.add_argument('--source-head', required=True)
    parser.add_argument('--outputDir', type=Path, required=True)
    parser.add_argument('--entry', default='index.html')
    parser.add_argument('--editor-source-sha256', default=CANDIDATE_EDITOR_SHA256)
    parser.add_argument('--serial-freeze-approved', action='store_true')
    args = parser.parse_args()
    if not args.serial_freeze_approved:
        parser.error('browser launch forbidden until parent serial freeze approval')
    # Admission declarations are checked BEFORE git, stat, ancestry or content reads.
    try:
        if len(args.admission_json) > 1024 * 1024:
            raise ValueError('public registration too large')
        admission = admit(json.loads(args.admission_json), args.delivery, args.manifest, args.source_head)
        source_paths, _members, manifest_path = admission_paths(admission)
    except (ValueError, TypeError, KeyError):
        parser.error('public admission refused; no content read or browser launch')
    actual_head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
    if actual_head != args.source_head:
        parser.error('source HEAD mismatch')
    source_hash = admission.sources['public/js/editor.js']['sha256']
    if source_hash != args.editor_source_sha256:
        parser.error('reviewed editor candidate source hash mismatch')
    manifest_size = manifest_path.stat().st_size
    if manifest_size > 1024 * 1024:
        parser.error('registered manifest exceeds finite metadata bound')
    manifest_bytes = read_verified(manifest_path, {'bytes': manifest_size, 'sha256': admission.manifest_sha256})
    manifest = json.loads(manifest_bytes)
    validate_manifest(args.delivery, manifest, args.source_head, admission)
    source_fingerprint = verify_sources(admission, source_paths)
    payload = immutable_payload(args.delivery, manifest, args.source_head, admission)
    if relative(args.entry) not in payload:
        parser.error('entry must be a manifest member')
    output = args.outputDir.absolute()
    if any(p.is_symlink() for p in (output, *output.parents)):
        parser.error('output symlink ancestry refused')
    output.mkdir(parents=False, exist_ok=False)
    report = {'schema': 1, 'sourceHead': args.source_head, 'engine': args.engine,
              'manifestSha256': admission.manifest_sha256, 'files': list(admission.files.values()),
              'sourceInputs': list(admission.sources.values()), 'sourceFingerprint': source_fingerprint,
              'provenance': {'sourceFingerprint': source_fingerprint, 'files': list(admission.files.values()),
                             'scope': 'independently admitted source/output association; not a reproducible-build proof'},
              'editorSourceSha256': source_hash,
              'startedUtc': datetime.now(timezone.utc).isoformat(),
              'fixtureSha256': digest((BODY + HEADING + TRIM_BODY).encode()),
              'setup': 'openDocument public synthetic fixture API, clearThesaurus; not OS picker proof',
              'status': 'NOT RUN', 'cases': []}
    if args.engine == 'safari':
        report['reason'] = 'Actual native Safari requires separate protocol runner; WebKit substitution forbidden'
    else:
        try:
            payload = embed_payload(payload)
        except ValueError:
            payload = dict(payload)  # Full-page checks still run; embed MUST fail, never skip.
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler_for(payload))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        origin = 'http://127.0.0.1:' + str(server.server_port)
        categories = []
        try:
            def deadline(_signum, _frame):
                raise TimeoutError('bounded acceptance deadline')
            previous_handler = signal.signal(signal.SIGALRM, deadline)
            signal.alarm(60)
            from playwright.sync_api import sync_playwright
            version = importlib.metadata.version('playwright')
            require(tuple(int(x) for x in version.split('.')[:2]) >= (1, 57))
            with sync_playwright() as p:
                verify_sources(admission, source_paths)  # Recheck the entire admitted cohort immediately before launch.
                browser = (p.chromium.launch(channel='chrome') if args.engine == 'chrome' else p.firefox.launch())
                report.update({'playwrightVersion': version, 'browserVersion': browser.version})
                context = browser.new_context(viewport={'width': 1140, 'height': 720}, device_scale_factor=1, service_workers='block')
                def route(request_route):
                    url = urlsplit(request_route.request.url)
                    if url.scheme in ('http', 'https') and url.scheme + '://' + url.netloc == origin:
                        request_route.continue_()
                    else:
                        categories.append('external-request-blocked')
                        request_route.abort()
                context.route('**/*', route)
                page = context.new_page()
                page.set_default_timeout(10000)
                page.on('pageerror', lambda _error: categories.append('page-runtime-error'))
                page.on('console', lambda message: categories.append('console-error') if message.type == 'error' else None)
                page.goto(origin + '/acceptance/nested/' + args.entry)
                page.wait_for_function('()=>window.terraphimEditor?.surface && typeof window.terraphimEditor.openDocument === "function"')
                page.evaluate('()=>document.fonts.ready')
                report['fonts'] = page.evaluate('()=>({ready:document.fonts.status,body:getComputedStyle(document.body).fontFamily,surface:getComputedStyle(window.terraphimEditor.surface.root).fontFamily})')
                run_scenes(page, output, report['cases'])
                page.set_viewport_size({'width': 900, 'height': 1000})
                open_fixture(page, HEADING)
                surface(page).click()
                page.keyboard.press('Meta+ArrowDown' if page.evaluate('()=>navigator.platform.includes("Mac")') else 'Control+End')
                page.wait_for_function('()=>!window.terraphimEditor.surface.root.querySelector(".te-heading-marker--shown")')
                snapshot(page, output, 'narrow-heading')
                page.set_viewport_size({'width': 1140, 'height': 720})
                try:
                    require(EMBED_PATH in payload)
                    report['generatedFixture'] = {'path': EMBED_PATH, 'sha256': digest(payload[EMBED_PATH])}
                    report['cases'].append(run_embed(page, origin + '/acceptance/nested/' + EMBED_PATH, output))
                except Exception as error:
                    report['cases'].append({'case': 'embed-host-preservation', 'status': 'FAIL', 'errorClass': type(error).__name__})
                    if type(error) is TimeoutError:
                        raise
                report['status'] = 'FAIL' if categories or any(x['status'] == 'FAIL' for x in report['cases']) else 'INCOMPLETE'
                browser.close()
        except Exception as error:
            report.update({'status': 'FAIL', 'errorClass': type(error).__name__})
        finally:
            signal.alarm(0)
            signal.signal(signal.SIGALRM, previous_handler)
            server.shutdown()
            server.server_close()
        report['errorCategories'] = categories
    report['finishedUtc'] = datetime.now(timezone.utc).isoformat()
    report['screenshots'] = [{'path': p.name, 'sha256': digest(p.read_bytes()),
                              'viewport': {'width': 900, 'height': 1000} if p.stem == 'narrow-heading' else {'width': 1140, 'height': 720},
                              'deviceScaleFactor': 1, 'caret': 'transparent', 'animations': 'disabled'}
                             for p in sorted(output.glob('*.png'))]
    with (output / 'report.json').open('x') as stream:
        json.dump(report, stream, indent=2)
        stream.write('\n')
    return 1  # Prepared coverage is deliberately incomplete, never release PASS.


if __name__ == '__main__':
    raise SystemExit(main())
