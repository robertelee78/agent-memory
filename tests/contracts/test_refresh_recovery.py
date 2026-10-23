"""Actual interrupted import and completed-write/lost-response recovery."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from refresh_fixtures import RefreshFixture


class RefreshRecoveryContract(RefreshFixture):
    def unlock(self, w, repo):
        plan = self.good(w, 'bank_writes.py', 'recover', '--repo', repo)
        self.good(w, 'bank_writes.py', 'recover', '--repo', repo,
                  '--proposal', self.save(w, plan, 'unlock.json'))

    def recover(self, w, repo, mode):
        plan = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', mode)
        return self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--proposal', self.save(w, plan, 'recovery.json'))

    def crash(self, w, proposal, stage):
        outcome = w.run([sys.executable, '-I', self.pack/'scripts/refresh_memory.py', 'apply',
                         '--input', self.save(w, proposal)], check=False, timeout=120,
                        env={'AGENT_MEMORY_FAULT_STAGE':stage,'AGENT_MEMORY_FAULT_MODE':'exit'})
        self.assertEqual(outcome.returncode, 86)

    def test_lost_completed_response_recovers_without_duplicates_and_remains_honest_while_offline(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            record, path = self.imported(w, peer, 'import', '# Imported\n\nPreserve the actual completed observation.\n')
            proposal = self.prepared_refresh(w, repo); before_code = self.state(w, repo, bank)[1:]
            self.crash(w, proposal, 'transaction-after-receipt')
            self.assertTrue((bank/'.agent-memory-transaction.md').exists()); self.assertTrue((bank/path).exists())
            self.unlock(w, repo); finished = self.recover(w, repo, 'finish')
            self.assertEqual(finished['outcome'], 'finished'); self.assertFalse((bank/'.agent-memory-transaction.md').exists())
            remote = Path(w.git('remote','get-url','origin',cwd=repo).stdout.strip()); away = remote.with_name('offline.git')
            remote.rename(away)
            pending = self.apply_refresh(w, proposal)
            self.assertEqual(pending['sharing_state'], 'pending-stale'); self.assertTrue(pending['refresh_applied'])
            self.assertFalse(pending['remote_freshness_verified']); self.assertFalse(pending['relevant_imports_read'])
            self.assertFalse(pending['canonical_writes_performed']); self.assertTrue(pending['local_memory_available'])
            away.rename(remote)
            online = self.apply_refresh(w, proposal)
            self.assertTrue(online['remote_freshness_verified']); self.assertTrue(online['receipt']['already_applied'])
            self.assertFalse(online['canonical_writes_performed']); self.assertEqual(before_code, self.state(w, repo, bank)[1:])
            self.assertEqual(len(list((bank/'topics').glob('*.md'))), 1)

    def test_unfinished_source_bound_import_requires_rollback_then_fresh_review_not_old_finish(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            record, path = self.imported(w, peer, 'import', '# Imported\n\nRevalidate after interrupted writes.\n')
            proposal = self.prepared_refresh(w, repo); before = self.state(w, repo, bank)
            self.crash(w, proposal, 'transaction-after-marker')
            self.unlock(w, repo)
            p, failed = self.call(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'finish')
            self.assertEqual(p.returncode, 1); self.assertTrue((bank/'.agent-memory-transaction.md').exists())
            rolled = self.recover(w, repo, 'rollback'); self.assertEqual(rolled['outcome'], 'rolled-back')
            self.assertEqual(before, self.state(w, repo, bank))
            old = self.apply_refresh(w, proposal)
            self.assertFalse(old['refresh_applied']); self.assertFalse(old['canonical_writes_performed'])
            fresh = self.prepared_refresh(w, repo); self.assertNotEqual(fresh['operation_id'], proposal['operation_id'])
            done = self.apply_refresh(w, fresh)
            self.assertTrue(done['remote_freshness_verified']); self.assertTrue((bank/path).exists())
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
