"""Public interruption and erasure boundaries for governed memory operations."""
import hashlib
import json
from pathlib import Path
import sys
import time
import unittest
import uuid

from world import World
from contracts import test_governance as fixtures
from contracts import test_comparison as records
from contracts import test_sharing_boundary as sharing


class GovernanceRecoveryContract(unittest.TestCase):
    call = fixtures.GovernanceContract.call
    good = fixtures.GovernanceContract.good
    save = fixtures.GovernanceContract.save
    fixture = fixtures.GovernanceContract.fixture
    add = fixtures.GovernanceContract.add
    state = fixtures.GovernanceContract.state
    request = fixtures.GovernanceContract.request
    preview = fixtures.GovernanceContract.preview
    apply = fixtures.GovernanceContract.apply
    replacement_capture = fixtures.GovernanceContract.replacement_capture

    def prepared(self, w, repo, request):
        r, value = self.preview(w, repo, request)
        self.assertEqual(r.returncode, 0, value);self.assertIsNotNone(value['proposal'])
        return value['proposal']

    def unlock(self, w, repo):
        value = self.good(w, 'bank_writes.py', 'recover', '--repo', repo)
        self.good(w, 'bank_writes.py', 'recover', '--repo', repo, '--proposal', self.save(w, value, 'unlock.json'))

    def recover(self, w, repo, mode):
        value = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', mode)
        return self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--proposal', self.save(w, value, 'recover.json'))

    def test_correction_crash_requires_owned_lock_recovery_and_rollback_before_fresh_comparison(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'wrong', body='# Retry policy\n\nThe disproved old claim allowed five attempts.\n')
            value = self.replacement_capture(w, repo, bank, old, path);proposal = self.prepared(w, repo, value)
            before = self.state(w, repo, bank)
            w.env.update(AGENT_MEMORY_FAULT_STAGE='transaction-after-write:' + path, AGENT_MEMORY_FAULT_MODE='exit')
            r = w.run([sys.executable, '-I', self.pack / 'scripts/govern_memory.py', 'apply', '--proposal', self.save(w, proposal)], check=False)
            w.env.pop('AGENT_MEMORY_FAULT_STAGE');w.env.pop('AGENT_MEMORY_FAULT_MODE');self.assertEqual(r.returncode, 86, r.stdout + r.stderr)
            self.unlock(w, repo)
            self.assertEqual(self.call(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'finish')[0].returncode, 1)
            done = self.recover(w, repo, 'rollback');self.assertEqual(done['outcome'], 'rolled-back')
            self.assertTrue(done['verified']);self.assertEqual(before, self.state(w, repo, bank))
            fresh = self.replacement_capture(w, repo, bank, old, path);proposal = self.prepared(w, repo, fresh)
            r, done = self.apply(w, proposal);self.assertEqual(r.returncode, 0, done);self.assertTrue(done['governance_verified'])
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_changed_supporting_source_at_readback_cannot_commit_pruning_and_rollback_preserves_it(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'obsolete');request = self.request(bank, old, path)
            request['authority'].update(source_class='verified-repository', evidence=['evidence.md'],
                repository_evidence={'path': 'evidence.md', 'sha256': hashlib.sha256((repo / 'evidence.md').read_bytes()).hexdigest()})
            proposal = self.prepared(w, repo, request);before = self.state(w, repo, bank);directory = w.inside('rendezvous')
            w.env.update(AGENT_MEMORY_FAULT_STAGE='transaction-before-receipt', AGENT_MEMORY_FAULT_MODE='pause', AGENT_MEMORY_FAULT_DIR=str(directory))
            child = w.spawn([sys.executable, '-I', self.pack / 'scripts/govern_memory.py', 'apply', '--proposal', self.save(w, proposal)])
            ready = directory / 'transaction-before-receipt.ready';deadline = time.monotonic() + 15
            while not ready.exists() and child.poll() is None and time.monotonic() < deadline:time.sleep(.01)
            self.assertTrue(ready.exists())
            changed = '# Source changed during readback; retain this external edit.\n';(repo / 'evidence.md').write_text(changed)
            (directory / 'transaction-before-receipt.release').touch()
            stdout, stderr = child.communicate(timeout=20);self.assertEqual(child.returncode, 1, stdout + stderr)
            self.assertFalse(json.loads(stdout).get('governance_verified', False));self.assertTrue((bank / '.agent-memory-transaction.md').exists())
            for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):w.env.pop(key)
            done = self.recover(w, repo, 'rollback');self.assertEqual(done['outcome'], 'rolled-back')
            self.assertEqual(before, self.state(w, repo, bank));self.assertEqual((repo / 'evidence.md').read_text(), changed)

    def test_nested_completed_receipt_refuses_corruption_then_cleans_up_without_requiring_new_evidence(self):
        with World() as w:
            helper = sharing.SharingBoundaryContract();repo, bank = helper.fixture(w, 'docs/shared memory')
            self.pack = helper.pack
            status = self.good(w, 'scope_binding.py', 'status', '--repo', repo);self.pid = status['project_id'];self.actor = status['developer_id']
            (repo / 'evidence.md').write_text('# Explicit current fixture observation\n')
            old, path = self.add(bank, 'obsolete');request = self.request(bank, old, path)
            request['authority'].update(source_class='verified-repository', evidence=['evidence.md'],
                repository_evidence={'path': 'evidence.md', 'sha256': hashlib.sha256((repo / 'evidence.md').read_bytes()).hexdigest()})
            proposal = self.prepared(w, repo, request);before = self.state(w, repo, bank)
            w.env.update(AGENT_MEMORY_FAULT_STAGE='transaction-after-receipt', AGENT_MEMORY_FAULT_MODE='exit')
            r = w.run([sys.executable, '-I', self.pack / 'scripts/govern_memory.py', 'apply', '--proposal', self.save(w, proposal)], check=False)
            w.env.pop('AGENT_MEMORY_FAULT_STAGE');w.env.pop('AGENT_MEMORY_FAULT_MODE');self.assertEqual(r.returncode, 86, r.stdout + r.stderr)
            self.unlock(w, repo)
            matches = [p for p in Path(w.env['AGENT_MEMORY_STATE_HOME']).rglob('*.json')
                if json.loads(p.read_text()).get('schema') == 'agent-memory/transaction-receipt-v1'
                and json.loads(p.read_text()).get('operation_id') == proposal['operation_id']]
            self.assertEqual(len(matches), 1);receipt_path = matches[0];original = receipt_path.read_bytes();receipt = json.loads(original)
            self.assertEqual(receipt['bank_path'], str(bank));self.assertTrue(all(p.startswith('docs/shared memory/') for p in receipt['changed_memory_paths']))
            recovery = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'finish')
            for field in ('project_id', 'changed_memory_paths', 'extra'):
                corrupted = dict(receipt)
                if field == 'project_id':corrupted[field] = str(uuid.uuid4())
                elif field == 'changed_memory_paths':corrupted[field] = [p.replace('docs/shared memory/', 'memory-bank/', 1) for p in receipt[field]]
                else:corrupted[field] = 'Unknown receipt extension must refuse.'
                receipt_path.write_text(json.dumps(corrupted));current = self.state(w, repo, bank)
                r, refused = self.call(w, 'bank_transactions.py', 'recover', '--repo', repo, '--proposal', self.save(w, recovery, 'recover.json'))
                self.assertEqual(r.returncode, 1, refused);self.assertEqual(current, self.state(w, repo, bank))
                self.assertTrue((bank / '.agent-memory-transaction.md').exists());receipt_path.write_bytes(original)
            (repo / 'evidence.md').write_text('# Later source, after the verified commit point\n')
            done = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--proposal', self.save(w, recovery, 'recover.json'))
            self.assertEqual(done['outcome'], 'finished');self.assertFalse(done['writes_performed'])
            (bank / path).write_text((bank / path).read_text() + '\nLater manual orientation must remain intact.\n')
            current = self.state(w, repo, bank);r, replay = self.apply(w, proposal)
            self.assertEqual(r.returncode, 0, replay);self.assertTrue(replay['already_applied']);self.assertFalse(replay['still_current'])
            self.assertEqual(current, self.state(w, repo, bank));self.assertFalse((repo / 'memory-bank').exists())
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_explicit_deletion_erases_corrected_versions_and_their_content_bearing_history(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'wrong', body='# Retry policy\n\nFive attempts was a disproved claim.\n')
            correction = self.replacement_capture(w, repo, bank, old, path);proposal = self.prepared(w, repo, correction)
            r, done = self.apply(w, proposal);self.assertEqual(r.returncode, 0, done)
            current, _ = records.unpack((bank / path).read_text());request = self.request(bank, current, path, 'delete')
            request['authority'].update(basis='explicit-deletion', reason_class='user-request');request['delete_ids'] = [old['id']]
            proposal = self.prepared(w, repo, request);before = self.state(w, repo, bank)
            r, done = self.apply(w, proposal);self.assertEqual(r.returncode, 0, done)
            files = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            self.assertEqual(set(files), {'MEMORY.md', 'projectbrief.md', 'history/tombstones/' + old['id'] + '.md'})
            self.assertNotIn('Five attempts', '\n'.join(files.values()));self.assertNotIn('Use at most three attempts', '\n'.join(files.values()))
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_deleting_last_nested_route_preserves_custom_index_context_before_removing_empty_maps(self):
        with World() as w:
            repo, bank = self.fixture(w);metadata, path = self.add(bank, 'last-child')
            child, parent = 'topics/nested/index.md', 'topics/index.md'
            def index(path, target):
                request = {'path': path, 'project-id': self.pid, 'updated': '2026-10-22T22:00:00Z',
                    'routes': [{'path': target, 'purpose': 'Routed fixture', 'scope': 'repository',
                        'lifecycle': 'active', 'read-when': 'When examining this fixture'}]}
                return self.good(w, 'bank_layout.py', 'render-index', '--input', self.save(w, request))['content']
            (bank / child).parent.mkdir(parents=True);(bank / child).write_text(index(child, path))
            parent_text = index(parent, child);context = 'Unrelated useful index orientation must survive.'
            (bank / parent).write_text(parent_text.replace('\n## Routes\n', '\n' + context + '\n\n## Routes\n'))
            router = bank / 'MEMORY.md'
            text = ''.join(line for line in router.read_text().splitlines(keepends=True) if not line.startswith('| [' + path + ']'))
            text = text.replace('\n## Omitted contexts', '| [' + parent + '](<' + parent + '>) | Nested group | repository | active | When needed |\n\n## Omitted contexts')
            router.write_text(text)
            request = self.request(bank, metadata, path, 'delete');request['authority'].update(basis='explicit-deletion', reason_class='user-request')
            request['delete_ids'] = [metadata['id']];before = self.state(w, repo, bank)
            r, refused = self.preview(w, repo, request)
            self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))
            # The caller preserves the unrelated prose through the existing
            # ordinary-file workflow, then repeats the exact governance review.
            (bank / 'projectbrief.md').write_text((bank / 'projectbrief.md').read_text() + '\n' + context + '\n')
            (bank / parent).write_text(parent_text)
            request = self.request(bank, metadata, path, 'delete');request['authority'].update(basis='explicit-deletion', reason_class='user-request')
            request['delete_ids'] = [metadata['id']]
            r, plan = self.preview(w, repo, request);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            self.assertFalse((bank / child).exists());self.assertFalse((bank / parent).exists())
            self.assertIn(context, (bank / 'projectbrief.md').read_text())
            self.good(w, 'bank_contract.py', 'validate', '--repo', repo)
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
