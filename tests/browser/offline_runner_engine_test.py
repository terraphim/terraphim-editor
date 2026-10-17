"""Engine-selection/report unit doubles only; no real browser qualification."""
import importlib.util
import json
import hashlib
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('offline_engine_unit', ROOT / 'scripts/ci/editor-offline-check.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class BrowserTypeDouble:
    def __init__(self):
        self.calls = []
        self.browser = object()

    def launch(self, **options):
        self.calls.append(options)
        return self.browser


class PlaywrightDouble:
    def __init__(self):
        self.chromium = BrowserTypeDouble()
        self.firefox = BrowserTypeDouble()


class EngineSelection(unittest.TestCase):
    def executable_fixture(self, temp):
        path = Path(temp) / 'firefox'
        path.write_bytes(b'symbolic unit executable, not a browser')
        path.chmod(0o700)
        return path, hashlib.sha256(path.read_bytes()).hexdigest()

    def test_explicit_firefox_launch_and_identity(self):
        with tempfile.TemporaryDirectory(dir=ROOT / 'target') as temp:
            path, digest = self.executable_fixture(temp)
            executable = runner.validate_executable('firefox', path, digest)
            playwright = PlaywrightDouble()
            self.assertIs(runner.launch_browser(playwright, 'firefox', executable), playwright.firefox.browser)
            self.assertEqual(playwright.firefox.calls, [{'headless': True, 'executable_path': str(path)}])
            class Browser:
                version = 'symbolic-nightly-version'
            class Page:
                def evaluate(self, _):
                    return 'symbolic-UA'
            identity = runner.browser_metadata(Browser(), Page(), 'firefox', executable)
            self.assertEqual(identity['browserChannel'], 'explicit-executable')
            self.assertEqual(identity['executablePath'], str(path))
            self.assertEqual(identity['executableSha256'], digest)
            self.assertEqual(identity['browserVersion'], Browser.version)

    def test_override_requires_firefox_absolute_existing_executable_and_checksum(self):
        with tempfile.TemporaryDirectory(dir=ROOT / 'target') as temp:
            path, digest = self.executable_fixture(temp)
            directory = path.parent / 'directory' / 'firefox'
            directory.mkdir(parents=True)
            for engine, candidate, checksum in [
                ('chrome', path, digest), ('chromium', path, digest), ('safari', path, digest),
                ('firefox', Path('firefox'), digest), ('firefox', path.parent / 'absent' / 'firefox', digest),
                ('firefox', directory, digest), ('firefox', path.parent / 'other-browser', digest),
                ('firefox', path, '0' * 64),
                ('firefox', path, None), ('firefox', None, digest),
                ('firefox', path, 'not-a-sha256')]:
                with self.subTest(engine=engine, candidate=candidate, checksum=checksum):
                    with self.assertRaises(ValueError):
                        runner.validate_executable(engine, candidate, checksum)
            path.chmod(0o600)
            with self.assertRaises(ValueError):
                runner.validate_executable('firefox', path, digest)

    def test_override_rejects_symlink_file_and_ancestor(self):
        with tempfile.TemporaryDirectory(dir=ROOT / 'target') as temp:
            path, digest = self.executable_fixture(temp)
            directory = path.parent / 'linked'
            directory.symlink_to(path.parent, target_is_directory=True)
            link = path.parent / 'alias'
            link.symlink_to(path)
            for candidate in (link, directory / 'firefox'):
                with self.assertRaises(ValueError):
                    runner.validate_executable('firefox', candidate, digest)

    def test_binary_drift_refuses_launch(self):
        with tempfile.TemporaryDirectory(dir=ROOT / 'target') as temp:
            path, digest = self.executable_fixture(temp)
            executable = runner.validate_executable('firefox', path, digest)
            path.write_bytes(b'changed')
            playwright = PlaywrightDouble()
            with self.assertRaises(ValueError):
                runner.launch_browser(playwright, 'firefox', executable)
            self.assertEqual(playwright.firefox.calls, [])

    def test_post_launch_drift_closes_browser(self):
        with tempfile.TemporaryDirectory(dir=ROOT / 'target') as temp:
            path, digest = self.executable_fixture(temp)
            executable = runner.validate_executable('firefox', path, digest)
            class Browser:
                closed = False
                def close(self):
                    self.closed = True
            browser = Browser()
            playwright = PlaywrightDouble()
            def launch(**_):
                path.write_bytes(b'post-launch drift')
                return browser
            playwright.firefox.launch = launch
            with self.assertRaises(ValueError):
                runner.launch_browser(playwright, 'firefox', executable)
            self.assertTrue(browser.closed)

    def test_invalid_cli_override_records_not_run_without_browser(self):
        with tempfile.TemporaryDirectory(dir=ROOT / 'target') as temp:
            report = Path(temp) / 'receipt.json'
            with self.assertRaises(SystemExit) as error:
                runner.main(['unused-distribution', '--engine', 'safari', '--report', str(report),
                             '--executable-path', '/missing/firefox', '--executable-sha256', '0' * 64])
            self.assertEqual(error.exception.code, 2)
            receipt = json.loads(report.read_text())
            self.assertEqual(receipt['status'], 'NOT RUN')
            self.assertEqual(receipt['phase'], 'executable-validation')
            self.assertNotIn('/missing', json.dumps(receipt))

    def test_installed_chrome_explicit_channel(self):
        playwright = PlaywrightDouble()
        self.assertIs(runner.launch_browser(playwright, 'chrome'), playwright.chromium.browser)
        self.assertEqual(playwright.chromium.calls, [{'channel': 'chrome', 'headless': True}])
        self.assertEqual(playwright.firefox.calls, [])

    def test_cached_engines_preserve_launch_arguments(self):
        for engine in ('chromium', 'firefox'):
            with self.subTest(engine=engine):
                playwright = PlaywrightDouble()
                chosen = getattr(playwright, engine)
                self.assertIs(runner.launch_browser(playwright, engine), chosen.browser)
                self.assertEqual(chosen.calls, [{'headless': True}])

    def test_safari_and_webkit_cannot_be_substituted(self):
        for engine in ('safari', 'webkit', 'unknown'):
            playwright = PlaywrightDouble()
            with self.assertRaises(ValueError):
                runner.launch_browser(playwright, engine)
            self.assertEqual(playwright.chromium.calls, [])
            self.assertEqual(playwright.firefox.calls, [])

    def test_metadata_uses_runtime_version_and_user_agent(self):
        class RuntimeBrowserDouble:
            version = 'synthetic-runtime-version-not-qualification'
        class PageDouble:
            def evaluate(self, expression):
                self.expression = expression
                return 'synthetic-UA-not-browser-evidence'
        page = PageDouble()
        metadata = runner.browser_metadata(RuntimeBrowserDouble(), page, 'chrome')
        self.assertEqual(page.expression, 'navigator.userAgent')
        self.assertEqual(metadata['browserVersion'], RuntimeBrowserDouble.version)
        self.assertEqual(metadata['userAgent'], 'synthetic-UA-not-browser-evidence')
        self.assertEqual(metadata['browserChannel'], 'chrome')
        self.assertEqual(runner.browser_metadata(RuntimeBrowserDouble(), page, 'chromium')['browserChannel'], 'cached')

    def test_launch_failure_is_not_run_and_redacts_exception_message(self):
        error = RuntimeError('private/path and sensitive launch environment must not be copied')
        report = runner.launch_failure('chrome', error)
        self.assertEqual(report['status'], 'NOT RUN')
        self.assertEqual(report['phase'], 'browser-launch')
        self.assertEqual(report['errorClass'], 'RuntimeError')
        self.assertNotIn('private/path', json.dumps(report))
        self.assertNotIn('browserVersion', report)

    def test_reports_preserve_existing_evidence(self):
        scratch = ROOT / 'target/offline-engine-unit'
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as temp:
            report = Path(temp) / 'nested/report.json'
            runner.write_report(report, {'status': 'NOT RUN'})
            with self.assertRaises(FileExistsError):
                runner.write_report(report, {'status': 'PASS'})
            self.assertEqual(json.loads(report.read_text()), {'status': 'NOT RUN'})


if __name__ == '__main__':
    unittest.main()
