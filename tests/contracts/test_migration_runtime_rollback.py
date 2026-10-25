"""Actual partial runtime update cannot roll back over an unreadable migrated bank."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from identity_fixtures import bind
from migration_fixtures import save, call, recover, snapshot
from runtime_update_fixtures import ROOT, fixture, advance, call as update, bank, owned

TARGET = 'agent-memory/test-only-target'
FIXTURE = '''"""TEST ONLY: native source/target transition, never shipped in production."""
from . import bank_contract
from .developer_identity import digest, encoded


def source(files):
    value = convert(files)
    checked = bank_contract.validate_files(value)
    return {'project_id': checked['project_id'], 'preservation_sha256': digest(encoded(value))}


def convert(files):
    value = dict(files)
    value['MEMORY.md'] = value['MEMORY.md'].replace('agent-memory/v1', 'agent-memory/test-only-target', 1)
    return value
'''


def next_format(pack):
    # A fixture release really uses its native target contract. Merely listing
    # another readable schema without implementing a reader would be false proof.
    path = pack/'scripts/agent_memory/bank_contract.py'
    path.write_text(path.read_text().replace('agent-memory/v1', TARGET))
    path = pack/'scripts/agent_memory/repository_config.py'
    path.write_text(path.read_text().replace('agent-memory/v1', TARGET))
    path = pack/'scripts/agent_memory/updating.py'
    path.write_text(path.read_text().replace("schema == 'agent-memory/v1'", "schema == '"+TARGET+"'"))
    path = pack/'schemas/bank-router.json'; value = json.loads(path.read_text())
    value['properties']['schema']['const'] = TARGET; path.write_text(json.dumps(value))
    path = pack/'schemas/runtime-compatibility.json'; value = json.loads(path.read_text())
    value['readable_bank_schemas'] = [TARGET]; value['migrator_source_schemas'] = ['agent-memory/v1']
    path.write_text(json.dumps(value))
    path = pack/'schemas/migrations.json'
    path.write_text(json.dumps({'schema':'agent-memory/migration-catalog-v1', 'codecs':[
        {'schema':TARGET, 'validator':{'module':'scripts/agent_memory/migration_codecs.py','function':'native'}},
        {'schema':'agent-memory/v1','validator':{'module':'scripts/agent_memory/migration_fixture.py','function':'source'}}],
        'steps':[{'id':'test-only-native-transition','from':'agent-memory/v1','to':TARGET,
                  'converter':{'module':'scripts/agent_memory/migration_fixture.py','function':'convert'}}]}))
    (pack/'scripts/agent_memory/migration_fixture.py').write_text(FIXTURE)


class MigrationRuntimeRollbackContract(unittest.TestCase):
    def test_partial_runtime_update_requires_explicit_retained_backup_restoration_before_rollback(self):
        with World() as w:
            channel, pack, receipt = fixture(w)
            repo = w.repository(); memory = bank(w, repo); bind(w, repo)
            before_bank, before_runtime = snapshot(memory), owned(receipt)
            next_format(pack); advance(w, channel, pack)
            _, checked = update(w, receipt, '--check', '--repo', str(repo))
            self.assertTrue(checked['compatibility']['banks'][0]['explicit_migration_required'])
            self.assertFalse(checked['compatibility']['implicit_migration_performed'])
            update(w, receipt, '--repo', str(repo), expected=86,
                   env={'AGENT_MEMORY_FAULT_STAGE':'update-after-activate-0','AGENT_MEMORY_FAULT_MODE':'exit'})
            current_pack = Path(receipt['source']['pack'])
            helper = current_pack/'scripts/migrate_bank.py'
            def migration(*args):
                result = w.run([sys.executable, '-I', helper, *args], check=False, timeout=120)
                value = json.loads(result.stdout); self.assertEqual(result.returncode, 0, value); return value
            p = migration('preview', '--repo', repo, '--to', TARGET)
            done = migration('apply', '--proposal', save(w, p))
            self.assertIn(TARGET, (memory/'MEMORY.md').read_text())
            actual_reader = w.run([sys.executable, '-I', current_pack/'scripts/bank_contract.py',
                                   'validate', '--repo', repo], check=False)
            self.assertEqual(actual_reader.returncode, 0, actual_reader.stdout+actual_reader.stderr)
            refused_runtime, refused_bank = owned(receipt), snapshot(memory)
            recovery = w.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'recover'], check=False)
            denied = json.loads(recovery.stdout); self.assertEqual(recovery.returncode, 1, denied)
            self.assertIn('cannot directly read', denied['error'])
            self.assertEqual(owned(receipt), refused_runtime); self.assertEqual(snapshot(memory), refused_bank)
            # The retained package operates while global installation is partial;
            # no successful global recover/install is a restoration prerequisite.
            retained = Path(done['retained_helper']); retained_before = snapshot(retained.parents[1])
            def outside(*args):
                result = w.run([sys.executable, '-I', retained, *args], check=False, timeout=120)
                value = json.loads(result.stdout); self.assertEqual(result.returncode, 0, value); return value
            p = outside('restore', '--repo', repo, '--operation', done['receipt']['operation_id'])
            restored = outside('restore', '--repo', repo, '--proposal', save(w, p, 'restore.json'))
            self.assertTrue(restored['verified']); self.assertEqual(snapshot(memory), before_bank)
            self.assertFalse(restored['receipt']['outcome_directly_readable'])
            self.assertEqual(snapshot(retained.parents[1]), retained_before)
            result = w.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'recover'], check=False)
            value = json.loads(result.stdout); self.assertEqual(result.returncode, 0, value)
            self.assertEqual(owned(receipt), before_runtime); self.assertEqual(snapshot(memory), before_bank)
            # Exercise the restored runtime's actual native reader, separately
            # from the newer helper's mechanical restoration receipt.
            result = w.run([Path(receipt['bin_dir'])/'am','--json','bank','validate','--repo',repo], check=False)
            self.assertEqual(result.returncode, 0, result.stdout+result.stderr)


if __name__ == '__main__':
    unittest.main()
