"""Real staged-file regressions for the production publisher inventory."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("publisher", ROOT / "scripts/publish-editor-dist.py")
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)


class ReleaseInventoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.stage = Path(self.temp.name).resolve()
        for name in P.FIXED:
            self.write(name, b"")
        self.write("vendor/manifest.json", b'{"files": []}')
        self.write("terraphim-editor-1234.js", b"")
        self.write("terraphim-editor-1234_bg.wasm", b"wasm")

    def write(self, name, data):
        path = self.stage / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def test_unpadded_trunk_css_hash_and_worker_declarations_are_hashed(self):
        names = {"persistence-cb27cfbe192754a.css", "tokens-1.css"} | P.DECLARATIONS
        for name in names:
            self.write(name, b"actual staged bytes")
        inventory = P.inventory(self.stage)
        for name in names:
            self.assertEqual(inventory[name], P.digest(b"actual staged bytes"))

    def test_unknown_declarations_and_invalid_css_names_fail_closed(self):
        for name in ("unexpected.d.ts", "tokens-.css", "tokens-123456789abcdef01.css"):
            with self.subTest(name=name):
                self.write(name, b"unadmitted")
                with self.assertRaisesRegex(ValueError, "unexpected non-production"):
                    P.inventory(self.stage)
                (self.stage / name).unlink()

    def test_optional_declarations_do_not_relax_required_assets(self):
        for name in P.DECLARATIONS:
            self.write(name, b"type declaration")
        (self.stage / "roles/default.json").unlink()
        with self.assertRaisesRegex(ValueError, "missing production files"):
            P.inventory(self.stage)
