"""Approved shared-learning obligations stay bound to mandatory executable cases."""
import ast
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


class MultiMachineMatrix(unittest.TestCase):
    def test_every_approved_shared_learning_obligation_has_registered_executable_cases(self):
        matrix = json.loads((ROOT/'tests/shared-learning-matrix.json').read_text())
        expected = {
            'actors-and-repository-scope',
            'automatic-durable-capture',
            'memory-only-publication',
            'next-boundary-recall',
            'simultaneous-publication',
            'compatible-same-file-edits',
            'deduplicate-before-write',
            'deduplicate-before-landing',
            'contradictions-and-provenance',
            'dependent-work-only-holds',
            'conditional-learning-and-stale-code',
            'evidence-pruning-and-older-branches',
            'initialization-code-docs-and-awa',
            'offline-retry',
            'lost-success-response',
            'later-ordinary-code-merge',
            'terminal-deletion-without-resurrection',
        }
        self.assertEqual(matrix['schema'],'agent-memory/shared-learning-test-matrix-v1')
        self.assertEqual({row['requirement'] for row in matrix['requirements']},expected)
        self.assertEqual(len(matrix['requirements']),len(expected))
        registry = {row['name']:row for row in json.loads((ROOT/'tests/contracts.json').read_text())['suites']}
        methods = {}
        for row in matrix['requirements']:
            self.assertTrue(row['cases'],row['requirement'])
            for case in row['cases']:
                entry = registry[case['suite']]
                self.assertEqual(entry['scope'],'v0.0.1');self.assertTrue(entry['path'])
                if entry['path'] not in methods:
                    tree = ast.parse((ROOT/entry['path']).read_text())
                    methods[entry['path']] = {cls.name+'.'+fn.name for cls in tree.body if isinstance(cls,ast.ClassDef)
                                             for fn in cls.body if isinstance(fn,ast.FunctionDef) and fn.name.startswith('test_')}
                self.assertIn(case['test'],methods[entry['path']],case)


if __name__=='__main__':unittest.main()
