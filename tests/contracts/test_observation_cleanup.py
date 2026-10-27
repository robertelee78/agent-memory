"""Authority failures remain attributable when private-ref cleanup also fails."""
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2]/'skills/agent-memory/scripts'))
from agent_memory import sharing_recovery as r
from agent_memory.frontmatter import ContractError

class ObservationCleanupContract(unittest.TestCase):
    def test_fetch_failure_survives_failed_cleanup(self):
        calls=[]
        def command(root, *args, **kwargs):
            calls.append(args[0])
            raise ContractError('sensitive raw details')
        with patch.object(r.writes,'bank_path',return_value=(Path('/fixture'),None)), \
             patch.object(r.scope_binding,'require',return_value={'project_id':'p'}), \
             patch.object(r.sharing_boundary,'inspect',return_value={'blockers':[], 'authority_remote':'origin','authority_branch':'main'}), \
             patch.object(r,'git',return_value='head'), patch.object(r,'server',return_value='head'), \
             patch.object(r,'command',side_effect=command):
            with self.assertRaises(r.FetchFailure) as observed:r.observe('/fixture')
        self.assertEqual(calls,['fetch','update-ref'])
        self.assertEqual(observed.exception.code,'authority-fetch-unavailable')
        self.assertIn('cleanup failed',str(observed.exception))
        self.assertIn('repository permissions',observed.exception.repair)
        self.assertNotIn('sensitive raw details',str(observed.exception))
