"""Contract checks for the literal browser examples (the browser tests use real WASM)."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location('lab_outputs', pathlib.Path(__file__).with_name('verify-lab-outputs.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class LiteralExamples(unittest.TestCase):
    def test_unicode_offsets_address_literal_terms(self):
        self.assertEqual(module.expected_ranges('🦀 Café perhaps.', ['perhaps']),
                         [{'start': 8, 'end': 15, 'text': 'perhaps'}])

    def test_missing_expected_term_is_an_error(self):
        with self.assertRaises(ValueError):
            module.expected_ranges('Actual text.', ['absent'])

    def test_every_action_has_positive_and_negative_examples(self):
        examples = module.cases()
        self.assertEqual(len({action for action, _, _ in examples}), 6)
        for action in {action for action, _, _ in examples}:
            self.assertEqual([bool(terms) for name, _, terms in examples if name == action], [True, False])


if __name__ == '__main__':
    unittest.main()
