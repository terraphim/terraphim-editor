"""Native Safari runner scaffolding tests; protocol doubles are NOT browser proof."""
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import threading
import unittest
from unittest import mock
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('safari_offline', ROOT / 'scripts/ci/editor-safari-offline-check.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class NativeOfflineScaffolding(unittest.TestCase):
    def test_policy_and_path_boundary(self):
        for directive in ["default-src 'self'", "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'",
                          "style-src 'self' 'unsafe-inline'", "font-src 'self'", "connect-src 'self'",
                          "worker-src 'self'", "img-src 'self' data:", "base-uri 'self'", "object-src 'none'"]:
            self.assertIn(directive, runner.CSP)
        self.assertNotIn('https:', runner.CSP)
        self.assertNotIn("'unsafe-eval'", runner.CSP)
        self.assertTrue(runner.safe_path('/abc/nested/editor/index.html'))
        for path in ['/../index.html', '/abc/../a', '//abc/a', '/abc/%2e%2e/a',
                     '/abc\\a', '/abc//a', '/abc/./a', 'index.html']:
            self.assertFalse(runner.safe_path(path), path)

    def test_real_local_http_headers_and_exact_allowlist(self):
        server = runner.PolicyServer({'/token/index.html': b'<!doctype html><title>synthetic fixture</title>'})
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            origin = f'http://127.0.0.1:{server.server_port}'
            with urllib.request.urlopen(origin + '/token/index.html?ignored=1', timeout=3) as response:
                self.assertEqual(response.headers['Content-Security-Policy'], runner.CSP)
                self.assertEqual(response.headers['Cache-Control'], 'no-store, max-age=0')
                self.assertEqual(response.headers['X-Content-Type-Options'], 'nosniff')
                self.assertIn(b'synthetic fixture', response.read())
            for path in ['/unknown', '/token/%2e%2e/index.html', '/token/../index.html']:
                with self.assertRaises(urllib.error.HTTPError) as caught:
                    urllib.request.urlopen(origin + path, timeout=3)
                self.assertEqual(caught.exception.code, 404)
                self.assertEqual(caught.exception.headers['Content-Security-Policy'], runner.CSP)
                caught.exception.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=3)
        self.assertFalse(thread.is_alive())

    def test_capture_preserves_real_vendor_and_hashes_instrumented_pages(self):
        # Synthetic delivery glue/index: validates capture, not WASM readiness.
        scratch = ROOT / 'target/safari-offline-unit'
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as temp:
            delivery = Path(temp) / 'delivery'
            delivery.mkdir()
            shutil.copytree(ROOT / 'public/vendor', delivery / 'vendor')
            for name in ('terraphim-editor.min.js', 'terraphim_editor.js', 'terraphim_editor_bg.wasm',
                         'terraphim-editor.min.css'):
                (delivery / name).write_bytes(b'synthetic protocol fixture')
            (delivery / 'index.html').write_text('<!doctype html><html><head></head><body></body></html>')
            (delivery / 'private.txt').write_bytes(b'not in served allowlist')
            payload, source, staged = runner.capture(delivery, 'a' * 32)
            prefix = '/' + 'a' * 32
            self.assertNotIn('private.txt', source)
            self.assertNotIn(prefix + '/nested/editor/private.txt', payload)
            self.assertIn(runner.EARLY.encode(), payload[prefix + '/nested/editor/index.html'])
            self.assertEqual(payload[prefix + '/fixture/thesaurus.json'],
                             (ROOT / 'tests/fixtures/kg/thesaurus.json').read_bytes())
            for name, data in payload.items():
                self.assertEqual(staged[name]['sha256'], runner.hashlib.sha256(data).hexdigest())
            self.assertNotEqual(source['index.html']['sha256'], staged[prefix + '/nested/editor/index.html']['sha256'])
            (delivery / 'linked.js').symlink_to(ROOT / 'public/js/config.js')
            with self.assertRaisesRegex(ValueError, 'symlink'):
                runner.capture(delivery, 'b' * 32)

    def test_native_capabilities_and_async_wire_contract(self):
        safari = runner.module('native_scaffolding', ROOT / 'scripts/ci/rolegraph-safari-check.py')
        self.assertEqual(safari.session_request()['capabilities']['alwaysMatch'],
                         {'browserName': 'safari', 'platformName': 'mac'})
        safari.validate_capabilities({'browserName': 'Safari', 'platformName': 'macOS'})
        for capability in [{'browserName': 'webkit', 'platformName': 'mac'},
                           {'browserName': 'safari', 'platformName': 'iOS'}, {'browserName': 'safari'}]:
            with self.assertRaises(RuntimeError):
                safari.validate_capabilities(capability)
        calls = []
        class WireDouble:
            @staticmethod
            def webdriver(base, method, route, payload):
                calls.append((base, method, route, payload))
                return {'ok': True, 'value': {'synthetic': True}}
        result = runner.async_function(WireDouble, 'http://127.0.0.1:1', '/session/owned', runner.NEGATIVE)
        self.assertEqual(runner.checked_value(result), {'synthetic': True})
        self.assertEqual(calls[0][2], '/session/owned/execute/async')
        self.assertIn('security', runner.EARLY)
        self.assertIn('missing-csp-denial-event', calls[0][3]['script'])
        self.assertIn('requestRejected:true', calls[0][3]['script'])
        with self.assertRaises(RuntimeError):
            runner.checked_value({'ok': False, 'error': 'must not copy page contents'})

    def test_negative_controls_require_observable_events_not_dns_failure(self):
        # Source/protocol guards only: actual denial requires native browser run.
        for name in ('connect.denied.invalid', 'script.denied.invalid', 'style.denied.invalid'):
            self.assertIn(name, runner.NEGATIVE)
        self.assertIn('e.directive.startsWith(control.directive)', runner.NEGATIVE)
        self.assertIn('e.blockedURI.startsWith(new URL(control.url).origin)', runner.NEGATIVE)
        self.assertIn('if (!rejected)', runner.NEGATIVE)
        self.assertIn('window.__offlineViolations.length', runner.FORMAT)
        self.assertIn('button.click()', runner.FORMAT)
        self.assertIn('font-glyph-style', runner.FORMAT)
        self.assertIn('help.click()', runner.FORMAT)
        self.assertIn('e.trusted === true', runner.NEGATIVE)

    def test_missing_asset_fails_real_preflight_before_native_driver(self):
        scratch = ROOT / 'target/safari-offline-unit'
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as temp:
            delivery = Path(temp) / 'delivery'
            delivery.mkdir()  # Missing vendor: real checker must fail first.
            report = Path(temp) / 'result.json'
            with mock.patch.object(runner.subprocess, 'check_output') as version:
                status = runner.main([str(delivery), '--result', str(report),
                                      '--profile-label', 'synthetic-missing-asset', '--scratch', str(Path(temp) / 'scratch')])
            self.assertEqual(status, 1)
            version.assert_not_called()
            result = json.loads(report.read_text())
            self.assertFalse(result['ok'])
            self.assertEqual(result['cases'], [])
            self.assertEqual(result['errors'][0]['errorClass'], 'CalledProcessError')


if __name__ == '__main__':
    unittest.main()
