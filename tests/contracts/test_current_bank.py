"""Complete canonical Markdown with optional Git facts and no application state."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / 'scripts/bank_state.py'
RENDER = ROOT / 'scripts/bank_contract.py'
BODIES = {
    'projectbrief.md': '# Project brief\n\nBounded retry helper only; [goals](../README.md).\n',
    'productContext.md': '# Product context\n\nReduce repeated transient work; [problem](../README.md).\n',
    'activeContext.md': '# Active context\n\nFocus: inspect retry limit.\nEvidence: [limit](../retry.py).\nOpen: integration behavior remains unverified.\nNext: add an integration fixture.\n',
    'systemPatterns.md': '# System patterns\n\nThe retry limit is explicit; [implementation](../retry.py).\n',
    'techContext.md': '# Technical context\n\nPython standard library only; [source](../retry.py).\n',
    'progress.md': '# Progress\n\nThe isolated limit probe returned 3; [actual probe output](../evidence.txt).\nIntegration behavior remains unverified.\n'
}


class CurrentBankContract(unittest.TestCase):
    def command(self, w, repo, helper=HELPER):
        p = w.run([sys.executable, '-I', helper, 'inspect', '--repo', repo], cwd=w.root, check=False)
        return p, json.loads(p.stdout)

    def fixture(self, w, *, tracked=False, complete=False):
        repo = w.repository() if tracked else w.inside('plain-project')
        repo.mkdir(exist_ok=True)
        (repo / 'README.md').write_text('Bounded retry helper; reduce transient work.\n')
        (repo / 'retry.py').write_text('def limit():\n    return 3\n')
        result = w.run([sys.executable, '-I', '-c', 'import runpy; print(runpy.run_path("retry.py")["limit"]())'], cwd=repo)
        self.assertEqual(result.stdout.strip(), '3')
        (repo / 'evidence.txt').write_text(result.stdout)
        names = list(BODIES) if complete else ['activeContext.md']
        proposal = {'project-id': str(uuid.uuid4()), 'router-updated': '2026-10-22T20:00:00Z',
                    'contexts': {name: {'body': BODIES[name], 'purpose': 'Useful ' + name.removesuffix('.md'),
                         'scope': 'repository', 'lifecycle': 'active', 'read-when': 'When continuing retry work'} for name in names},
                    'omitted': {name: 'No additional verified context needed yet' for name in BODIES if name not in names}}
        source = w.inside('proposal.json')
        source.write_text(json.dumps(proposal))
        rendered = w.run([sys.executable, '-I', RENDER, 'render', '--input', source])
        bank = repo / 'memory-bank'
        bank.mkdir()
        for name, body in json.loads(rendered.stdout)['files'].items():
            (bank / name).write_text(body)
        if tracked:
            w.git('add', '.', cwd=repo)
            w.git('commit', '-m', 'Canonical current fixture with source evidence', cwd=repo)
        return repo, bank, proposal

    def test_minimal_untracked_bank_survives_poisoned_then_removed_application_state(self):
        with World() as w:
            repo, bank, proposal = self.fixture(w)
            poison = w.paths['state'] / 'agent-memory'
            poison.mkdir()
            (poison / 'current.json').write_text('{"body":"STALE_CACHE_CANARY"}')
            (w.paths['cache'] / 'index.sqlite').write_text('Not a usable database; must not be opened')
            (w.paths['data'] / 'events.jsonl').write_text('STALE_CACHE_CANARY\n')
            p, before = self.command(w, repo)
            self.assertEqual(p.returncode, 0, before)
            self.assertFalse(before['reads_memory_generated_state'])
            self.assertEqual(before['git']['status'], 'not-discovered')
            self.assertEqual(len(before['contexts']), 6)
            self.assertTrue(before['contexts']['activeContext.md']['present'])
            for name in BODIES:
                if name != 'activeContext.md':
                    self.assertFalse(before['contexts'][name]['present'])
                    self.assertTrue(before['contexts'][name]['omission_reason'])
            current = {p.name: p.read_bytes() for p in bank.iterdir()}
            for name in ('state', 'cache', 'data', 'config'):
                shutil.rmtree(w.paths[name])
            p, after = self.command(w, repo)
            self.assertEqual(p.returncode, 0, after)
            self.assertEqual(after, before)
            self.assertEqual({p.name: p.read_bytes() for p in bank.iterdir()}, current)
            self.assertEqual(w.run(['cat', bank / 'activeContext.md']).stdout, BODIES['activeContext.md'])
            self.assertNotIn('STALE_CACHE_CANARY', p.stdout)
            self.assertFalse(after['sharing']['publication_verified'])
            self.assertFalse(after['sharing']['freshness_verified'])

    def test_all_six_tracked_contexts_retain_git_evidence_without_claiming_uncommitted_bytes_are_shared(self):
        with World() as w:
            repo, bank, proposal = self.fixture(w, tracked=True, complete=True)
            head = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            (repo / 'feature.py').write_text('unfinished = True\n')
            w.git('add', 'feature.py', cwd=repo)
            (repo / 'scratch.txt').write_text('Preserve working copy\n')
            target = bank / 'activeContext.md'
            target.write_text(target.read_text().replace('add an integration fixture', 'inspect the second clone'))
            index = (repo / '.git/index').read_bytes()
            current = {p.name: p.read_bytes() for p in bank.iterdir()}
            for name in ('state', 'cache', 'data', 'config'):
                shutil.rmtree(w.paths[name])
            p, result = self.command(w, repo)
            self.assertEqual(p.returncode, 0, result)
            self.assertTrue(all(c['present'] for c in result['contexts'].values()))
            self.assertEqual(result['git']['context_head'], head)
            self.assertEqual(result['git']['indexed_paths'], sorted(current))
            self.assertEqual(result['git']['not_indexed_paths'], [])
            self.assertEqual(result['git']['current_bytes_committed'], 'not-assessed')
            self.assertFalse(result['sharing']['publication_verified'])
            self.assertFalse(result['sharing']['freshness_verified'])
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip(), head)
            self.assertEqual((repo / 'scratch.txt').read_text(), 'Preserve working copy\n')
            self.assertEqual({p.name: p.read_bytes() for p in bank.iterdir()}, current)
            self.assertIn('[actual probe output](../evidence.txt)', (bank / 'progress.md').read_text())
            clone = w.clone(repo, 'fresh-clone')
            p, fresh = self.command(w, clone)
            self.assertEqual(p.returncode, 0, fresh)
            self.assertEqual(fresh['project_id'], proposal['project-id'])
            self.assertEqual(w.run(['cat', clone / 'memory-bank/activeContext.md']).stdout, BODIES['activeContext.md'])
            self.assertIn('second clone', target.read_text())

    def test_missing_git_still_reads_the_copied_portable_markdown_bank(self):
        with World() as w:
            repo, bank, _ = self.fixture(w)
            pack = w.inside('portable current pack')
            shutil.copytree(ROOT / 'skills/agent-memory', pack)
            empty = w.inside('empty-bin')
            empty.mkdir()
            w.env['PATH'] = str(empty)
            p, result = self.command(w, repo, pack / 'scripts/bank_state.py')
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['git']['status'], 'unavailable')
            self.assertEqual(result['canonical_source'], 'Markdown')
            self.assertEqual(set(result['source_hashes']), {'MEMORY.md', 'activeContext.md'})
            self.assertFalse(result['writes_performed'])

    def test_git_root_spelling_preserves_trailing_spaces(self):
        with World() as w:
            repo, bank, _ = self.fixture(w, tracked=True)
            renamed = w.inside('repo with trailing space ')
            repo.rename(renamed)
            p, result = self.command(w, renamed)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['git']['status'], 'checkout')
            self.assertEqual(result['git']['not_indexed_paths'], [])

    def test_busy_or_interrupted_bank_is_not_repaired_by_state_loss(self):
        with World() as w:
            repo, bank, _ = self.fixture(w)
            for name in ('state', 'cache', 'data'):
                shutil.rmtree(w.paths[name])
            for name in ('.agent-memory.lock', '.agent-memory-transaction.md'):
                marker = bank / name
                marker.write_text('Unknown recovery evidence; preserve it\n')
                before = {p.name: p.read_bytes() for p in bank.iterdir()}
                p, result = self.command(w, repo)
                self.assertEqual(p.returncode, 1, result)
                self.assertEqual({p.name: p.read_bytes() for p in bank.iterdir()}, before)
                marker.unlink()

    def test_git_inspection_ignores_ambient_repo_redirection_and_does_not_execute_fsmonitor_or_write_trace(self):
        with World() as w:
            repo, bank, _ = self.fixture(w, tracked=True)
            foreign = w.repository('foreign')
            canary = w.inside('fsmonitor-ran')
            script = w.inside('fsmonitor-canary')
            script.write_text('#!/usr/bin/env python3\nfrom pathlib import Path\nPath(' + repr(str(canary)) + ').write_text("ran")\n')
            script.chmod(0o700)
            w.git('config', 'core.fsmonitor', str(script), cwd=repo)
            head = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            index = (repo / '.git/index').read_bytes()
            trace = w.inside('git-trace.json')
            w.env['GIT_DIR'] = str(foreign / '.git')
            w.env['GIT_TRACE2_EVENT'] = str(trace)
            p, result = self.command(w, repo)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['git']['context_head'], head)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertFalse(canary.exists())
            self.assertFalse(trace.exists())


if __name__ == '__main__':
    unittest.main()
