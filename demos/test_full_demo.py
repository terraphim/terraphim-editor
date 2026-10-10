"""Independent text/count oracles and complete requirement inventory, no browser mocks."""
import unittest
from full_demo import word_count, cut_text, preview_text, promotion_ranges, REQUIREMENTS

class OutputOracles(unittest.TestCase):
    def test_joined_words(self):
        self.assertEqual(word_count("It's a well-made café. A/B _name 42."), 7)
    def test_combining_and_non_bmp(self):
        self.assertEqual(word_count('🦀 Cafe\u0301 e\u0301lan — yes'), 3)
    def test_markdown_and_code_are_counted(self):
        self.assertEqual(word_count('# Title\n\n```\ncode_word\n```\n- one'), 3)
    def test_utf16_exact_cut(self):
        self.assertEqual(cut_text('🦀 Café extra words.', [(8, 14)]), '🦀 Café words.')
    def test_overlap_refused(self):
        with self.assertRaises(ValueError): cut_text('abc def', [(0,4),(3,6)])
    def test_split_surrogate_refused(self):
        with self.assertRaises(ValueError): cut_text('🦀 hi', [(1,2)])
    def test_preview_unions_nested_and_crossing_ranges(self):
        self.assertEqual(preview_text('one two three four',[(0,7),(4,13),(4,7)]),' four')
    def test_promotion_keeps_adjacent_and_deduplicates_containment(self):
        self.assertEqual(promotion_ranges([(0,8),(0,8),(2,5),(8,10)]),[(0,8),(8,10)])
        with self.assertRaises(ValueError): promotion_ranges([(0,7),(4,10)])
    def test_crossing_inside_enclosing_range_refused(self):
        with self.assertRaises(ValueError): promotion_ranges([(0,20),(2,10),(5,15)])
    def test_all_requirement_ids(self):
        expected = {f'R-{section}.{i}' for section,n in [(2,7),(3,6),(4,6),(5,3),(6,5),(7,4),(8,7),(9,3)] for i in range(1,n+1)}
        self.assertEqual(set(REQUIREMENTS), expected)

if __name__ == '__main__': unittest.main()
