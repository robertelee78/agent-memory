"""Public legacy uninstall recognizes verified Git metadata-only drift."""
from pathlib import Path
import unittest
from world import World
from runtime_update_fixtures import fixture
from contracts.test_runtime_uninstall import uninstall

class LegacyUninstallMetadataContract(unittest.TestCase):
    def test_metadata_drift_removes_verified_owned_source(self):
        with World() as w:
            _,_,receipt=fixture(w)
            source=Path(receipt['source']['path'])
            w.git('config','am-repro.example','changed-git-metadata',cwd=source)
            preview=uninstall(w,receipt,'--check')
            self.assertFalse(preview['preserved'])
            result=uninstall(w,receipt)
            self.assertTrue(result['verified'])
            self.assertFalse(source.exists())

    def test_changed_payload_is_preserved(self):
        with World() as w:
            _,_,receipt=fixture(w)
            source=Path(receipt['source']['path'])
            (source/'skills/agent-memory/SKILL.md').write_text('user-edited payload\n')
            result=uninstall(w,receipt,source=True)
            self.assertTrue(result['verified'])
            self.assertTrue(source.exists())
            self.assertTrue(any(x['kind']=='source' for x in result['preserved']))

    def test_untracked_payload_is_preserved(self):
        with World() as w:
            _,_,receipt=fixture(w)
            source=Path(receipt['source']['path'])
            (source/'user-file.txt').write_text('do not remove\n')
            result=uninstall(w,receipt,source=True)
            self.assertTrue(result['verified'])
            self.assertEqual((source/'user-file.txt').read_text(),'do not remove\n')
            self.assertTrue(any(x['kind']=='source' for x in result['preserved']))
