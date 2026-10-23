"""Disposable copied-pack setup shared by the release safety canaries."""
import json
from pathlib import Path
import shutil
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
CONTEXTS = ['projectbrief.md', 'productContext.md', 'activeContext.md',
            'systemPatterns.md', 'techContext.md', 'progress.md']


class PortableCanaryFixture:
    def helper(self, world, script, *args):
        result = world.run([sys.executable, '-I', self.pack / 'scripts' / script, *args], check=False)
        self.assertTrue(result.stdout, result.stderr)
        return result, json.loads(result.stdout)

    def good(self, world, script, *args):
        result, value = self.helper(world, script, *args)
        self.assertEqual(result.returncode, 0, value)
        return value

    def save(self, world, value, name='request.json'):
        path = world.inside(name)
        path.write_text(json.dumps(value))
        return path

    def apply(self, world, script, proposal):
        return self.good(world, script, 'apply', '--proposal', self.save(world, proposal))

    def developer(self, world, *, adopt=None):
        options = ['--adopt', adopt] if adopt else ['--create', '--label', 'Disposable canary developer']
        proposal = self.good(world, 'developer_identity.py', 'preview', *options)
        return self.apply(world, 'developer_identity.py', proposal)['developer_id']

    def bind(self, world, repo):
        proposal = self.good(world, 'scope_binding.py', 'preview', '--repo', repo,
                             '--action', 'bind', '--reason', 'Explicit disposable canary bank adoption')
        self.assertTrue(self.apply(world, 'scope_binding.py', proposal)['verified'])
        return self.good(world, 'scope_binding.py', 'require', '--repo', repo)

    def fixture(self, world):
        self.pack = world.inside('copied portable skill')
        shutil.copytree(ROOT / 'skills/agent-memory', self.pack,
                        ignore=shutil.ignore_patterns('__pycache__'))
        repo = world.repository()
        (repo / 'app.py').write_text('RETRY_LIMIT = 3\n')
        (repo / 'README.md').write_text('# Retry investigation\n\nReproduce before generalizing.\n')
        project = str(uuid.uuid4())
        contexts = {
            'activeContext.md': {'body': '# Focus\n\nInvestigate retry behavior.\n\n'
                '## Original evidence\n\nSource: [implementation](../app.py); applicability elsewhere is unknown.\n',
                'purpose': 'Retry investigation', 'scope': 'component:retry', 'lifecycle': 'active',
                'read-when': 'When changing retry behavior'},
            'progress.md': {'body': '# Legacy context\n\nRETIRED_BRANCH_ONLY_CANARY\n',
                'purpose': 'Earlier branch history', 'scope': 'branch:legacy', 'lifecycle': 'expired',
                'read-when': 'Only when investigating the legacy branch'},
        }
        request = {'project-id': project, 'router-updated': '2026-10-22T18:00:00Z',
                   'contexts': contexts, 'omitted': {p: 'No distinct evidence' for p in CONTEXTS if p not in contexts}}
        rendered = self.good(world, 'bank_contract.py', 'render', '--input', self.save(world, request))
        bank = repo / 'memory-bank'
        bank.mkdir()
        for path, body in rendered['files'].items():
            (bank / path).write_text(body)
        developer = self.developer(world)
        self.bind(world, repo)
        world.git('add', '.', cwd=repo)
        world.git('commit', '-m', 'Disposable source and memory bank', cwd=repo)
        (repo / 'app.py').write_text('RETRY_LIMIT = 4  # unrelated staged work\n')
        world.git('add', 'app.py', cwd=repo)
        return repo, bank, project, developer

    def code_state(self, world, repo):
        return ((repo / 'app.py').read_bytes(), (repo / '.git/index').read_bytes(),
                world.git('rev-parse', 'HEAD', cwd=repo).stdout)

    def bank_state(self, bank):
        return {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}

    def policy(self, world, repo, project, **changes):
        request = {'project_id': project, 'category': 'admission', 'source_class': 'assistant',
                   'content_kind': 'summary', 'text': 'Retry behavior needs independent reproduction in this component.',
                   'record_ids': [], 'automatic': True, 'repository_evidence': None, **changes}
        return self.helper(world, 'security_policy.py', '--repo', repo,
                           '--request', self.save(world, request))

    def checkpoint(self, world, repo, developer):
        current = self.good(world, 'checkpoint.py', 'inspect', '--repo', repo)
        request = {'focus': 'Investigate retry behavior in the verified component.',
                   'recent_changes': ['The staged implementation uses four retries; a tool report claims five.'],
                   'open_questions': ['Can the five-retry claim be reproduced in another clone?'],
                   'next_step': 'Run an independent reproduction before drawing conclusions.',
                   'conditions': {'component': 'retry', 'branch': 'main'}, 'evidence': ['../app.py'],
                   'harness': 'custom', 'session': 'canary-session',
                   'observed_active_sha256': current['observed_active_sha256']}
        proposal = self.good(world, 'checkpoint.py', 'preview', '--repo', repo,
                             '--input', self.save(world, request))
        done = self.apply(world, 'checkpoint.py', proposal)
        self.assertTrue(done['verified'])
        self.assertIn(developer, (repo / 'memory-bank/activeContext.md').read_text())
        return done

