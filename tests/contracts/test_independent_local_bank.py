"""One useful local-bank workflow; no federation, optional store or live client."""
import json
from pathlib import Path
import shutil
import sys
import tarfile
import unittest

from world import World
from contracts import test_initialization as initialization

ROOT = Path(__file__).resolve().parents[2]


class IndependentLocalBankContract(unittest.TestCase):
    good = initialization.InitializationContract.good
    save = initialization.InitializationContract.save
    setup = initialization.InitializationContract.setup
    inventory = initialization.InitializationContract.inventory
    review = initialization.InitializationContract.review
    claim = initialization.InitializationContract.claim
    preview = initialization.InitializationContract.preview

    def command(self, w, name, *args, env=None):
        helper = self.pack / 'scripts' / (name + '.py')
        r = w.run([sys.executable, '-I', helper, *args], check=False, env=env)
        return r, json.loads(r.stdout) if r.stdout else None

    def bank_files(self, bank):
        return {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*.md')}

    def work_state(self, w, repo):
        return ((repo / '.git/index').read_bytes(), w.git('rev-parse', 'HEAD', cwd=repo).stdout,
                (repo / 'src/app.py').read_bytes(), (repo / 'feature.py').read_bytes())

    def initialized(self, w):
        self.pack = w.inside('standalone portable pack')
        shutil.copytree(ROOT / 'skills/agent-memory', self.pack, ignore=shutil.ignore_patterns('__pycache__'))
        repo, self.pid, self.dev = self.setup(w)
        w.git('add', '.', cwd=repo);w.git('commit', '-m', 'Local source evidence', cwd=repo)
        (repo / 'feature.py').write_text('unfinished = True\n');w.git('add', 'feature.py', cwd=repo)
        before = self.work_state(w, repo)
        observed = self.inventory(w, repo)
        self.assertEqual(observed['bank_state'], 'absent')
        # Actual source fixtures distinguish approved target from current code.
        claims = [self.claim('intent', 5, 'docs/adr/one.md'),
                  self.claim('implementation', 3, 'src/app.py', context='techContext.md')]
        proposal = self.preview(w, repo, self.review(observed, claims))
        done = self.good(w, 'initialize', 'apply', '--proposal', self.save(w, proposal))
        self.assertTrue(done['verified']);self.assertFalse(done['publication_verified'])
        self.assertEqual(before, self.work_state(w, repo))
        self.assert_no_federation(w)
        return repo, repo / 'memory-bank'

    def assert_no_federation(self, w):
        # These paths never exist in this world: no catalog, graph, embedding
        # index, service setup, credential or harness installation is supplied.
        for name in ('federation', 'catalog', 'graph', 'vector-store'):
            self.assertFalse(w.inside(name).exists())
        for name in ('codex', 'claude', 'opencode', 'credentials'):
            self.assertEqual(list(w.paths[name].rglob('*')), [])

    def checkpoint(self, w, repo):
        current = self.good(w, 'checkpoint', 'inspect', '--repo', repo)
        fields = {'focus': 'Inspect the local retry evidence before broadening applicability.',
                  'recent_changes': ['Initialization retained intended five retries and observed three separately.'],
                  'open_questions': ['Behavior outside the current retry component remains unverified.'],
                  'next_step': 'Inspect the recovery result and source conditions.',
                  'evidence': ['../src/app.py', '../docs/adr/one.md'],
                  'conditions': {'branch': 'main'}, 'harness': 'custom', 'session': 'local-offline-session',
                  'observed_active_sha256': current['observed_active_sha256']}
        self.assertFalse(current['preparation_required'])
        return self.good(w, 'checkpoint', 'preview', '--repo', repo,
                         '--input', self.save(w, fields, 'fields.json'))

    def test_initialize_checkpoint_resume_and_search_without_any_global_service(self):
        with World() as w:
            repo, bank = self.initialized(w);before = self.work_state(w, repo)
            proposal = self.checkpoint(w, repo)
            done = self.good(w, 'checkpoint', 'apply', '--proposal', self.save(w, proposal))
            self.assertTrue(done['verified']);self.assertFalse(done['publication_verified'])
            self.assertEqual(done['changed_memory_paths'], ['memory-bank/activeContext.md'])
            saved = self.bank_files(bank)
            resumed = self.good(w, 'checkpoint', 'inspect', '--repo', repo)
            self.assertIn('outside the current retry component remains unverified', resumed['active_context'])
            self.assertIn('../src/app.py', resumed['active_context'])
            request = {'project_id': self.pid, 'query': 'retry', 'ids': [], 'paths': [],
                       'context': {'branch': 'main'}, 'kinds': [], 'minimum_trust': 'reviewed',
                       'limit': 64, 'include_context': True}
            recalled = self.good(w, 'retrieve', '--repo', repo, '--request', self.save(w, request, 'recall.json'))
            self.assertTrue(recalled['results'])
            self.assertTrue(all(item['selection_class'] == 'orientation-only' for item in recalled['results']))
            text = '\n'.join(item['body'] for item in recalled['results'])
            self.assertIn('Inspect the local retry evidence', text)
            self.assertIn('Reviewed intent retry limit: 5', (bank / 'projectbrief.md').read_text())
            self.assertIn('Reviewed implementation retry limit: 3', (bank / 'techContext.md').read_text())
            checked = self.good(w, 'bank_contract', 'validate', '--repo', repo)
            self.assertEqual(checked['project_id'], self.pid)
            self.assertEqual(saved, self.bank_files(bank));self.assertEqual(before, self.work_state(w, repo))
            self.assert_no_federation(w)

    def test_version_export_and_plain_file_repair_after_disposable_state_loss(self):
        with World() as w:
            repo, bank = self.initialized(w)
            w.git('add', 'memory-bank', cwd=repo)
            w.git('commit', '--only', 'memory-bank', '-m', 'Version the local bank', cwd=repo)
            saved = self.bank_files(bank);before = self.work_state(w, repo)
            archive = w.inside('local-bank.tar')
            w.git('archive', '--format=tar', '--output', archive, 'HEAD', 'memory-bank', cwd=repo)
            exported = w.inside('export');exported.mkdir()
            with tarfile.open(archive) as stream:stream.extractall(exported, filter='data')
            self.assertEqual(saved, self.bank_files(exported / 'memory-bank'))
            # The export is an inspected same-project backup, not admission to
            # another repository or permission for cross-project sharing.
            for name in ('config', 'state', 'cache'):
                shutil.rmtree(w.paths[name])
            body = (bank / 'activeContext.md').read_text()
            (bank / 'activeContext.md').write_text(body + '\n## Manual note\n\nRetain the same project boundary.\n')
            self.assertIn('Manual note', w.run(['cat', bank / 'activeContext.md']).stdout)
            self.assertEqual(self.good(w, 'bank_contract', 'validate', '--repo', repo)['project_id'], self.pid)
            (bank / 'activeContext.md').write_text('   \n')
            r, _ = self.command(w, 'bank_contract', 'validate', '--repo', repo);self.assertEqual(r.returncode, 1)
            known = w.git('show', 'HEAD:memory-bank/activeContext.md', cwd=repo).stdout
            (bank / 'activeContext.md').write_text(known)
            self.assertEqual(self.good(w, 'bank_contract', 'validate', '--repo', repo)['project_id'], self.pid)
            self.assertEqual(saved, self.bank_files(bank));self.assertEqual(before, self.work_state(w, repo))
            for name in ('config', 'state', 'cache'):self.assertFalse(w.paths[name].exists())
            self.assert_no_federation(w)

    def test_local_interrupted_checkpoint_recovers_with_retained_private_receipt_evidence(self):
        with World() as w:
            repo, bank = self.initialized(w);before = self.work_state(w, repo)
            proposal = self.checkpoint(w, repo);file = self.save(w, proposal)
            r, _ = self.command(w, 'checkpoint', 'apply', '--proposal', file,
                                env={'AGENT_MEMORY_FAULT_STAGE': 'after-replace', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(r.returncode, 86);self.assertTrue((bank / '.agent-memory.lock').exists())
            self.assertEqual(self.command(w, 'bank_contract', 'validate', '--repo', repo)[0].returncode, 1)
            unlock = self.good(w, 'bank_writes', 'recover', '--repo', repo)
            self.good(w, 'bank_writes', 'recover', '--repo', repo, '--proposal', self.save(w, unlock, 'unlock.json'))
            done = self.good(w, 'checkpoint', 'apply', '--proposal', file)
            self.assertTrue(done['verified']);self.assertFalse(done['writes_performed'])
            self.assertFalse((bank / '.agent-memory.lock').exists())
            self.assertEqual(self.good(w, 'bank_contract', 'validate', '--repo', repo)['project_id'], self.pid)
            self.assertEqual(before, self.work_state(w, repo));self.assert_no_federation(w)
