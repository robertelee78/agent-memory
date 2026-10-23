"""Memory-only publication through isolated public commands and real Git remotes."""
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World
from identity_fixtures import bind
from contracts import test_semantic_sharing as semantic
from contracts import test_sharing_recovery as recovery
import publication_github

ROOT = Path(__file__).resolve().parents[1]


class PublicationFixture(unittest.TestCase):
    save = recovery.SharingRecoveryContract.save
    judgment = semantic.SemanticSharingContract.judgment

    def state(self, w, repo, bank):
        index = Path(w.git('rev-parse', '--path-format=absolute', '--git-path', 'index', cwd=repo).stdout.strip())
        return ({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()},
                (repo/'code.py').read_bytes(), index.read_bytes(), w.git('rev-parse', 'HEAD', cwd=repo).stdout,
                w.git('symbolic-ref', 'HEAD', cwd=repo).stdout)

    def call(self, w, helper, *args):
        r = w.run([sys.executable, '-I', self.pack/'scripts'/helper, *args], check=False, timeout=120)
        return r, json.loads(r.stdout)

    def good(self, w, helper, *args):
        r, value = self.call(w, helper, *args)
        self.assertEqual(r.returncode, 0, (value, r.stderr))
        return value

    def fixture(self, w):
        helper = recovery.SharingRecoveryContract()
        result = helper.fixture(w)
        self.pack = helper.pack
        return result

    def bootstrap(self, w):
        self.pack = w.inside('portable pack')
        shutil.copytree(ROOT/'skills/agent-memory', self.pack, ignore=shutil.ignore_patterns('__pycache__'))
        repo = w.repository(); remote = w.repository('remote.git', bare=True)
        w.git('remote', 'add', 'origin', str(remote), cwd=repo)
        w.git('config', '--local', 'agent-memory.authorityBranch', 'main', cwd=repo)
        (repo/'code.py').write_text('VALUE = 1\n')
        w.git('add', 'code.py', cwd=repo); w.git('commit', '-m', 'Initial authority code only', cwd=repo)
        w.git('push', 'origin', 'HEAD:main', cwd=repo)
        base = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
        proposal = json.loads((self.pack/'templates/router-proposal.example.json').read_text())
        proposal['project-id'] = str(uuid.uuid4())
        files = self.good(w, 'bank_contract.py', 'render', '--input', self.save(w, proposal))['files']
        bank = repo/'memory-bank'; bank.mkdir()
        for path, body in files.items(): (bank/path).write_text(body)
        bind(w, repo)
        w.git('checkout', '-b', 'feature', cwd=repo)
        (repo/'code.py').write_text('VALUE = 7  # unfinished feature ancestor\n')
        w.git('add', 'code.py', cwd=repo); w.git('commit', '-m', 'Unfinished feature code', cwd=repo)
        (repo/'code.py').write_text('VALUE = 8  # staged feature work\n'); w.git('add', 'code.py', cwd=repo)
        (repo/'unrelated.txt').write_text('Unstaged independent work\n')
        return repo, bank, base

    def proof(self, w, repo):
        inspection = self.good(w, 'reconcile_memory.py', 'inspect', '--repo', repo)
        judgment = self.judgment(inspection)
        return self.good(w, 'reconcile_memory.py', 'review', '--repo', repo,
                         '--input', self.save(w, judgment))['proof']

    def github(self, w, repo, bank, peer=None, *, protected=False):
        remote = Path(w.git('remote', 'get-url', 'origin', cwd=repo).stdout.strip())
        policy = publication_github.install(w, repo, remote, protected=protected)
        w.git('remote', 'set-url', 'origin', 'https://github.com/fixture/memory.git', cwd=repo)
        proposal = self.good(w, 'scope_binding.py', 'preview', '--repo', repo, '--action', 'rebind',
                             '--reason', 'Explicit isolated transport fixture for the same project GitHub authority')
        self.good(w, 'scope_binding.py', 'apply', '--proposal', self.save(w, proposal))
        if peer is not None:
            (peer/'memory-bank/MEMORY.md').write_bytes((bank/'MEMORY.md').read_bytes())
            w.git('add', 'memory-bank/MEMORY.md', cwd=peer)
            w.git('commit', '-m', 'Explicit canonical authority rebind fixture', cwd=peer)
            w.git('push', 'origin', 'HEAD:main', cwd=peer)
        publication_github.enforce(w, remote)
        return policy

    def prepared(self, w, repo):
        proof = self.proof(w, repo)
        return self.good(w, 'publish_memory.py', 'prepare', '--repo', repo,
                         '--input', self.save(w, proof))

    def land(self, w, repo, request):
        return self.good(w, 'publish_memory.py', 'land', '--repo', repo,
                         '--input', self.save(w, request, 'publication-request.json'))

