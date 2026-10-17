"""Real filesystem negative tests, no browser acceptance claim."""
import importlib.util
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('checker', ROOT / 'scripts/ci/runtime-assets-check.py')
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class AssetIntegrity(unittest.TestCase):
    def test_real_source(self):
        self.assertEqual(checker.check(ROOT / 'public/vendor'), 107)

    def test_tampering_and_missing_assets_fail(self):
        paths = ['shoelace/2.12.0/cdn/components/input/input.js',
                 'fontawesome/6.5.2/webfonts/fa-solid-900.woff2',
                 'shoelace/2.12.0/cdn/assets/icons/code.svg',
                 'fontawesome/6.5.2/LICENSE.txt', 'manifest.json']
        for path in paths:
            with self.subTest(path=path), tempfile.TemporaryDirectory() as temp:
                vendor = Path(temp) / 'vendor'
                shutil.copytree(ROOT / 'public/vendor', vendor)
                (vendor / path).write_bytes(b'tampered')
                with self.assertRaises(ValueError):
                    checker.check(vendor)
                (vendor / path).unlink()
                with self.assertRaises((ValueError, OSError)):
                    checker.check(vendor)

    def test_unexpected_file_and_symlink_fail(self):
        with tempfile.TemporaryDirectory() as temp:
            vendor = Path(temp) / 'vendor'
            shutil.copytree(ROOT / 'public/vendor', vendor)
            extra = vendor / 'extra'
            extra.write_bytes(b'not approved')
            with self.assertRaises(ValueError):
                checker.check(vendor)
            extra.unlink()
            extra.symlink_to(ROOT / 'index.html')
            with self.assertRaises(ValueError):
                checker.check(vendor)


if __name__ == '__main__':
    unittest.main()
