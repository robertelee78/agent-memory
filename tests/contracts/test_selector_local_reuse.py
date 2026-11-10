"""Consumer-visible selection across repeated queries and independent invocations."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import sharing_frontier as frontier
from agent_memory.frontmatter import ContractError


def row(key, path, body='', framing=None, metadata=None, logical=None):
    return {'key': key, 'path': path, 'logical_key': logical or key,
            'body': body, 'effective_framing': framing or [],
            'metadata': metadata, 'witness_metadata': None}


def selection(searches, *, paths=(), ids=(), complete=True, uncertain=False):
    return {'searches': list(searches), 'paths': list(paths), 'ids': list(ids),
            'complete': complete, 'uncertain': uncertain}


def request(selections, dependencies=None, groups=None):
    return {'selections': selections, 'dependencies': dependencies or {},
            'consolidation_sources': groups or []}


class SelectorLocalReuseContract(unittest.TestCase):
    def test_exact_path_components_and_query_reuse_are_independent_of_selectors(self):
        identity = '12345678-1234-4234-8234-123456789abc'
        rows = {
            'a': row('a', 'topics/a.md'),
            'b': row('b', 'topics/b.md'),
            'c': row('c', 'topics/shared.md', 'introduction', metadata={'id': identity}),
            'd': row('d', 'topics/shared.md', 'strasse café prose'),
            'e': row('e', 'topics/text.md', 'Straße', [{'scope': 'CAFÉ framing'}]),
            'f': row('f', 'topics/other.md'),
        }
        selections = {
            'a': selection(['  STRASSE  café ', 'nothing one'], paths=['topics/shared.md']),
            'b': selection(['strasse café', 'nothing two'], ids=[identity], complete=False),
        }
        chosen, entries, pairs, pending = frontier.selected(rows, {'a', 'b', 'f'},
            request(selections, {'topics/b.md': ['topics/other.md']}, [['d', 'e']]))
        self.assertEqual(entries['a']['matched'], ['a', 'c', 'd', 'e'])
        self.assertEqual(entries['b']['matched'], ['b', 'c', 'd', 'e'])
        self.assertEqual(entries['f'], {'complete': False, 'matched': ['f']})
        self.assertEqual(list(chosen), ['a', 'b', 'c', 'd', 'e', 'f'])
        self.assertEqual(pairs, [['a', 'c'], ['a', 'd'], ['a', 'e'],
                                 ['b', 'c'], ['b', 'd'], ['b', 'e'], ['b', 'f'], ['d', 'e']])
        self.assertEqual(pending, ['b', 'f'])

    def test_changed_body_and_framing_are_observed_on_the_next_call(self):
        rows = {'a': row('a', 'topics/a.md'),
                'b': row('b', 'topics/b.md', 'alpha', [{'scope': 'violet'}])}
        req = request({'a': selection(['alpha violet', 'another wording'])})
        self.assertEqual(frontier.selected(rows, {'a'}, req)[1]['a']['matched'], ['a', 'b'])
        rows['b']['body'] = 'beta'
        rows['b']['effective_framing'] = [{'scope': 'amber'}]
        self.assertEqual(frontier.selected(rows, {'a'}, req)[1]['a']['matched'], ['a'])
        req['selections']['a']['searches'] = ['beta amber', 'another wording']
        self.assertEqual(frontier.selected(rows, {'a'}, req)[1]['a']['matched'], ['a', 'b'])

    def test_more_than_256_distinct_queries_still_match_without_truncation(self):
        rows = {f'{n:03}': row(f'{n:03}', f'topics/p{n:03}.md') for n in range(130)}
        rows['target'] = row('target', 'topics/target.md', 'marker129')
        selections = {f'{n:03}': selection([f'absent{n:03}', f'marker{n:03}'])
                      for n in range(130)}
        chosen, entries, pairs, pending = frontier.selected(rows, set(selections), request(selections))
        self.assertEqual(entries['129']['matched'], ['129', 'target'])
        self.assertEqual(entries['128']['matched'], ['128'])
        self.assertIn('target', chosen)
        self.assertEqual(pairs, [['129', 'target']])
        self.assertEqual(pending, [])

    def test_uncached_and_bounded_reuse_preserve_framed_matches_and_edges(self):
        rows = {'a': row('a', 'topics/a.md'),
                'b': row('b', 'topics/b.md', 'Straße', [{'scope': 'CAFÉ framing'}]),
                'c': row('c', 'topics/c.md')}
        req = request({'a': selection(['strasse café', 'unrelated phrase'], complete=False),
                       'c': selection(['strasse café', 'other phrase'])})
        for rendered_limit, query_limit in ((0, 0), (1, 1), (1 << 30, 1 << 30)):
            with self.subTest(rendered_limit=rendered_limit, query_limit=query_limit):
                with patch.object(frontier, '_SELECTOR_RENDER_BYTES', rendered_limit), \
                     patch.object(frontier, '_SELECTOR_QUERY_BYTES', query_limit):
                    chosen, entries, pairs, pending = frontier.selected(rows, {'a', 'c'}, req)
                self.assertEqual(list(chosen), ['a', 'b', 'c'])
                self.assertEqual(entries['a']['matched'], ['a', 'b'])
                self.assertEqual(entries['c']['matched'], ['b', 'c'])
                self.assertEqual(pairs, [['a', 'b'], ['b', 'c']])
                self.assertEqual(pending, ['a'])

    def test_validation_still_precedes_matching_and_pending_is_preserved(self):
        rows = {'a': row('a', 'topics/a.md'), 'b': row('b', 'topics/b.md')}
        with self.assertRaises(ContractError):
            frontier.selected(rows, {'a'}, request({'a': selection(['Same  words', 'same words'])}))
        with self.assertRaises(ContractError):
            frontier.selected(rows, {'a'}, request({'a': selection(['one', 'two'], ids=['invalid'])}))
        with self.assertRaises(ContractError):
            frontier.selected(rows, {'a'}, request({'a': selection(['one', 'two'], paths=['../bad'])}))
        result = frontier.selected(rows, {'a', 'b'}, request({'a': selection(['one', 'two'], uncertain=True)}))
        self.assertTrue(result[1]['a']['uncertain'])
        self.assertEqual(result[3], ['b'])


if __name__ == '__main__':
    unittest.main()
