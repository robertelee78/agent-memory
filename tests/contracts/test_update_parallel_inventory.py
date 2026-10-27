"""Update inventory preserves every observation while overlapping reads."""
import pathlib
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'skills/agent-memory/scripts'))
from agent_memory import updating
from agent_memory.frontmatter import ContractError


class ParallelUpdateInventory(unittest.TestCase):
    def test_bounded_pool_keeps_all_roots_and_propagates_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            roots = [(pathlib.Path(tmp) / str(i)).resolve() for i in range(8)]
            for root in roots:
                root.mkdir()
            active = 0
            maximum = 0
            lock = threading.Lock()
            def settings(root, **kwargs):
                nonlocal active, maximum
                with lock:
                    active += 1
                    maximum = max(maximum, active)
                try:
                    time.sleep(0.025)
                    return {'bank_root': 'memory-bank', 'authority_branch': None}
                finally:
                    with lock:
                        active -= 1
            compatibility = {'readable_bank_schemas': ['agent-memory/v1'], 'migrator_source_schemas': []}
            with patch.object(pathlib.Path, 'cwd', return_value=roots[0]), \
                 patch.object(updating.project_identity, 'observe', return_value=({'checkouts': {}}, None)), \
                 patch.object(updating.repository_config, 'settings', side_effect=settings):
                result = updating.banks(roots, compatibility)
                self.assertTrue(set(map(str, roots)).issubset(result['observed_repositories']))
                self.assertEqual(result['banks'], [])
                self.assertGreater(maximum, 1)
                self.assertLessEqual(maximum, 4)
                def fail_on_one(root, **kwargs):
                    if root == roots[3]:
                        raise ContractError('incomplete inventory')
                    return settings(root, **kwargs)
                with patch.object(updating.repository_config, 'settings', side_effect=fail_on_one):
                    with self.assertRaisesRegex(ContractError, 'incomplete inventory'):
                        updating.banks(roots, compatibility)


if __name__ == '__main__':
    unittest.main()
