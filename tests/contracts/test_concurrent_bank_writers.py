"""Real competing public writers; same-bank races are defensive checks only."""
import hashlib
import json
from pathlib import Path
import socket
import sys
import time
import unittest
import uuid

from world import World
from contracts import test_bank_writes as single
from contracts import test_bank_transactions as transactions
from publication_fixtures import PublicationFixture

SCRIPTS = Path(__file__).resolve().parents[2] / 'scripts'


class ConcurrentBankWriters(unittest.TestCase):
    def collect(self, child, expected=0):
        stdout, stderr = child.communicate(timeout=20)
        self.assertEqual(child.returncode, expected, (stdout, stderr))
        self.assertFalse(stderr, stderr)
        return json.loads(stdout)

    def wait_ready(self, child, ready):
        deadline = time.monotonic() + 5
        while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
            time.sleep(.005)
        self.assertTrue(ready.exists(), ('rendezvous not reached', child.poll()))

    def pause(self, world, helper, proposal, stage):
        rendezvous = world.inside('rendezvous-' + uuid.uuid4().hex)
        keys = {'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': 'pause',
                'AGENT_MEMORY_FAULT_DIR': str(rendezvous)}
        self.assertFalse(set(keys) & set(world.env))
        world.env.update(keys)
        try:
            child = world.spawn([sys.executable, '-I', helper, 'apply', '--proposal', proposal])
        finally:
            for key in keys: world.env.pop(key)
        leaf = stage.replace('/', '%2F')
        self.wait_ready(child, rendezvous/(leaf + '.ready'))
        return child, rendezvous/(leaf + '.release')

    def git_state(self, world, repo):
        index = Path(world.git('rev-parse', '--path-format=absolute', '--git-path', 'index', cwd=repo).stdout.strip())
        return ((repo/'code.py').read_bytes(), index.read_bytes(),
                world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                world.git('symbolic-ref', 'HEAD', cwd=repo).stdout)

    def fixture(self, world):
        helper = single.BankWritesContract()
        repo = world.repository(); bank = helper.bank(world, repo)
        (repo/'code.py').write_text('VALUE = 1\n')
        world.git('add', '.', cwd=repo); world.git('commit', '-m', 'Canonical defensive race fixture', cwd=repo)
        (repo/'code.py').write_text('VALUE = 2  # staged unfinished feature\n')
        world.git('add', 'code.py', cwd=repo)
        return helper, repo, bank

    def event(self, name, **value):
        print(json.dumps({'case': name, **value}, sort_keys=True), file=sys.stderr, flush=True)

    def test_four_contenders_refuse_live_owner_and_stale_retries_then_preserve_every_rebased_fact(self):
        with World() as w:
            h, repo, bank = self.fixture(w); git_before = self.git_state(w, repo)
            target = bank/'activeContext.md'; original = target.read_text()
            facts = ['Race fact ' + str(i) for i in range(5)]
            proposals = [h.preview(w, bank, original + '\n' + fact + '\n') for fact in facts]
            paths = [h.saved(w, p, 'actor-' + str(i) + '.json') for i,p in enumerate(proposals)]
            winner, release = self.pause(w, single.HELPER, paths[0], 'locked-before-inventory')
            lock = bank/'.agent-memory.lock'; owner_bytes = lock.read_bytes(); owner = json.loads(owner_bytes)
            self.assertEqual(owner['pid'], winner.pid); self.assertEqual(owner['host'], socket.gethostname())
            others = [w.spawn([sys.executable, '-I', single.HELPER, 'apply', '--proposal', path]) for path in paths[1:]]
            self.assertEqual(len({winner.pid, *(child.pid for child in others)}), 5)
            for child in others:
                self.assertFalse(self.collect(child, 1)['verified'])
                self.assertEqual(lock.read_bytes(), owner_bytes); self.assertEqual(target.read_text(), original)
            release.touch(); receipt = self.collect(winner); self.assertTrue(receipt['verified'])
            self.assertEqual(target.read_text(), proposals[0]['proposal']['content']); self.assertFalse(lock.exists())
            for i in range(1,5):
                # Repeating the stale request is bounded and cannot overwrite the
                # winner. A fresh observed proposal explicitly folds its fact in.
                before = target.read_bytes(); r, refused = h.command(w, 'apply', '--proposal', paths[i])
                self.assertEqual(r.returncode, 1, refused); self.assertEqual(target.read_bytes(), before)
                fresh = h.preview(w, bank, target.read_text() + '\n' + facts[i] + '\n')
                r, written = h.apply(w, fresh); self.assertEqual(r.returncode, 0, written); self.assertTrue(written['verified'])
            text = target.read_text(); self.assertTrue(text.startswith(original))
            for fact in facts: self.assertEqual(text.count(fact), 1)
            self.assertEqual(len(h.receipts(w)), 5); self.assertEqual(self.git_state(w, repo), git_before)
            self.event('same-bank-contention', actual_processes=5, busy_refusals=4, stale_refusals=4,
                       explicit_fresh_writes=4, final_distinct_facts=5, silent_overwrites=0)

    def test_concurrent_observer_sees_complete_old_or_new_bytes_through_atomic_replacement(self):
        with World() as w:
            h, repo, bank = self.fixture(w); before = self.git_state(w, repo); target = bank/'activeContext.md'
            old = target.read_bytes(); new = old.decode() + '\n' + ('Retain a complete observed fact.\n' * 1000)
            proposal = h.preview(w, bank, new)
            writer, release = self.pause(w, single.HELPER, h.saved(w, proposal), 'before-recheck')
            ready, stop = w.inside('reader.ready'), w.inside('reader.stop')
            code = '''import hashlib,json,sys,time
from pathlib import Path
target,ready,stop=map(Path,sys.argv[1:]); seen=set();reads=0;deadline=time.monotonic()+20
def read():
 global reads
 raw=target.read_bytes()
 if len(raw)>65536:raise RuntimeError('unexpected unbounded target')
 seen.add(hashlib.sha256(raw).hexdigest());reads+=1
read();ready.touch()
while not stop.exists():
 if time.monotonic()>deadline:raise RuntimeError('observer deadline exceeded')
 read();time.sleep(.0005)
read();print(json.dumps({'reads':reads,'hashes':sorted(seen)}))
'''
            reader = w.spawn([sys.executable, '-I', '-c', code, target, ready, stop])
            self.wait_ready(reader, ready); release.touch(); result = self.collect(writer)
            self.assertTrue(result['verified']); stop.touch(); observation = self.collect(reader)
            self.assertGreater(observation['reads'], 2)
            expected = {hashlib.sha256(raw).hexdigest() for raw in (old, new.encode())}
            self.assertEqual(set(observation['hashes']), expected)
            self.assertEqual(target.read_text(), new); self.assertEqual(self.git_state(w, repo), before)
            self.event('atomic-observer', actual_processes=2, reads=observation['reads'], complete_hashes=2, partial_hashes=0)

    def test_actual_external_editor_invalidates_the_observed_target_without_losing_its_edit(self):
        with World() as w:
            h, repo, bank = self.fixture(w); before = self.git_state(w, repo)
            proposal = h.preview(w, bank); writer, release = self.pause(w, single.HELPER, h.saved(w, proposal), 'before-recheck')
            external = '# Focus\n\nActual external editor changed the target while its writer was paused.\n'
            editor = w.spawn([sys.executable, '-I', '-c',
                'from pathlib import Path;import json,sys;Path(sys.argv[1]).write_text(sys.argv[2]);print(json.dumps({"edited":True}))',
                bank/'activeContext.md', external])
            self.assertTrue(self.collect(editor)['edited']); release.touch(); refused = self.collect(writer, 1)
            self.assertFalse(refused['verified']); self.assertEqual((bank/'activeContext.md').read_text(), external)
            self.assertFalse((bank/'.agent-memory.lock').exists()); self.assertEqual(h.receipts(w), [])
            self.assertEqual(self.git_state(w, repo), before)
            self.event('external-editor', actual_processes=2, external_edit_preserved=True, stale_write_refused=True)

    def test_killed_transaction_owner_cannot_be_stolen_and_public_recovery_preserves_the_complete_bank(self):
        with World() as w:
            tx = transactions.BankTransactionsContract(); repo, bank, before, after, prepared = tx.fixture(w)
            h = single.BankWritesContract(); competing = h.preview(w, bank)
            owner, _release = self.pause(w, transactions.HELPER, tx.save(w, prepared, 'tx.json'),
                                          'transaction-after-write:activeContext.md')
            lock_before = (bank/'.agent-memory.lock').read_bytes()
            rival = w.spawn([sys.executable, '-I', single.HELPER, 'apply', '--proposal', h.saved(w, competing, 'rival.json')])
            self.assertFalse(self.collect(rival, 1)['verified']); self.assertEqual((bank/'.agent-memory.lock').read_bytes(), lock_before)
            r, denied = h.command(w, 'recover', '--repo', repo)
            self.assertEqual(r.returncode, 1, denied); self.assertEqual((bank/'.agent-memory.lock').read_bytes(), lock_before)
            owner.kill(); owner.communicate(timeout=20); self.assertNotEqual(owner.returncode, 0)
            self.assertTrue((bank/transactions.MARKER).exists()); tx.unlock(w, repo)
            self.assertEqual(h.command(w, 'inspect', '--repo', repo)[0].returncode, 1)
            recovery = tx.recovery(w, repo, 'finish'); r, finished = tx.recover_apply(w, repo, recovery)
            self.assertEqual(r.returncode, 0, finished); self.assertTrue(finished['verified'])
            self.assertEqual(tx.canonical(bank), after); self.assertFalse((bank/transactions.MARKER).exists())
            self.assertFalse((bank/'.agent-memory.lock').exists())
            r, stale = h.apply(w, competing); self.assertEqual(r.returncode, 1, stale); self.assertEqual(tx.canonical(bank), after)
            self.event('transaction-owner-recovery', actual_processes=2, live_recovery_refused=True,
                       owner_killed_and_reaped=True, explicit_finish_verified=True, stale_rival_refused=True)

    def test_distinct_worktrees_hold_independent_locks_and_sharing_manifests_contain_only_canonical_paths(self):
        with World() as w:
            h, repo, bank = self.fixture(w)
            other = w.inside('independent worktree'); w.git('worktree', 'add', '-b', 'other-session', other, cwd=repo)
            before = self.git_state(w, repo)
            a = h.preview(w, bank, '# Focus\n\nFirst worktree fact.\n')
            b = h.preview(w, other/'memory-bank', '# Focus\n\nIndependent second worktree fact.\n')
            first, release_a = self.pause(w, single.HELPER, h.saved(w, a, 'a.json'), 'locked-before-inventory')
            second, release_b = self.pause(w, single.HELPER, h.saved(w, b, 'b.json'), 'locked-before-inventory')
            first_lock, second_lock = bank/'.agent-memory.lock', other/'memory-bank/.agent-memory.lock'
            self.assertEqual(json.loads(first_lock.read_bytes())['pid'], first.pid)
            self.assertEqual(json.loads(second_lock.read_bytes())['pid'], second.pid)
            for root in (repo,other):
                r = w.run([sys.executable, '-I', SCRIPTS/'sharing_boundary.py', '--repo', root], check=False)
                self.assertEqual(r.returncode, 1, r.stdout)
            release_a.touch(); release_b.touch(); receipts = [self.collect(child) for child in (first,second)]
            self.assertNotEqual(receipts[0]['receipt_path'], receipts[1]['receipt_path'])
            self.assertEqual(receipts[0]['project_id'], receipts[1]['project_id'])
            self.assertFalse(first_lock.exists()); self.assertFalse(second_lock.exists())
            for root in (repo,other):
                r = w.run([sys.executable, '-I', SCRIPTS/'sharing_boundary.py', '--repo', root])
                boundary = json.loads(r.stdout)
                self.assertEqual(set(boundary['eligible_memory_paths']),
                                 {'memory-bank/MEMORY.md','memory-bank/activeContext.md','memory-bank/projectbrief.md'})
                self.assertEqual(boundary['excluded_tracked_artifact_count'], 0)
                self.assertFalse(boundary['publication_verified']); self.assertFalse(boundary['remote_freshness_verified'])
                tracked = w.git('ls-files', cwd=root).stdout.splitlines()
                self.assertFalse(any('.agent-memory' in name for name in tracked))
            self.assertEqual(self.git_state(w, repo), before)
            self.event('independent-worktrees', actual_processes=2, simultaneous_owned_locks=2,
                       separate_receipts=True, tracked_runtime_locks=0, shared_freshness_claimed=False)

    def test_publication_refuses_a_live_bank_lock_then_lands_without_runtime_state(self):
        with World() as w:
            pub = PublicationFixture(); repo, bank, _base = pub.bootstrap(w); pub.github(w, repo, bank)
            initial = pub.prepared(w, repo)['request']
            h = single.BankWritesContract()
            proposal = h.preview(w, bank, (bank/'projectbrief.md').read_text() + '\nRetain this actual defensive race result.\n',
                                 path='projectbrief.md')
            writer, release = self.pause(w, single.HELPER, h.saved(w, proposal), 'locked-before-inventory')
            lock_bytes = (bank/'.agent-memory.lock').read_bytes()
            remote_before = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            r, refused = pub.call(w, 'publish_memory.py', 'land', '--repo', repo,
                                  '--input', pub.save(w, initial, 'initial-publication.json'))
            self.assertNotEqual(r.returncode, 0, refused)
            self.assertEqual((bank/'.agent-memory.lock').read_bytes(), lock_bytes)
            self.assertEqual(w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout, remote_before)
            release.touch(); self.assertTrue(self.collect(writer)['verified'])
            before = pub.state(w, repo, bank); prepared = pub.prepared(w, repo)['request']
            landed = pub.land(w, repo, prepared); self.assertTrue(landed['publication_verified'])
            self.assertEqual(pub.state(w, repo, bank), before)
            tree = set(w.git('ls-tree', '-r', '--name-only', prepared['candidate'], '--', 'memory-bank', cwd=repo).stdout.splitlines())
            self.assertEqual(tree, {'memory-bank/' + p.relative_to(bank).as_posix()
                                   for p in bank.rglob('*.md') if not p.name.startswith('.agent-memory')})
            self.assertFalse(any('.agent-memory' in path for path in tree))
            self.assertEqual(w.git('show', prepared['candidate']+':code.py', cwd=repo).stdout, 'VALUE = 1\n')
            self.event('lock-publication-boundary', live_lock_publication_refused=True,
                       actual_isolated_remote_commit=prepared['candidate'], committed_runtime_paths=0,
                       feature_files_index_and_history_preserved=True)


if __name__ == '__main__':
    unittest.main()
