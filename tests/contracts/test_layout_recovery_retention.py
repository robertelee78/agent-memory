"""Exact private-file retention primitives, not fabricated release acceptance."""
import json
import os
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]


class LayoutRetentionContract(unittest.TestCase):
    def fixture(self, w):
        self.pack = w.inside('portable pack')
        shutil.copytree(ROOT/'skills/agent-memory', self.pack, ignore=shutil.ignore_patterns('__pycache__'))
        directory = w.inside('private recovery operation'); directory.mkdir(mode=0o700)
        (directory/'originals').mkdir(mode=0o700)
        source = directory/'originals'/(str(uuid.uuid4())+'.bin')
        source.write_bytes(b'Original exact bytes and conditions.\n'); source.chmod(0o600)
        receipt = directory/'receipt.json'
        receipt.write_text('{"outcome":"layout-restored"}\n'); receipt.chmod(0o600)
        return directory, source, receipt

    def module(self, w, code, *args, check=True):
        program = ('import sys,json;from pathlib import Path;'
                   'sys.path.insert(0,sys.argv.pop(1));'
                   'from agent_memory import layout_recovery_retention as r;'+code)
        return w.run([sys.executable, '-I', '-B', '-c', program,
                      str(self.pack/'scripts'), *map(str, args)], check=check)

    def test_unknown_backup_even_empty_directory_is_not_disposable(self):
        for kind in ('file', 'directory', 'incomplete-staging'):
            with self.subTest(kind=kind), World() as w:
                directory, source, receipt = self.fixture(w)
                unknown = directory/('staging.json' if kind == 'incomplete-staging' else 'unknown-backup')
                if kind != 'directory':
                    unknown.write_bytes(b'{"observation_complete":false}\n' if kind == 'incomplete-staging' else b'Unowned evidence\n')
                    unknown.chmod(0o600)
                else:
                    unknown.mkdir(mode=0o700)
                before = source.read_bytes(), receipt.read_bytes()
                process = self.module(w, 'r._inventory(Path(sys.argv[1]))', directory, check=False)
                self.assertNotEqual(process.returncode, 0)
                self.assertEqual(before, (source.read_bytes(), receipt.read_bytes()))
                self.assertTrue(unknown.exists())

    def test_resolution_and_quiescence_metadata_remain_retained(self):
        with World() as w:
            directory, _, _ = self.fixture(w)
            names = ['coverage-reviews/'+'a'*64+'.json',
                     'union-reviews/'+str(uuid.uuid4())+'.json',
                     'discard-reviews/'+str(uuid.uuid4())+'.json',
                     'quiescence-reviews/'+str(uuid.uuid4())+'.json',
                     'ledger-transitions/00000001.json', 'quiescence-attestations.json']
            for name in names:
                target = directory/name
                target.parent.mkdir(mode=0o700, exist_ok=True)
                target.write_text('{}\n'); target.chmod(0o600)
            output = self.module(w, 'root=Path(sys.argv[1]);inventory=r._inventory(root);'
                                 'names=json.loads(sys.argv[2]);'
                                 'print(json.dumps({name:r._retained(name) and name in inventory["files"] for name in names}))',
                                 directory, json.dumps(names))
            self.assertTrue(all(json.loads(output.stdout).values()))

    def test_owned_incomplete_staging_is_retained_not_promoted_to_complete(self):
        with World() as w:
            directory, _, _ = self.fixture(w)
            value = {'schema': 'agent-memory/layout-recovery-staging-v1',
                     'operation_id': str(uuid.uuid4()), 'project_id': str(uuid.uuid4()),
                     'repo_root': str(w.paths['repo']), 'bank_path': str(w.paths['repo']/'memory-bank'),
                     'identity': {}, 'observation_complete': False}
            stage = directory/'staging.json'
            stage.write_text(json.dumps(value)); stage.chmod(0o600)
            output = self.module(w, 'root=Path(sys.argv[1]);inventory=r._inventory(root);'
                                 'print(json.dumps({"retained":r._retained("staging.json"),'
                                 '"present":"staging.json" in inventory["files"],'
                                 '"complete":json.loads((root/"staging.json").read_text())["observation_complete"]}))',
                                 directory)
            self.assertEqual(json.loads(output.stdout), {'retained': True, 'present': True, 'complete': False})

    def test_symlink_hardlink_and_unsafe_mode_never_gain_cleanup_ownership(self):
        for kind in ('symlink', 'hardlink', 'mode'):
            with self.subTest(kind=kind), World() as w:
                directory, source, receipt = self.fixture(w)
                if kind == 'mode':
                    source.chmod(0o644)
                else:
                    alias = directory/'originals'/(str(uuid.uuid4())+'.bin')
                    if kind == 'symlink':
                        alias.symlink_to(source)
                    else:
                        os.link(source, alias)
                process = self.module(w, 'r._inventory(Path(sys.argv[1]))', directory, check=False)
                self.assertNotEqual(process.returncode, 0)
                self.assertTrue(source.exists()); self.assertTrue(receipt.exists())

    def test_individual_unlink_keeps_receipt_and_directories_and_checks_changed_bytes(self):
        with World() as w:
            directory, source, receipt = self.fixture(w)
            inventory = self.module(w, 'print(json.dumps(r._inventory(Path(sys.argv[1]))))', directory)
            pins = json.loads(inventory.stdout)
            review = w.inside('exact inventory.json'); review.write_text(json.dumps(pins))
            relative = source.relative_to(directory).as_posix()
            source.write_bytes(b'External replacement must survive.\n')
            rejected = self.module(w, 'root=Path(sys.argv[1]);pins=json.loads(Path(sys.argv[3]).read_text());'
                                   'r._remove(root,sys.argv[2],pins["files"][sys.argv[2]],pins["directories"])',
                                   directory, relative, review, check=False)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertEqual(source.read_bytes(), b'External replacement must survive.\n')
            current = self.module(w, 'print(json.dumps(r._inventory(Path(sys.argv[1]))))', directory)
            review.write_text(current.stdout)
            self.module(w, 'root=Path(sys.argv[1]);pins=json.loads(Path(sys.argv[3]).read_text());'
                        'r._remove(root,sys.argv[2],pins["files"][sys.argv[2]],pins["directories"])',
                        directory, relative, review)
            self.assertFalse(source.exists()); self.assertTrue(receipt.exists())
            self.assertTrue(source.parent.is_dir()); self.assertTrue(directory.is_dir())

    def test_resume_accepts_only_the_one_unlink_before_progress_boundary(self):
        with World() as w:
            directory, source, receipt = self.fixture(w)
            second = source.parent/(str(uuid.uuid4())+'.bin')
            second.write_bytes(b'Second independently pinned original\n'); second.chmod(0o600)
            inventory = json.loads(self.module(w, 'print(json.dumps(r._inventory(Path(sys.argv[1]))))', directory).stdout)
            proposal = {'delete': {name: pin for name, pin in inventory['files'].items() if name.startswith('originals/')},
                        'retain': {'receipt.json': inventory['files']['receipt.json']}}
            path = w.inside('resume proposal.json'); path.write_text(json.dumps(proposal))
            ordered = sorted(proposal['delete'])
            (directory/ordered[0]).unlink()
            self.module(w, 'root=Path(sys.argv[1]);proposal=json.loads(Path(sys.argv[2]).read_text());'
                        'r._remaining(root,proposal,[])', directory, path)
            (directory/ordered[1]).unlink()
            refused = self.module(w, 'root=Path(sys.argv[1]);proposal=json.loads(Path(sys.argv[2]).read_text());'
                                  'r._remaining(root,proposal,[])', directory, path, check=False)
            self.assertNotEqual(refused.returncode, 0)
            self.assertTrue(receipt.exists())

    def public_repair(self, w, *, pending=False):
        from layout_recovery_fixtures import installed, memory, inspect, prepare, call
        command, source = installed(w)
        repo, bank, intended = memory(w, oversized=not pending, handoff=pending)
        _, observed = inspect(w, command, repo)
        proposal_path, proposal = prepare(w, command, source, repo, observed, intended)
        applied = call(w, command, 'apply', '--repo', repo, '--proposal', proposal_path)
        return command, source, repo, bank, observed, proposal_path, proposal, applied

    def reviewed_cleanup(self, w, command, repo, observed, layout_proposal):
        from datetime import datetime, timezone
        from layout_recovery_fixtures import call, save, sha, encoded
        output = w.inside('retention-preview-'+str(uuid.uuid4())+'.json')
        call(w, command, 'retention', 'inspect', '--repo', repo, '--operation', observed['operation_id'], '--output', output)
        proposal = json.loads(output.read_text())['proposal']
        author = layout_proposal['proposal']['author']
        review = {'schema': 'agent-memory/layout-retention-review-v1', 'review_id': str(uuid.uuid4()),
                  'cleanup_id': proposal['cleanup_id'], 'preview_sha256': sha(encoded(proposal)),
                  'reviewer': {**author, 'harness': 'controlled-retention-reviewer', 'session': str(uuid.uuid4())},
                  'reviewed_at': datetime.now(timezone.utc).isoformat(), 'decision': 'approve',
                  'dependency_review_complete': True, 'rollback_loss_acknowledged': True,
                  'operator_authorization': {'developer_id': author['developer_id'],
                      'author_harness': author['harness'], 'author_session': author['session'],
                      'evidence': 'Controlled disposable authorization; not independent release acceptance',
                      'statement': 'Delete only this exact reviewed terminal evidence list; surrender future rollback'},
                  'rationale': 'Complete controlled dependency inventory has no pending source or live restoration/publication dependency'}
        reviewed = w.inside('reviewed-retention-'+str(uuid.uuid4())+'.json')
        call(w, command, 'retention', 'inspect', '--repo', repo, '--operation', observed['operation_id'],
             '--input', save(w, {'cleanup_id': proposal['cleanup_id'], 'review': review}), '--output', reviewed)
        return reviewed, json.loads(reviewed.read_text())['proposal']

    def test_public_terminal_cleanup_crash_resume_and_real_historical_retry(self):
        from layout_recovery_fixtures import call
        with World() as w:
            command, _, repo, bank, observed, layout_path, layout_proposal, applied = self.public_repair(w)
            self.assertTrue(applied['semantic_recovery_complete'])
            before_bank = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            index_path = Path(w.git('rev-parse', '--path-format=absolute', '--git-path', 'index', cwd=repo).stdout.strip())
            before_index = index_path.read_bytes() if index_path.exists() else None
            before_head = w.git('rev-parse', 'HEAD', cwd=repo).stdout
            reviewed_path, reviewed = self.reviewed_cleanup(w, command, repo, observed, layout_proposal)
            call(w, command, 'retention', 'apply', '--repo', repo, '--operation', observed['operation_id'],
                 '--proposal', reviewed_path, expected=1,
                 env={**w.env, 'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-retention-after-delete-0'})
            status = call(w, command, 'status', '--repo', repo, '--operation', observed['operation_id'])
            self.assertFalse(status.get('original_evidence_available', False))
            call(w, command, 'retention', 'apply', '--repo', repo, '--operation', observed['operation_id'], '--proposal', reviewed_path)
            again = call(w, command, 'retention', 'apply', '--repo', repo, '--operation', observed['operation_id'], '--proposal', reviewed_path)
            self.assertTrue(again.get('receipt_is_historical'))
            historical = call(w, command, 'apply', '--repo', repo, '--proposal', layout_path)
            self.assertTrue(historical['receipt_is_historical'])
            self.assertEqual(before_bank, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})
            self.assertEqual(before_index, index_path.read_bytes() if index_path.exists() else None)
            self.assertEqual(before_head, w.git('rev-parse', 'HEAD', cwd=repo).stdout)

    def test_public_pending_units_cannot_be_deleted_by_retention(self):
        from layout_recovery_fixtures import call
        with World() as w:
            command, _, repo, bank, observed, _, _, applied = self.public_repair(w, pending=True)
            self.assertFalse(applied['semantic_recovery_complete'])
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            call(w, command, 'retention', 'inspect', '--repo', repo, '--operation', observed['operation_id'],
                 '--output', w.inside('refused retention.json'), expected=1)
            self.assertEqual(before, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})

    def test_public_child_restore_cleanup_uses_real_parent_receipts_without_child_ledger(self):
        from layout_recovery_fixtures import call
        with World() as w:
            command, _, repo, bank, observed, _, layout_proposal, _ = self.public_repair(w)
            child_path = w.inside('child restore preview.json')
            child = call(w, command, 'rollback', '--repo', repo, '--operation', observed['operation_id'], '--output', child_path)
            child_id = child['operation_id']
            call(w, command, 'retention', 'inspect', '--repo', repo, '--operation', observed['operation_id'],
                 '--output', w.inside('unfinished child refusal.json'), expected=1)
            call(w, command, 'rollback', '--repo', repo, '--proposal', child_path)
            current = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            reviewed, proposal = self.reviewed_cleanup(w, command, repo, {'operation_id': child_id}, layout_proposal)
            self.assertEqual(proposal['terminal']['kind'], 'child-restore')
            call(w, command, 'retention', 'apply', '--repo', repo, '--operation', child_id, '--proposal', reviewed)
            # A completed child has no remaining canonical dependency. Parent
            # cleanup may surrender originals while both real receipts survive.
            parent_review, _ = self.reviewed_cleanup(w, command, repo, observed, layout_proposal)
            call(w, command, 'retention', 'apply', '--repo', repo, '--operation', observed['operation_id'], '--proposal', parent_review)
            historical = call(w, command, 'rollback', '--repo', repo, '--proposal', child_path)
            self.assertTrue(historical['receipt_is_historical'])
            self.assertEqual(current, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})


if __name__ == '__main__':
    unittest.main()
