"""Diagnostic schema/protocol units; no native browser proof."""
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('safari_diagnostics', ROOT / 'scripts/ci/editor-safari-offline-check.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def sample_with(wire):
    return lambda base, route, timeout: wire.webdriver(base, 'POST', route + '/execute/sync',
                                                      {'script': runner.VISIBILITY_SCRIPT, 'args': []})


class SafeDiagnostics(unittest.TestCase):
    def test_readonly_visibility_transport_uses_remaining_socket_bound(self):
        response = io.BytesIO(json.dumps({'value': {'visibility': 'visible', 'documentFocused': True}}).encode())
        with patch.object(runner.urllib.request, 'urlopen', return_value=response) as request:
            self.assertEqual(runner.visibility_request('http://127.0.0.1:1', '/session/PRIVATE', .025),
                             {'visibility': 'visible', 'documentFocused': True})
        args, kwargs = request.call_args
        self.assertEqual(kwargs['timeout'], .025)
        self.assertEqual(json.loads(args[0].data), {'script': runner.VISIBILITY_SCRIPT, 'args': []})
        self.assertNotIn('focus()', runner.VISIBILITY_SCRIPT)
        with patch.object(runner.urllib.request, 'urlopen', side_effect=TimeoutError('PRIVATE')):
            with self.assertRaises(runner.RequestError) as caught:
                runner.visibility_request('http://127.0.0.1:1', '/session/PRIVATE', .025)
        self.assertEqual(runner.error_record(caught.exception, 'visibility-precondition')['code'], 'webdriver-timeout')
        self.assertNotIn('PRIVATE', str(caught.exception))

    def test_invalid_visibility_and_request_errors_are_not_retried(self):
        with patch.object(runner, 'visibility_request', return_value={'visibility': 'PRIVATE'}) as request, patch('builtins.print'):
            with self.assertRaises(runner.OracleError) as caught:
                runner.wait_visible_window('', '', runner.time.monotonic() + 60)
        self.assertEqual(caught.exception.code, 'environment-visibility-unavailable')
        self.assertEqual(request.call_count, 1)
        error = runner.RequestError('visibility-snapshot', RuntimeError('WebDriver: unknown error: PRIVATE'))
        with patch.object(runner, 'visibility_request', side_effect=error) as request, patch('builtins.print'):
            with self.assertRaises(runner.RequestError):
                runner.wait_visible_window('', '', runner.time.monotonic() + 60)
        self.assertEqual(request.call_count, 1)

    def test_readiness_notice_precedes_poll_and_success_is_bounded(self):
        events = []
        clock = [100.0]
        class Wire:
            @staticmethod
            def webdriver(base, method, route, payload=None):
                events.append('sample')
                assert 'focus()' not in payload['script']
                return {'visibility': 'visible' if clock[0] > 100 else 'hidden',
                        'documentFocused': clock[0] > 100}
        def sleep(duration):
            self.assertLessEqual(duration, .1)
            clock[0] += duration
        with patch.object(runner.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(runner, 'visibility_request', side_effect=sample_with(Wire)), \
             patch.object(runner.time, 'sleep', side_effect=sleep), \
             patch('builtins.print', side_effect=lambda *args, **kwargs: events.append(('notice', args, kwargs))):
            result = runner.wait_visible_window('', '/session/PRIVATE', 160)
        self.assertEqual(events[0], ('notice', (runner.VISIBILITY_NOTICE,), {'flush': True}))
        self.assertEqual(events[1:], ['sample', 'sample'])
        self.assertEqual(result, {'visibility': 'visible', 'documentFocused': True})
        self.assertNotIn('PRIVATE', str(events))

    def test_visibility_wait_exhaustion_respects_remaining_budget_and_cap(self):
        for remaining, expected in [(.05, .05), (60, 10)]:
            clock = [100.0]
            class Wire:
                @staticmethod
                def webdriver(base, method, route, payload=None):
                    return {'visibility': 'hidden', 'documentFocused': False, 'body': 'PRIVATE'}
            with patch.object(runner.time, 'monotonic', side_effect=lambda: clock[0]), \
                 patch.object(runner, 'visibility_request', side_effect=sample_with(Wire)), \
                 patch.object(runner.time, 'sleep', side_effect=lambda duration: clock.__setitem__(0, clock[0] + duration)), \
                 patch('builtins.print'):
                with self.assertRaises(runner.OracleError) as caught:
                    runner.wait_visible_window('', '', 100 + remaining)
            self.assertEqual(caught.exception.code, 'environment-visibility-budget-exhausted')
            self.assertAlmostEqual(clock[0], 100 + expected)
            self.assertEqual(caught.exception.diagnostics['visibility'], 'hidden')
            self.assertNotIn('PRIVATE', str(caught.exception.diagnostics))

    def test_visibility_wait_never_accepts_late_success_or_samples_expired_budget(self):
        clock = [100.0]
        class Wire:
            calls = 0
            @classmethod
            def webdriver(cls, base, method, route, payload=None):
                cls.calls += 1
                clock[0] = 111.0
                return {'visibility': 'visible', 'documentFocused': True}
        with patch.object(runner.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(runner, 'visibility_request', side_effect=sample_with(Wire)), patch('builtins.print'):
            with self.assertRaises(runner.OracleError):
                runner.wait_visible_window('', '', 160)
            self.assertEqual(Wire.calls, 1)
            with self.assertRaises(runner.OracleError):
                runner.wait_visible_window('', '', 110)
            self.assertEqual(Wire.calls, 1)

    def test_owned_window_requires_exact_single_session_handle(self):
        self.assertEqual(runner.validate_owned_window('PRIVATE-HANDLE', ['PRIVATE-HANDLE']), 'PRIVATE-HANDLE')
        for current, handles in [(None, []), ('PRIVATE-HANDLE', ['OTHER']),
                                  ('PRIVATE-HANDLE', ['PRIVATE-HANDLE', 'PERSONAL']),
                                  ('PRIVATE-HANDLE', ['PRIVATE-HANDLE'] * 2), ('', ['']),
                                  ('x', 'x')]:
            with self.assertRaises(runner.OracleError) as caught:
                runner.validate_owned_window(current, handles)
            record = runner.error_record(caught.exception, 'window-ownership')
            self.assertEqual(record['code'], 'environment-window-ownership')
            self.assertNotIn('PRIVATE', str(record))
            self.assertNotIn('PERSONAL', str(record))

    def test_visibility_requires_visible_and_true_focus(self):
        self.assertEqual(runner.visibility_code({'visibility': 'visible', 'documentFocused': True}), None)
        for value, code in [({'visibility': 'hidden', 'documentFocused': True}, 'environment-document-hidden'),
                            ({'visibility': 'visible', 'documentFocused': False}, 'environment-document-unfocused'),
                            ({'visibility': 'visible', 'documentFocused': 1}, 'environment-visibility-unavailable'),
                            ({'visibility': 'PRIVATE', 'documentFocused': False}, 'environment-visibility-unavailable'),
                            (None, 'environment-visibility-unavailable')]:
            self.assertEqual(runner.visibility_code(value), code)

    def test_window_precondition_targets_only_owned_handle_and_gates_visibility(self):
        class Wire:
            calls = []
            @classmethod
            def webdriver(cls, base, method, route, payload=None):
                cls.calls.append((method, route, payload))
                if route.endswith('/window/handles'): return ['PRIVATE-HANDLE']
                if method == 'GET' and route.endswith('/window'): return 'PRIVATE-HANDLE'
                if route.endswith('/execute/sync'):
                    return {'visibility': 'hidden', 'documentFocused': False, 'body': 'PRIVATE-BODY'}
                return None
        with patch.object(runner, 'visibility_request', side_effect=sample_with(Wire)), patch('builtins.print'):
            with self.assertRaises(runner.OracleError) as caught:
                runner.visible_window_precondition(Wire, '', '/session/test', 'PRIVATE-HANDLE', True, runner.time.monotonic() + .001)
        self.assertEqual(caught.exception.code, 'environment-visibility-budget-exhausted')
        self.assertNotIn('PRIVATE', str(caught.exception.diagnostics))
        switch = [c for c in Wire.calls if c[0] == 'POST' and c[1].endswith('/window')]
        self.assertEqual(switch[0][2], {'handle': 'PRIVATE-HANDLE'})
        rect = [c for c in Wire.calls if c[1].endswith('/window/rect')]
        self.assertEqual(rect[0][2], {'width': 1280, 'height': 900})
        self.assertFalse(any('maximize' in c[1] or 'screenshot' in c[1] or 'execute/async' in c[1] for c in Wire.calls))
        Wire.calls.clear()
        with patch.object(runner, 'visibility_request', side_effect=sample_with(Wire)), patch('builtins.print'):
            with self.assertRaises(runner.OracleError):
                runner.visible_window_precondition(Wire, '', '/session/test', 'PRIVATE-HANDLE', False, runner.time.monotonic() + .001)
        self.assertFalse(any(c[1].endswith('/window/rect') for c in Wire.calls))

    def test_handle_mismatch_stops_before_switch_rect_or_script(self):
        class Wire:
            calls = []
            @classmethod
            def webdriver(cls, base, method, route, payload=None):
                cls.calls.append(method)
                return ['FOREIGN-HANDLE'] if route.endswith('/handles') else 'FOREIGN-HANDLE'
        with self.assertRaises(runner.OracleError):
            runner.visible_window_precondition(Wire, '', '/session/test', 'OWNED-HANDLE', True, runner.time.monotonic() + 60)
        self.assertTrue(all(method == 'GET' for method in Wire.calls))

    def test_visible_owned_window_passes_only_curated_state(self):
        class Wire:
            @staticmethod
            def webdriver(base, method, route, payload=None):
                if route.endswith('/handles'): return ['OWNED']
                if method == 'GET': return 'OWNED'
                if route.endswith('/execute/sync'):
                    return {'visibility': 'visible', 'documentFocused': True, 'body': 'PRIVATE'}
                return None
        with patch.object(runner, 'visibility_request', side_effect=sample_with(Wire)), patch('builtins.print'):
            self.assertEqual(runner.visible_window_precondition(Wire, '', '/session/test', 'OWNED', False, runner.time.monotonic() + 60),
                             {'visibility': 'visible', 'documentFocused': True})
        subprocess.run(['node', '--check'], input=runner.VISIBILITY_SCRIPT.replace('return {', 'const state = {'),
                       text=True, capture_output=True, timeout=10, check=True)

    def test_window_command_error_is_finite_and_visibility_precedes_positive(self):
        class Wire:
            @staticmethod
            def webdriver(base, method, route, payload=None):
                raise RuntimeError('WebDriver HTTP 500: unsupported operation: PRIVATE-HANDLE URL BODY')
        with self.assertRaises(runner.RequestError) as caught:
            runner.session_window(Wire, '', '/session/private')
        record = runner.error_record(caught.exception, 'window-ownership')
        self.assertEqual(record['request'], 'get-window-handle')
        self.assertEqual(record['code'], 'webdriver-unsupported-operation')
        self.assertNotIn('PRIVATE', str(record))
        source = (ROOT / 'scripts/ci/editor-safari-offline-check.py').read_text()
        self.assertLess(source.index('visibility = visible_window_precondition'),
                        source.index('positive = bounded_probe'))

    def test_test_server_policy_allows_only_self_and_nonnetwork_data_connections(self):
        directives = {parts[0]: parts[1:] for clause in runner.CSP.split(';')
                      if (parts := clause.strip().split())}
        self.assertEqual(directives['connect-src'], ["'self'", 'data:'])
        self.assertEqual(directives, {
            'default-src': ["'self'"],
            'script-src': ["'self'", "'unsafe-inline'", "'wasm-unsafe-eval'"],
            'style-src': ["'self'", "'unsafe-inline'"],
            'font-src': ["'self'"], 'connect-src': ["'self'", 'data:'],
            'worker-src': ["'self'"], 'img-src': ["'self'", 'data:'],
            'base-uri': ["'self'"], 'object-src': ["'none'"],
        })
        for forbidden in ('https:', 'http:', '*', 'blob:', 'javascript:'):
            self.assertNotIn(forbidden, directives['connect-src'])

    def test_positive_subphase_copy_preserves_original_operations(self):
        probes = runner.module('positive_probe_unit', ROOT / 'scripts/ci/editor-offline-check.py')
        original = probes.PROBE
        instrumented = runner.instrument_positive_probe(original)
        self.assertEqual(probes.PROBE, original)
        self.assertIn("'positive-dialog-show'", instrumented)
        self.assertIn("'positive-dialog-show-resolved'", instrumented)
        self.assertIn("'positive-dialog-hide'", instrumented)
        self.assertEqual(instrumented.count('await dialog.show();'), 1)
        self.assertEqual(instrumented.count('await dialog.hide();'), 1)
        self.assertIn('observeDiagnosticDialog(dialog)', instrumented)
        with self.assertRaisesRegex(ValueError, 'positive-diagnostic-source-contract'):
            runner.instrument_positive_probe(original.replace('await dialog.show();', 'changedOperation();'))

    def test_exact_failed_webdriver_request_is_retained_without_session_url(self):
        class TimeoutWire:
            @staticmethod
            def webdriver(base, method, route, payload):
                raise RuntimeError('WebDriver: timeout: PRIVATE SESSION URL')
        with self.assertRaises(runner.RequestError) as caught:
            runner.async_function(TimeoutWire, 'http://127.0.0.1:1', '/session/private-id', runner.FORMAT)
        record = runner.error_record(caught.exception, 'positive', 'fullPage')
        self.assertEqual(record['request'], 'execute-async')
        self.assertEqual(record['code'], 'webdriver-timeout')
        self.assertNotIn('private-id', str(record))
        self.assertNotIn('PRIVATE', str(record))

    def test_dialog_snapshot_is_finite_and_nontextual(self):
        projected = runner.sanitize_snapshot({'asyncBridge': 'entered', 'visibility': 'visible',
            'documentFocused': False, 'dialogEvents': {'show': 1, 'afterShow': 0, 'private': 'BODY'},
            'dialogAnimationStates': ['running', 'finished', 'private BODY'], 'dialogPanelHidden': False})
        self.assertEqual(projected['dialogEvents'], {'show': 1, 'afterShow': 0})
        self.assertEqual(projected['dialogAnimationStates'], ['running', 'finished'])
        self.assertEqual(projected['asyncBridge'], 'entered')
        self.assertNotIn('BODY', str(projected))

    def test_script_timeout_configuration_request_is_distinguished(self):
        class TimeoutWire:
            @staticmethod
            def webdriver(base, method, route, payload):
                raise RuntimeError('WebDriver: timeout: PRIVATE')
        with self.assertRaises(runner.RequestError) as caught:
            runner.bounded_probe(TimeoutWire, '', '/private', runner.FORMAT, runner.time.monotonic() + 60)
        self.assertEqual(runner.error_record(caught.exception, 'positive')['request'], 'set-script-timeout')

    def test_passive_dialog_event_observer_and_generated_scripts(self):
        script = r'''const vm=require('node:vm'),fs=require('node:fs'),assert=require('node:assert/strict');
const input=JSON.parse(fs.readFileSync(0,'utf8')), window={}, listeners={};
const dialog={addEventListener(name,fn){listeners[name]=fn;}};
vm.runInNewContext(input.observer+';observeDiagnosticDialog(dialog);',{window,dialog});
listeners['sl-show']({target:dialog}); listeners['sl-after-show']({target:{}});
listeners['sl-after-show']({target:dialog}); listeners['sl-hide']({target:dialog});
listeners['sl-after-hide']({target:dialog});
assert.deepEqual(JSON.parse(JSON.stringify(window.__offlineDialogEvents)),{show:1,afterShow:1,hide:1,afterHide:1});
for (const fn of input.functions) new vm.Script('('+fn+')');
new vm.Script(input.bridge);
'''
        class Wire:
            @staticmethod
            def webdriver(base, method, route, payload):
                return payload['script']
        probes = runner.module('positive_probe_syntax', ROOT / 'scripts/ci/editor-offline-check.py')
        positive = runner.instrument_positive_probe(probes.PROBE)
        subprocess.run(['node', '-e', script], input=json.dumps({
            'observer': runner.DIALOG_DIAGNOSTIC_JS,
            'functions': [positive, runner.SNAPSHOT, runner.FORMAT, runner.NEGATIVE],
            'bridge': runner.async_function(Wire, '', '', positive)}),
            text=True, capture_output=True, timeout=10, check=True)

    def test_known_page_code_and_phase_survive_bridge(self):
        with self.assertRaises(runner.OracleError) as caught:
            runner.checked_value({'ok': False, 'code': 'font-glyph-style',
                                  'diagnostics': {'readyState': 'complete', 'body': 'PRIVATE CONTENT'}})
        error = runner.error_record(caught.exception, 'formatting', 'fullPage')
        self.assertEqual(error['code'], 'font-glyph-style')
        self.assertEqual(error['phase'], 'formatting')
        self.assertEqual(error['case'], 'fullPage')
        self.assertNotIn('PRIVATE', str(error))

    def test_unknown_error_content_never_escapes(self):
        error = runner.error_record(RuntimeError('private/path credential body alias'), 'positive', 'embed')
        self.assertEqual(error['code'], 'unclassified')
        self.assertNotIn('private', str(error))
        with self.assertRaises(runner.OracleError) as caught:
            runner.checked_value({'ok': False, 'code': 'private secret', 'message': 'PRIVATE BODY'})
        self.assertEqual(caught.exception.code, 'unknown-page-error')

    def test_snapshot_allowlist_rejects_arbitrary_strings_and_fields(self):
        value = runner.sanitize_snapshot({'readyState': 'complete', 'probeStep': 'negative-module',
            'editorReady': True, 'surfaceText': 'PRIVATE', 'readyVariable': 'secret',
            'components': {'input': True, 'private-name': True},
            'csp': [{'directive': 'connect-src', 'blocked': 'negative-control', 'trusted': True},
                    {'directive': 'private', 'blocked': 'https://private.invalid/body', 'trusted': True}]})
        self.assertEqual(value['components'], {'input': True})
        self.assertEqual(value['csp'], [{'directive': 'connect-src', 'blocked': 'negative-control', 'trusted': True}])
        self.assertEqual(value['probeStep'], 'negative-module')
        self.assertNotIn('PRIVATE', str(value))
        self.assertNotIn('private', str(value))

    def test_wire_script_classifies_without_returning_message_or_stack(self):
        calls = []
        class WireDouble:
            @staticmethod
            def webdriver(base, method, route, payload):
                calls.append(payload['script'])
                return {'ok': False, 'code': 'unknown-page-error'}
        runner.async_function(WireDouble, 'http://127.0.0.1:1', '/session/owned', runner.FORMAT)
        self.assertIn('code:', calls[0])
        self.assertIn('diagnostics:', calls[0])
        self.assertNotIn('stack:', calls[0])
        self.assertNotIn('message:error', calls[0])

    def test_webdriver_typed_error_classification_is_bounded(self):
        error = runner.error_record(RuntimeError('WebDriver: script timeout: private body'), 'positive', 'fullPage')
        self.assertEqual(error['code'], 'webdriver-script-timeout')
        self.assertNotIn('private', str(error))

    def test_malformed_page_values_remain_sanitized(self):
        self.assertEqual(runner.sanitize_snapshot({'readyState': [], 'probeStep': {},
            'csp': [{'directive': [], 'blocked': {}, 'trusted': True}]}), {'csp': []})
        with self.assertRaises(runner.OracleError) as caught:
            runner.checked_value({'ok': False, 'code': ['private'], 'diagnostics': 'private'})
        self.assertEqual(caught.exception.code, 'unknown-page-error')

    def test_scheme_only_csp_target_normalizes_without_raw_uri(self):
        # JS snapshot unit in confined VM, not a Safari observation.
        script = r'''const vm=require('node:vm'), fs=require('node:fs'), assert=require('node:assert/strict');
const snapshot=JSON.parse(fs.readFileSync(0,'utf8')).snapshot;
const value=vm.runInNewContext('('+snapshot+')()',{
 window:{__offlineViolations:[{directive:'connect-src',blockedURI:'data',trusted:true}]},
 document:{readyState:'complete',getElementById(){return null;},hasFocus(){return false;}},
 customElements:{get(){return undefined;}},location:{origin:'http://127.0.0.1:1'},URL,
});
assert.equal(value.csp[0].blocked,'data');
assert.equal('blockedURI' in value.csp[0],false);
'''
        subprocess.run(['node', '-e', script], input=json.dumps({'snapshot': runner.SNAPSHOT}),
                       text=True, capture_output=True, timeout=10, check=True)

    def test_finite_uri_classifier_and_fetch_delegate(self):
        script = r'''const vm=require('node:vm'),fs=require('node:fs'),assert=require('node:assert/strict');
const code=JSON.parse(fs.readFileSync(0,'utf8')).code;
class RequestDouble {constructor(url){this.url=url;}}
const {classifyURI,instrumentFetch}=vm.runInNewContext(code+';({classifyURI,instrumentFetch})',{URL,Request:RequestDouble});
const base='http://127.0.0.1:1/uuid/nested/editor/index.html';
for(const [value,expected] of [['data','data'],['DATA:','data'],['data:image/svg+xml,PRIVATE','data'],
 ['blob','blob'],['blob:private','blob'],['inline','inline'],['eval','eval'],['','empty'],
 ['databaseexample','other'],['private alias','other'],[null,'other'],['data:'+ 'x'.repeat(33000),'other'],
 ['https://remote.invalid/PRIVATE?secret=1','external-network'],['http://127.0.0.1:1/PRIVATE','self']]) {
 assert.equal(classifyURI(value,base),expected);
}
const promise=Promise.resolve('synthetic');let captured;
const window={fetch:function(...args){captured={receiver:this,args};return promise;}};
instrumentFetch(window,{baseURI:base});
const input=new RequestDouble('data:image/svg+xml,PRIVATE');const options={fixture:true};
assert.equal(window.fetch(input,options),promise);assert.equal(captured.receiver,window);
assert.equal(captured.args[0],input);assert.equal(captured.args[1],options);
assert.equal(window.__offlineFetchCounts.production.data,1);
window.__offlineProbeStep='negative-fetch';window.fetch('https://connect.denied.invalid/probe');
assert.equal(window.__offlineFetchCounts.negative['negative-control'],1);
assert.equal(JSON.stringify(window.__offlineFetchCounts).includes('PRIVATE'),false);
assert.equal(JSON.stringify(window.__offlineFetchCounts).includes('https://'),false);
'''
        subprocess.run(['node', '-e', script], input=json.dumps({'code': runner.URI_DIAGNOSTIC_JS}),
                       text=True, capture_output=True, timeout=10, check=True)

    def test_event_target_retains_only_category_and_fixed_public_origin(self):
        script = r'''const vm=require('node:vm'),fs=require('node:fs'),assert=require('node:assert/strict');
const code=JSON.parse(fs.readFileSync(0,'utf8')).early;const listeners={};
const window={fetch(){return Promise.resolve();}};
vm.runInNewContext(code,{window,document:{baseURI:'http://127.0.0.1:1/index.html'},URL,Request:class {},
 addEventListener(name,fn){listeners[name]=fn;}});
listeners.securitypolicyviolation({effectiveDirective:'connect-src',blockedURI:'data:image/svg+xml,PRIVATE',isTrusted:true});
listeners.securitypolicyviolation({effectiveDirective:'connect-src',blockedURI:'https://connect.denied.invalid/PRIVATE?credential=1',isTrusted:true});
assert.equal(window.__offlineViolations[0].blocked,'data');
assert.equal(window.__offlineViolations[0].blockedURI,'');
assert.equal(window.__offlineViolations[1].blockedURI,'https://connect.denied.invalid');
assert.equal(JSON.stringify(window.__offlineViolations).includes('PRIVATE'),false);
assert.equal(JSON.stringify(window.__offlineViolations).includes('credential'),false);
'''
        early = runner.EARLY.removeprefix('<script>').removesuffix('</script>')
        subprocess.run(['node', '-e', script], input=json.dumps({'early': early}),
                       text=True, capture_output=True, timeout=10, check=True)


if __name__ == '__main__':
    unittest.main()
