"""Quick refresh shares path protection and explains operation-specific failures."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from world import World
from refresh_fixtures import RefreshFixture

SOURCE = Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'
sys.path.insert(0, str(SOURCE))
from agent_memory import memory_refresh, trust_paths


class TransactionPathPolicy(unittest.TestCase):
    def layout(self, w):
        state = w.paths['state'] / 'agent-memory'
        operations = state / 'writes' / 'checkout' / 'operations'
        operations.mkdir(parents=True, mode=0o700)
        transactions = operations.parent / 'transactions'
        transactions.mkdir(mode=0o700)
        return state, operations, transactions

    def test_protected_intermediates_share_policy_without_chmod(self):
        with World() as w:
            state, operations, target = self.layout(w)
            for mode in (0o775, 0o777):
                with self.subTest(mode=oct(mode)):
                    for directory in (state, state/'writes', operations.parent):
                        directory.chmod(mode)
                    before = {p: stat.S_IMODE(p.stat().st_mode) for p in (state, state/'writes', operations.parent)}
                    self.assertTrue(trust_paths.inspect(target)['unix_path_policy_verified'])
                    self.assertEqual(memory_refresh.transaction_inventory(operations)[2], ('empty',))
                    self.assertEqual(before, {p: stat.S_IMODE(p.stat().st_mode) for p in before})

    def test_unprotected_writable_ancestor_rejected_by_both_walks(self):
        with World() as w:
            state, operations, target = self.layout(w)
            w.root.chmod(0o755);w.paths['state'].chmod(0o755);state.chmod(0o775)
            try:
                with self.assertRaises(trust_paths.TrustPathError):trust_paths.inspect(target)
                with self.assertRaises(memory_refresh.CurrentFailure) as observed:
                    memory_refresh.transaction_inventory(operations)
                self.assertEqual(observed.exception.code, 'transaction-path-untrusted')
            finally:
                w.root.chmod(0o700);w.paths['state'].chmod(0o700)

    def test_foreign_owner_rejected_below_private_barrier(self):
        foreign = SimpleNamespace(st_uid=os.geteuid()+10000,st_mode=stat.S_IFDIR|0o700,st_nlink=2)
        with self.assertRaises(trust_paths.TrustPathError):
            trust_paths.inspect_entry(Path('/owned/private/foreign'), foreign, True)

    def test_redirected_intermediate_rejected_without_following(self):
        with World() as w:
            state, operations, target = self.layout(w)
            moved=state/'actual';(state/'writes').rename(moved);(state/'writes').symlink_to(moved,target_is_directory=True)
            with self.assertRaises(trust_paths.TrustPathError):trust_paths.inspect(target)
            with self.assertRaises(memory_refresh.CurrentFailure) as observed:
                memory_refresh.transaction_inventory(operations)
            self.assertEqual(observed.exception.code,'transaction-path-untrusted')
            self.assertTrue((state/'writes').is_symlink())

    def test_barrier_chmod_or_rebinding_during_observation_is_refused(self):
        for change in ('chmod','rebind'):
            with self.subTest(change=change), World() as w:
                state, operations, target = self.layout(w)
                barrier=w.paths['state'];real=trust_paths.inspect_entry;done=False
                def mutate(path, metadata, protected, **options):
                    nonlocal done
                    result=real(path,metadata,protected,**options)
                    if Path(path)==state and not done:
                        done=True
                        if change=='chmod':barrier.chmod(0o755)
                        else:
                            barrier.rename(w.root/'retained-old-state');barrier.mkdir(mode=0o700)
                    return result
                with patch.object(trust_paths,'inspect_entry',side_effect=mutate):
                    with self.assertRaises(memory_refresh.CurrentFailure) as observed:
                        memory_refresh.transaction_inventory(operations)
                self.assertEqual(observed.exception.code,'current-source-changed')
                self.assertTrue(done)

    def test_orphan_recovery_is_specific_and_preserved(self):
        with World() as w:
            state, operations, target=self.layout(w);evidence=target/'retained.json';evidence.write_text('{}')
            with self.assertRaises(memory_refresh.CurrentFailure) as observed:
                memory_refresh.transaction_inventory(operations)
            self.assertEqual(observed.exception.code,'transaction-recovery-pending')
            self.assertEqual(evidence.read_text(),'{}')

    def test_unexpected_error_text_is_not_exposed(self):
        with patch.object(memory_refresh,'current',side_effect=ValueError('secret-source-body')):
            result=memory_refresh.current_result('/unread')
        self.assertEqual(result['failure_code'],'refresh-prerequisite-unverified')
        self.assertNotIn('secret-source-body',json.dumps(result))
        self.assertFalse(result['canonical_writes_performed'])
        self.assertFalse(result['remote_freshness_verified'])


class RefreshPathPolicyIntegration(RefreshFixture):
    def test_final_source_mutation_has_specific_api_and_cli_diagnostic(self):
        for route in ('api', 'cli'):
            with self.subTest(route=route), World() as w:
                repo,bank,peer,base=self.fixture(w)
                target=bank/'activeContext.md'
                changed=target.read_bytes()+b'\nConcurrent source change must remain intact.\n'
                before=self.state(w,repo,bank)
                expected_bank={**before[0], 'activeContext.md': changed}
                if route=='api':
                    real_fault=memory_refresh.writes.fault
                    def mutate(stage):
                        if stage=='refresh-current-before-final-recheck':target.write_bytes(changed)
                        else:real_fault(stage)
                    with patch.dict(os.environ,w.env,clear=True), patch.object(memory_refresh.writes,'fault',side_effect=mutate):
                        value=memory_refresh.current_result(repo)
                else:
                    rendezvous=w.inside('source-race');rendezvous.mkdir()
                    stage='refresh-current-before-final-recheck'
                    w.env.update(AGENT_MEMORY_FAULT_STAGE=stage, AGENT_MEMORY_FAULT_MODE='pause',
                                 AGENT_MEMORY_FAULT_DIR=str(rendezvous))
                    child=w.spawn([sys.executable,'-I','-B',self.pack/'scripts/installer.py',
                                   'refresh','current','--repo',repo],cwd=repo)
                    ready=rendezvous/(stage+'.ready');deadline=time.monotonic()+10
                    try:
                        while not ready.exists() and time.monotonic()<deadline:time.sleep(.01)
                        self.assertTrue(ready.exists(),'quick refresh did not reach final recheck')
                        target.write_bytes(changed)
                    finally:
                        (rendezvous/(stage+'.release')).touch()
                    stdout,stderr=child.communicate(timeout=30)
                    self.assertEqual(child.returncode,1,stderr)
                    value=json.loads(stdout)
                self.assertEqual(value['failure_component'],'current.final-recheck',value)
                self.assertEqual(value['failure_code'],'current-source-changed',value)
                self.assertEqual(value['sharing_state'],'pending-inspection')
                self.assertFalse(value['remote_freshness_verified'])
                self.assertFalse(value['canonical_writes_performed'])
                self.assertEqual(self.state(w,repo,bank),(expected_bank,*before[1:]))

    def test_unprotected_state_failure_remains_specific_without_changing_work(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            state=Path(w.env['AGENT_MEMORY_STATE_HOME'])
            before=self.state(w,repo,bank)
            w.root.chmod(0o755);w.paths['state'].chmod(0o755);state.chmod(0o775)
            try:
                response,value=self.call(w,'refresh_memory.py','current','--repo',repo)
                self.assertEqual(response.returncode,1,value)
                self.assertEqual(value['failure_code'],'transaction-path-untrusted')
                self.assertIn('state',value['failure_component'])
                self.assertFalse(value['remote_freshness_verified'])
                self.assertFalse(value['canonical_writes_performed'])
                self.assertEqual(self.state(w,repo,bank),before)
                response,doctor=self.call(w,'diagnose.py','doctor','--repo',repo)
                self.assertTrue(any(c['component']=='bank.refresh_prerequisites' for c in doctor['checks']), doctor)
                check=next(c for c in doctor['checks'] if c['component']=='bank.refresh_prerequisites')
                self.assertEqual(check['state'],'failure')
                self.assertIn('transaction-path-untrusted',check['detail'])
                self.assertEqual(doctor['refresh']['remote_check'],'not-exercised')
                self.assertEqual(self.state(w,repo,bank),before)
            finally:
                w.root.chmod(0o700);w.paths['state'].chmod(0o700)

    def test_actual_protected_775_chain_current_and_doctor_preserve_feature_work(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            state=Path(w.env['AGENT_MEMORY_STATE_HOME'])
            checkout=state/'writes'/hashlib.sha256(str(bank).encode()).hexdigest()
            for path in (state,checkout):path.chmod(0o775)
            before=self.state(w,repo,bank)
            response,value=self.call(w,'refresh_memory.py','current','--repo',repo)
            self.assertEqual(response.returncode,0,value)
            self.assertEqual(value['sharing_state'],'bank-synchronized')
            self.assertTrue(value['remote_freshness_verified'])
            self.assertEqual(self.state(w,repo,bank),before)
            self.assertEqual(stat.S_IMODE(state.stat().st_mode),0o775)
            self.assertEqual(stat.S_IMODE(checkout.stat().st_mode),0o775)
            doctor=self.good(w,'diagnose.py','doctor','--repo',repo)
            check=next(c for c in doctor['checks'] if c['component']=='bank.refresh_prerequisites')
            self.assertEqual(check['state'],'pass')
            self.assertEqual(doctor['refresh']['remote_check'],'not-exercised')
            self.assertFalse(doctor['refresh']['remote_freshness_verified'])
            evidence=checkout/'transactions'/'unknown'/'journal.json';evidence.parent.mkdir();evidence.write_text('{}')
            response,value=self.call(w,'refresh_memory.py','current','--repo',repo)
            self.assertEqual(response.returncode,1)
            self.assertEqual(value['failure_code'],'transaction-recovery-pending')
            self.assertIn('journal',value['repair'])
            self.assertEqual(self.state(w,repo,bank),before)
            response,doctor=self.call(w,'diagnose.py','doctor','--repo',repo)
            check=next(c for c in doctor['checks'] if c['component']=='bank.refresh_prerequisites')
            self.assertEqual(check['state'],'failure')
            self.assertIn('transaction-recovery-pending',check['detail'])
            self.assertTrue(evidence.exists())
