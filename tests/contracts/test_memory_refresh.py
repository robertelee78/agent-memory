"""Actual public refresh preserves stale feature code, index and unpublished learning."""
import unittest
import json
import sys
import time
import hashlib

from world import World
from contracts import test_retrieval as retrieval_helpers


from refresh_fixtures import RefreshFixture


class MemoryRefreshContract(RefreshFixture):
    def test_current_equal_bank_and_orphan_journal(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            before = self.state(w, repo, bank)
            def run():
                response = w.run([sys.executable, '-I', self.pack/'scripts/installer.py',
                                  'refresh', 'current', '--repo', repo], check=False)
                return response, json.loads(response.stdout)
            response, result = run()
            self.assertEqual(response.returncode, 0, result)
            self.assertEqual(result['sharing_state'], 'bank-synchronized')
            self.assertEqual(result['pending_capture'], 'unknown')
            self.assertEqual(result['pending_publication'], 'unknown')
            self.assertFalse(result['semantic_truth_verified'])
            self.assertEqual(before, self.state(w, repo, bank))
            bank_hash = hashlib.sha256(str(bank).encode()).hexdigest()
            journal = w.paths['state']/'agent-memory'/'writes'/bank_hash/'transactions'/'unknown'/'journal.json'
            journal.parent.mkdir(parents=True, exist_ok=True);journal.write_text('{}')
            response, result = run()
            self.assertEqual(response.returncode, 1)
            self.assertNotEqual(result['sharing_state'], 'bank-synchronized')
            self.assertTrue(journal.exists())
            self.assertEqual(before, self.state(w, repo, bank))

    def test_current_refuses_local_and_authority_changes_at_final_recheck(self):
        for scenario in ('local', 'authority', 'index'):
            with self.subTest(scenario=scenario), World() as w:
                repo, bank, peer, base = self.fixture(w)
                rendezvous = w.inside('rendezvous');rendezvous.mkdir()
                stage = 'refresh-current-before-final-recheck'
                w.env.update(AGENT_MEMORY_FAULT_STAGE=stage, AGENT_MEMORY_FAULT_MODE='pause',
                             AGENT_MEMORY_FAULT_DIR=str(rendezvous))
                child = w.spawn([sys.executable, '-I', self.pack/'scripts/installer.py',
                                 'refresh', 'current', '--repo', repo], cwd=repo)
                ready = rendezvous/(stage+'.ready');deadline = time.monotonic()+10
                while not ready.exists() and time.monotonic()<deadline:
                    time.sleep(.01)
                self.assertTrue(ready.exists(), 'current observation did not reach its final recheck')
                if scenario == 'local':
                    target = bank/'activeContext.md';target.write_text(target.read_text()+'\nLocal changed during observation.\n')
                elif scenario == 'authority':
                    target = peer/'memory-bank'/'activeContext.md';target.write_text(target.read_text()+'\nAuthority changed during observation.\n')
                    w.git('add', 'memory-bank', cwd=peer);w.git('commit', '-m', 'Concurrent memory change', cwd=peer)
                    w.git('push', 'origin', 'HEAD:main', cwd=peer)
                else:
                    target = repo/'code.py';target.write_text('VALUE = 987  # changed during observation\n')
                    w.git('add', 'code.py', cwd=repo)
                (rendezvous/(stage+'.release')).touch()
                stdout, stderr = child.communicate(timeout=30);result=json.loads(stdout)
                self.assertNotEqual(result['sharing_state'], 'bank-synchronized', (result,stderr))
                self.assertFalse(result['remote_freshness_verified'])
                self.assertIn('changed during observation', target.read_text())

    def test_current_refuses_unchanged_hidden_record_and_corrupt_hold_anchor(self):
        from contracts.test_semantic_sharing import SemanticSharingContract, comparison, project_id
        for scenario in ('hidden', 'hold-anchor'):
            with self.subTest(scenario=scenario), World() as w:
                repo, bank, peer, base = self.fixture(w)
                if scenario == 'hidden':
                    meta, path = SemanticSharingContract().add(peer/'memory-bank', 'hidden', '# Hidden fixture\n\nA hidden lesson.\n')
                    meta['visibility'] = 'hidden'
                    (peer/'memory-bank'/path).write_text(comparison.render(meta, '# Hidden fixture\n\nA hidden lesson.\n'))
                    w.git('add', 'memory-bank', cwd=peer);w.git('commit', '-m', 'Hidden fixture', cwd=peer)
                    w.git('push', 'origin', 'HEAD:main', cwd=peer);w.git('fetch', 'origin', 'main', cwd=repo)
                    w.git('reset', '--hard', 'FETCH_HEAD', cwd=repo)
                else:
                    bank_hash=hashlib.sha256(str(bank).encode()).hexdigest()
                    directory=w.paths['state']/'agent-memory'/'writes'/bank_hash/'semantic-holds'
                    directory.mkdir(parents=True,exist_ok=True)
                    anchor=directory/(project_id((bank/'MEMORY.md').read_text())+'.present.json')
                    anchor.write_text('corrupt')
                before=self.state(w,repo,bank)
                response=w.run([sys.executable,'-I',self.pack/'scripts/installer.py',
                                'refresh','current','--repo',repo],check=False)
                result=json.loads(response.stdout)
                self.assertNotEqual(result['sharing_state'],'bank-synchronized',result)
                self.assertFalse(result['remote_freshness_verified'])
                self.assertEqual(before,self.state(w,repo,bank))
                if scenario == 'hold-anchor':self.assertEqual(anchor.read_text(),'corrupt')

    def test_public_cli_rejects_relative_outputs_before_any_refresh_operation(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            before = self.state(w, repo, bank)
            for operation, flags in (
                ('inspect', ['--repo', str(repo)]),
                ('prepare', ['--repo', str(repo), '--input', 'unused.json']),
                ('apply', ['--input', 'unused.json']),
            ):
                with self.subTest(operation=operation):
                    response = w.run([sys.executable, '-I', self.pack/'scripts/installer.py',
                        'refresh', operation, '--v2', *flags, '--output', 'private-secret-output.json'],
                        cwd=repo, check=False)
                    result = json.loads(response.stdout)
                    self.assertEqual(response.returncode, 1)
                    self.assertEqual(result['failure_code'], 'private-output-path-invalid')
                    self.assertFalse(result['canonical_writes_performed'])
                    self.assertFalse(result['git_metadata_writes_performed'])
                    self.assertIn('absolute', result['repair'])
                    self.assertNotIn('private-secret-output.json', response.stdout + response.stderr)
                    self.assertFalse((repo/'private-secret-output.json').exists())
                    self.assertEqual(before, self.state(w, repo, bank))

    def test_public_sharing_commands_reject_unsafe_output_before_work(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            before = self.state(w, repo, bank)
            unsafe=w.inside('unsafe-private-directory');unsafe.mkdir(mode=0o755);unsafe.chmod(0o755)
            occupied=w.inside('existing-private.json');occupied.write_text('preserve original')
            redirected=w.inside('redirected-private.json');redirected.symlink_to(occupied)
            for target in (unsafe/'secret-path.json', redirected):
                for command in (
                    ['semantic', 'inspect-v2', '--repo', str(repo)],
                    ['publish', 'prepare', '--repo', str(repo), '--input', 'unused.json'],
                    ['refresh', 'inspect', '--v2', '--repo', str(repo)],
                ):
                    with self.subTest(target=target.name, command=command[0]):
                        response=w.run([sys.executable,'-I',self.pack/'scripts/installer.py',
                                        *command,'--output',target],cwd=repo,check=False)
                        result=json.loads(response.stdout)
                        self.assertEqual(response.returncode,1,result)
                        self.assertEqual(result['failure_code'],'private-output-invalid')
                        self.assertIn('owner-only',result['repair'])
                        self.assertNotIn(str(target),response.stdout+response.stderr)
                        self.assertFalse(result['canonical_writes_performed'])
                        self.assertFalse(result.get('git_metadata_writes_performed',False))
                        self.assertEqual(before,self.state(w,repo,bank))
            self.assertEqual(occupied.read_text(),'preserve original')

    def test_private_output_preflight_preserves_identical_retry(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            target=w.inside('same-output.json')
            script=('import sys;sys.path.insert(0,sys.argv[1]);'
                    'from agent_memory import sharing_limits as s;'
                    'value={"operation":"fixture","value":"unchanged"};'
                    's.write_private(sys.argv[2],value,sys.argv[3]);'
                    's.preflight_private_output(sys.argv[2],sys.argv[3]);'
                    's.write_private(sys.argv[2],value,sys.argv[3])')
            response=w.run([sys.executable,'-I','-c',script,self.pack/'scripts',target,repo],check=False)
            self.assertEqual(response.returncode,0,response.stderr)

    def test_second_clone_import_is_recalled_with_conditions_without_code_index_or_local_learning_loss(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            (bank/'activeContext.md').write_text('# Current focus\n\nUnpublished local learning must survive.\n')
            (peer/'code.py').write_text('VALUE = 900  # newer authority code remains outside this branch\n')
            w.git('add', 'code.py', cwd=peer)
            record, path = self.imported(w, peer, 'imported-lesson', '# Imported learning\n\nA relevant shared lesson with provenance.\n', component='code.py')
            before = self.state(w, repo, bank); unrelated = (repo/'unrelated.txt').read_bytes()
            recall = retrieval_helpers.RetrievalContract().request(record['project-id'], ids=[record['id']], query='', context={'component':'code.py'})
            proposal = self.prepared_refresh(w, repo, recall=recall)
            self.assertEqual(before, self.state(w, repo, bank))
            done = self.apply_refresh(w, proposal)
            self.assertEqual(done['sharing_state'], 'refreshed'); self.assertTrue(done['refresh_applied'])
            self.assertTrue(done['remote_freshness_verified']); self.assertTrue(done['relevant_imports_read'])
            self.assertFalse(done['feature_code_freshness_verified']); self.assertIn('Unpublished local', (bank/'activeContext.md').read_text())
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:]); self.assertEqual(unrelated, (repo/'unrelated.txt').read_bytes())
            self.assertEqual(done['recall']['results'][0]['record'], record); self.assertEqual(done['recalled_import_paths'], [path])
            self.assertTrue(done['cursor']['stored']); self.assertTrue(done['cursor']['content_free'])
            repeated = self.apply_refresh(w, proposal)
            self.assertTrue(repeated['receipt']['already_applied']); self.assertFalse(repeated['canonical_writes_performed'])
            self.assertEqual(done['recall']['results'][0]['body'], repeated['recall']['results'][0]['body'])

    def test_cursor_advances_memory_without_feature_ancestry_and_old_retry_cannot_reverse_newer_import(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            first, path = self.imported(w, peer, 'first', '# First shared lesson\n\nPreserve its exact source.\n')
            proposal = self.prepared_refresh(w, repo); one = self.apply_refresh(w, proposal)
            before_code = self.state(w, repo, bank)[1:]
            second, other = self.imported(w, peer, 'second', '# Second shared lesson\n\nA later independent observation.\n')
            view = self.good(w, 'refresh_memory.py', 'inspect', '--repo', repo)
            self.assertEqual(view['baseline_source'], 'cursor')
            self.assertEqual(view['inspection']['binding']['base'], one['observed_authority_head'])
            two = self.apply_refresh(w, self.prepared_refresh(w, repo)); self.assertTrue(two['remote_freshness_verified'])
            before = self.state(w, repo, bank)
            old = self.apply_refresh(w, proposal)
            self.assertEqual(old['sharing_state'], 'historical-or-changed-local'); self.assertFalse(old['canonical_writes_performed'])
            self.assertFalse(old['relevant_imports_read']); self.assertEqual(before, self.state(w, repo, bank))
            self.assertEqual(before_code, before[1:]); self.assertTrue((bank/path).exists()); self.assertTrue((bank/other).exists())
