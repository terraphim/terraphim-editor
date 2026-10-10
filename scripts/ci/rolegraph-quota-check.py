#!/usr/bin/env python3
"""Exercise actual Chromium quota failure without filling disk/user storage.

Uses a fresh browser context and unique loopback origin/namespace. DevTools
overrides only that test origin's quota; actual IndexedDB emits the error.
"""
import argparse
import functools
import hashlib
import http.server
import json
import pathlib
import platform
import tempfile
import threading
import time
import uuid
from datetime import datetime, timezone
from urllib.parse import unquote, urlparse

from playwright.sync_api import sync_playwright


def cleanup_step(result, label, action):
    """Every external cleanup gets its own attempt; none may preserve a green run."""
    try:
        value = action()
        result.setdefault('cleanup', []).append({'step': label, 'ok': True, 'detail': value})
    except Exception as error:
        result['ok'] = False
        result.setdefault('runnerErrors', []).append({'where': label, 'error': str(error)})
        result.setdefault('cleanup', []).append({'step': label, 'ok': False})


def qualify(result):
    events = result.get('transactionEvents', [])
    quota_error = any(
        event.get('phase') == 'measured-save' and event.get('database') == result.get('database')
        and event.get('transactionMode') == 'readwrite' and event.get('transactionId') is not None
        and event.get('bubbles') is True and event.get('errorName') == 'QuotaExceededError'
        and (event.get('eventType'), event.get('targetKind')) in [('error', 'request'), ('abort', 'transaction')]
        for event in events)
    diagnostics = result.get('quotaDiagnostics', {})
    override_verified = all(diagnostics.get(label, {}).get('overrideActive') is True
                            and diagnostics[label].get('quota') == 1 for label in ['afterOverride', 'afterWait'])
    baseline_verified = (result.get('baselineSaved') is True and result.get('previousBytes', 0) > 0
                         and result.get('previousBytes') == result.get('retainedBytes')
                         and isinstance(result.get('previousSha256'), str)
                         and len(result['previousSha256']) == 64
                         and result.get('previousSha256') == result.get('retainedSha256'))
    return (not result.get('runnerErrors') and not result.get('pageErrors') and baseline_verified
            and result.get('refusal') == 'storage_failed' and quota_error and override_verified
            and result.get('cacheWaitMs', 0) >= 33000
            and all(result.get(key) is True for key in
                    ['previousCheckpointRetained', 'unsavedGraphAvailable', 'mainRoleAssociated']))


def stage_delivery(source, stage, metadata, stage_hook=None):
    def files():
        return sorted(p for p in source.rglob('*') if p.is_file())
    def identity(path):
        stat = path.stat()
        return (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    paths = files()
    before = {p: identity(p) for p in paths}
    captured = {str(p.relative_to(source)): p.read_bytes() for p in paths}
    required = {'index.html', 'js/rolegraph-worker.js', 'rolegraph/terraphim_rolegraph_browser.js',
                'rolegraph/terraphim_rolegraph_browser_bg.wasm'}
    if not required.issubset(captured):
        raise RuntimeError(f'Missing delivery inputs: {sorted(required - captured.keys())}')
    if files() != paths or any(identity(p) != before[p] for p in paths):
        raise RuntimeError('Delivery drift during capture; use a stable completed build')
    metadata['assets'] = {}
    for name, data in captured.items():
        metadata['assets'][name] = {'rawBytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        target = stage / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    manifest = json.dumps(metadata['assets'], sort_keys=True, separators=(',', ':')).encode()
    metadata['inputManifestSha256'] = hashlib.sha256(manifest).hexdigest()
    metadata['captureMethod'] = 'read-once complete delivery; stat drift rejection; stage hash verification; no release build manifest'
    if stage_hook:
        stage_hook(stage)
    for name, asset in metadata['assets'].items():
        if hashlib.sha256((stage / name).read_bytes()).hexdigest() != asset['sha256']:
            raise RuntimeError(f'Staged input hash drift: {name}')


def write_result(result, output):
    if result.get('runnerErrors') or result.get('pageErrors'):
        result['ok'] = False
    result['exitCode'] = 0 if result.get('ok') is True else 1
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + '\n')
    return result['exitCode']


SETUP = """async namespace => {
  const graph = window.terraphimEditor.rolegraph;
  await graph.ready; await graph.lifecycle;
  await graph.loadRole(JSON.stringify({name:'quota-fixture',data:{alpha:{id:1,nterm:'alpha'},beta:{id:2,nterm:'beta'}}}));
  await graph.indexDocuments([{id:'saved',title:'saved',body:'alpha beta alpha'}]);
  graph.setPersistence({enabled:true,namespace});
  const baselineText=await graph.saveCheckpoint();
  window.quotaGraph=graph;
  window.quotaPrevious=await graph.storage.load();
  if(typeof window.quotaPrevious!=='string'||window.quotaPrevious!==baselineText) throw Error('Baseline checkpoint was not committed/read back exactly');
  const database=await graph.storage._open();
  window.quotaTransactionEvents=[];
  window.quotaPhase='setup';
  const ids=new WeakMap(); let nextId=0;
  const observe=event=>{
    if(window.quotaPhase!=='measured-save') return;
    const target=event.target;
    const kind=target instanceof IDBTransaction?'transaction':target instanceof IDBRequest?'request':'other';
    const tx=kind==='transaction'?target:kind==='request'?target.transaction:null;
    if(!tx || tx.db!==database || tx.mode!=='readwrite' || !Array.from(tx.objectStoreNames).includes('snapshots')) return;
    if(!ids.has(tx)) ids.set(tx,++nextId);
    const record={phase:window.quotaPhase,database:database.name,eventType:event.type,targetKind:kind,
      transactionId:ids.get(tx),transactionMode:tx.mode,bubbles:event.bubbles,
      errorName:target.error?.name??null,timeMs:performance.now()};
    record.failureStage=kind==='request'?'request':event.type==='abort'
      ?(window.quotaTransactionEvents.some(e=>e.transactionId===record.transactionId&&e.targetKind==='request')
        ?'transaction-abort-after-request-error':'transaction-abort-without-request-error'):'transaction-complete';
    window.quotaTransactionEvents.push(record);
    if(event.type==='abort'||event.type==='complete') window.quotaTerminalResolve?.(event.type);
  };
  // Capture the real bubbling events before production handlers settle promises.
  // No preventDefault, prototype overrides, or replacement of production handlers.
  for(const type of ['error','abort','complete']) database.addEventListener(type,observe,true);
  window.quotaRemoveObservers=()=>{for(const type of ['error','abort','complete']) database.removeEventListener(type,observe,true);};
  await graph.indexDocuments([{id:'unsaved',title:'unsaved',body:'alpha beta '.repeat(12000)}]);
  return {baselineSaved:true,previousBytes:new TextEncoder().encode(window.quotaPrevious).length,
    database:database.name,fixture:{documents:2,concepts:2,unsavedBodyBytes:132000},userAgent:navigator.userAgent};
}"""

MEASURE = """async () => {
  const graph=window.quotaGraph;
  window.quotaTransactionEvents=[]; window.quotaPhase='measured-save';
  const terminal=new Promise(resolve=>{window.quotaTerminalResolve=resolve;});
  let refusal=null;
  try {await graph.saveCheckpoint();} catch(error) {refusal=error.code??String(error);}
  let timer;
  const terminalEvent=await Promise.race([terminal,new Promise(resolve=>{timer=setTimeout(()=>resolve('not-observed'),2000);})]);
  clearTimeout(timer); window.quotaPhase='verification';
  const stored=await graph.storage.load();
  const retained=stored===window.quotaPrevious;
  const query=await graph.query('alpha',{limit:10});
  const available=query.some(document=>document.id==='unsaved');
  const main=JSON.parse(window.wasmBindings.kg_role_snapshot_json());
  const associated=main.generation===graph.generation;
  const digest=async text=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text))),
    b=>b.toString(16).padStart(2,'0')).join('');
  return {refusal,terminalEvent,transactionEvents:window.quotaTransactionEvents,
    previousCheckpointRetained:retained,previousSha256:await digest(window.quotaPrevious),
    retainedSha256:stored===null?null:await digest(stored),retainedBytes:stored===null?null:new TextEncoder().encode(stored).length,
    unsavedGraphAvailable:available,queryIdentities:query.map(d=>({id:d.id,rank:d.rank,nodes:d.nodes})),
    mainRoleAssociated:associated,mainGeneration:main.generation,graphGeneration:graph.generation};
}"""

DELETE_NAMESPACE = """async namespace => {
  const database='te-rolegraph:'+namespace;
  await new Promise((resolve,reject)=>{
    const request=indexedDB.deleteDatabase(database);
    request.onsuccess=resolve;
    request.onerror=()=>reject(Error(request.error?.name??'delete failed'));
    request.onblocked=()=>reject(Error('Test namespace deletion blocked'));
  });
  const remaining=await indexedDB.databases();
  if(remaining.some(db=>db.name===database)) throw Error('Test namespace still exists');
  return {database,deleted:true};
}"""


def main(argv=None, *, cleanup_fault=None, stage_hook=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--executable", required=True)
    parser.add_argument("--result", type=pathlib.Path, required=True)
    parser.add_argument('--timeout-seconds', type=int, default=120, help='Isolated run bound (45–120 seconds)')
    args = parser.parse_args(argv)
    if not 45 <= args.timeout_seconds <= 120:
        parser.error('--timeout-seconds must be between 45 and 120')
    output = args.output.resolve()
    namespace = "qualification-quota-" + uuid.uuid4().hex
    started = time.monotonic()
    deadline = started + args.timeout_seconds
    result = {'ok': False, 'utc': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
              'executable': args.executable, 'namespace': namespace, 'database': 'te-rolegraph:'+namespace,
              'timeoutSeconds': args.timeout_seconds, 'quotaDiagnostics': {}, 'pageErrors': []}
    metadata = {'delivery': str(output)}
    result['runner'] = metadata
    result['runnerSourceSha256'] = hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
    def remaining():
        ms = int((deadline-time.monotonic())*1000)
        if ms <= 0:
            raise TimeoutError('Isolated quota runner deadline exceeded')
        return ms
    def cleanup(label, action):
        def injected():
            value = action()
            if cleanup_fault == label:
                raise RuntimeError(f'injected after actual {label}')
            return value
        cleanup_step(result, label, injected)
    try:
        with tempfile.TemporaryDirectory(prefix='rolegraph-quota-') as temporary:
            stage = pathlib.Path(temporary)
            stage_delivery(output, stage, metadata, stage_hook)
            server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
                functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(stage)))
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            origin = f'http://127.0.0.1:{server.server_port}'
            result['origin'] = origin
            thread.start()
            try:
                with sync_playwright() as playwright:
                    browser = playwright.chromium.launch(executable_path=args.executable, headless=True, timeout=remaining())
                    context = page = cdp = None
                    try:
                        result['browserVersion'] = browser.version
                        context = browser.new_context()
                        context.route('https://**', lambda route: route.abort())
                        page = context.new_page()
                        def evaluate(script, arg=None, *, cleanup=False):
                            timeout = 3000 if cleanup else min(remaining(), 20000)
                            wrapper = """async ({arg,timeoutMs})=>{let timer;try{return await Promise.race([
                              (SCRIPT)(arg),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Bounded page operation timeout')),timeoutMs);})
                            ]);}finally{clearTimeout(timer);}}""".replace('SCRIPT', script)
                            return page.evaluate(wrapper, {'arg':arg, 'timeoutMs':timeout})
                        requested = []
                        def response_observer(response):
                            if response.url.startswith(origin+'/'):
                                name = unquote(urlparse(response.url).path).lstrip('/') or 'index.html'
                                requested.append({'path':name, 'status':response.status})
                        page.on('response', response_observer)
                        page.on('pageerror', lambda error: result['pageErrors'].append(str(error)))
                        page.goto(origin, timeout=remaining())
                        page.wait_for_function('Boolean(window.terraphimEditor && window.wasmBindings)', timeout=remaining())
                        result.update(evaluate(SETUP, namespace))
                        cdp = context.new_cdp_session(page)
                        result['quotaDiagnostics']['beforeOverride'] = cdp.send('Storage.getUsageAndQuota', {'origin':origin})
                        cdp.send('Storage.overrideQuotaForOrigin', {'origin':origin, 'quotaSize':1})
                        result['quotaDiagnostics']['afterOverride'] = cdp.send('Storage.getUsageAndQuota', {'origin':origin})
                        # Chrome 155 BucketContext allowance decays across 30 seconds.
                        # A warmed database may admit a write earlier, even with quota=1.
                        if remaining() < 36000:
                            raise TimeoutError('Insufficient isolated run time for 33-second cache expiry wait')
                        wait_started = time.monotonic()
                        time.sleep(33)
                        result['cacheWaitMs'] = (time.monotonic()-wait_started)*1000
                        result['quotaDiagnostics']['afterWait'] = cdp.send('Storage.getUsageAndQuota', {'origin':origin})
                        result.update(evaluate(MEASURE))
                        result['quotaDiagnostics']['afterWrite'] = cdp.send('Storage.getUsageAndQuota', {'origin':origin})
                        result['completed'] = True
                        result['ok'] = qualify(result)
                        result['qualificationStatus'] = 'observed actual quota refusal' if result['ok'] else 'incomplete: required actual quota/storage/retention evidence absent'
                    finally:
                        if cdp is not None:
                            def reset_quota():
                                cdp.send('Storage.overrideQuotaForOrigin', {'origin':origin})
                                diagnostics = cdp.send('Storage.getUsageAndQuota', {'origin':origin})
                                result['quotaDiagnostics']['afterReset'] = diagnostics
                                if diagnostics.get('overrideActive') is not False:
                                    raise RuntimeError('Quota override remains active after reset')
                                return {'overrideActive': False}
                            cleanup('quota.reset', reset_quota)
                        if page is not None:
                            cleanup('telemetry.remove', lambda: evaluate("async()=>{window.quotaPhase='cleanup';window.quotaRemoveObservers?.();}", cleanup=True))
                            cleanup('editor.destroy', lambda: evaluate("async()=>{window.terraphimEditor?.destroy();}", cleanup=True))
                            cleanup('namespace.delete', lambda: evaluate(DELETE_NAMESPACE, namespace, cleanup=True))
                        if context is not None:
                            cleanup('context.close', context.close)
                        cleanup('browser.close', browser.close)
                        if page is not None:
                            metadata['requestedAssets'] = requested
                            metadata['executedOrRequestedPaths'] = sorted(set(r['path'] for r in requested))
                            for request in requested:
                                name = request['path']
                                if name.endswith(('.js','.wasm','.css','.html')) and (name not in metadata['assets'] or request['status'] >= 400):
                                    result['ok'] = False
                                    result.setdefault('runnerErrors', []).append({'where':'asset requests', 'error':str(request)})
                            metadata['inventoryOnlyPaths'] = sorted(set(metadata['assets'])-set(metadata['executedOrRequestedPaths']))
            finally:
                cleanup('server.shutdown', server.shutdown)
                cleanup('server.close', server.server_close)
                cleanup('server.thread.join', lambda: thread.join(timeout=3))
                if thread.is_alive():
                    result['ok'] = False
                    result.setdefault('runnerErrors', []).append({'where':'server.thread.join', 'error':'Server thread still alive'})
    except Exception as error:
        result['ok'] = False
        result.setdefault('runnerErrors', []).append({'where':'execution/staging', 'error':str(error)})
    result['elapsedSeconds'] = time.monotonic()-started
    code = write_result(result, args.result)
    print(json.dumps({key:result.get(key) for key in ['ok','exitCode','browserVersion','origin','namespace','cacheWaitMs',
        'refusal','terminalEvent','transactionEvents','previousCheckpointRetained','unsavedGraphAvailable','mainRoleAssociated','runnerErrors','elapsedSeconds']}, indent=2))
    print(f'Evidence: {args.result.resolve()}')
    return code


if __name__ == "__main__":
    raise SystemExit(main())
