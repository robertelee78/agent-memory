"""Source-bound disposable projection and observed-write integration contracts."""
import copy
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World
from record_fixtures import record_text

ROOT = Path(__file__).resolve().parents[2]


class ProjectionContract(unittest.TestCase):
    def command(self, w, script, *args, env=None):
        result = w.run([sys.executable, '-I', self.pack / 'scripts' / script, *args], check=False, env=env)
        return result, json.loads(result.stdout) if result.stdout else None

    def good(self, w, script, *args):
        r, v = self.command(w, script, *args)
        self.assertEqual(r.returncode, 0, v or r.stderr)
        return v

    def save(self, w, value, name='request.json'):
        path = w.inside(name)
        path.write_text(json.dumps(value))
        return path

    def fixture(self, w):
        self.pack = w.inside('portable pack with spaces')
        shutil.copytree(ROOT / 'skills/agent-memory', self.pack, ignore=shutil.ignore_patterns('__pycache__'))
        repo = w.repository()
        (repo / 'app.py').write_text('untouched = True\n')
        project = str(uuid.uuid4())
        proposal = json.loads((self.pack / 'templates/router-proposal.example.json').read_text())
        proposal['project-id'] = project
        proposal['contexts']['activeContext.md'] = {
            'body': '# Active context\n\nInvestigate the conditional source before generalizing.\n',
            'purpose': 'Current investigation', 'scope': 'repository', 'lifecycle': 'active',
            'read-when': 'When resuming the investigation'}
        proposal['omitted'].pop('activeContext.md')
        rendered = self.good(w, 'bank_contract.py', 'render', '--input', self.save(w, proposal))
        bank = repo / 'memory-bank'
        bank.mkdir()
        for path, body in rendered['files'].items():
            (bank / path).write_text(body)
        record = record_text('# Canonical conditional result\n\nVerify applicability before use.\n', project)
        meta = json.loads(next(line.split(': ', 1)[1] for line in record.splitlines() if line.startswith('id: ')))
        (bank / 'topics').mkdir()
        (bank / 'topics/result.md').write_text(record)
        row = '| [topics/result.md](<topics/result.md>) | Verified source reference | repository | active | Before qualified recall |\n'
        router = bank / 'MEMORY.md'
        router.write_text(router.read_text().replace('\n## Omitted contexts', row + '\n## Omitted contexts'))
        dev = self.good(w, 'developer_identity.py', 'preview', '--create', '--label', 'Disposable projection developer')
        self.good(w, 'developer_identity.py', 'apply', '--proposal', self.save(w, dev))
        binding = self.good(w, 'scope_binding.py', 'preview', '--repo', repo, '--action', 'bind', '--reason', 'Explicit fixture bank adoption')
        self.good(w, 'scope_binding.py', 'apply', '--proposal', self.save(w, binding))
        w.git('add', '.', cwd=repo)
        w.git('commit', '-m', 'Disposable canonical fixture', cwd=repo)
        (repo / 'app.py').write_text('untouched = False  # unrelated staged change\n')
        w.git('add', 'app.py', cwd=repo)
        return repo, bank, project, {'id': meta, 'version': 1}

    def state(self, bank):
        return {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}

    def code(self, w, repo):
        return ((repo / 'app.py').read_bytes(), (repo / '.git/index').read_bytes(), w.git('rev-parse', 'HEAD', cwd=repo).stdout)

    def build(self, w, repo, project, references):
        return self.good(w, 'projection.py', 'build', '--repo', repo,
                         '--input', self.save(w, {'project_id': project, 'records': references}))

    def check(self, w, repo, projection):
        return self.command(w, 'projection.py', 'check', '--repo', repo, '--input', self.save(w, projection))

    def test_projection_contains_only_canonical_references_and_rebuilds_after_disposable_file_loss(self):
        with World() as w:
            repo, bank, project, reference = self.fixture(w)
            before, code = self.state(bank), self.code(w, repo)
            built = self.build(w, repo, project, [{**reference, 'body': 'OLD_CACHE_PAYLOAD', 'trust': 'verified'}])
            projection = built['projection']
            self.assertEqual(set(projection), {'schema', 'project_id', 'source_hashes', 'records'})
            self.assertEqual(projection['records'], [{**reference, 'path': 'topics/result.md'}])
            inspected = self.good(w, 'bank_writes.py', 'inspect', '--repo', repo)
            self.assertEqual(projection['source_hashes'], inspected['observed_hashes'])
            self.assertNotIn('OLD_CACHE_PAYLOAD', json.dumps(built))
            self.assertFalse(built['admission_verified'])
            self.assertFalse(built['current_guidance_verified'])
            r, checked = self.check(w, repo, projection)
            self.assertEqual(r.returncode, 0, checked)
            self.assertTrue(checked['valid'])
            local = self.save(w, projection, 'disposable-projection.json')
            local.unlink()
            self.assertEqual(self.build(w, repo, project, [reference])['projection'], projection)
            self.assertEqual(self.state(bank), before)
            self.assertEqual(self.code(w, repo), code)

    def test_changed_source_or_interrupted_bank_invalidates_without_echoing_old_hashes(self):
        with World() as w:
            repo, bank, project, reference = self.fixture(w)
            projection = self.build(w, repo, project, [reference])['projection']
            old = copy.deepcopy(projection)
            target = bank / 'activeContext.md'
            target.write_text(target.read_text() + '\nA new bounded observation.\n')
            r, result = self.check(w, repo, projection)
            self.assertEqual(r.returncode, 1, result)
            self.assertFalse(result['valid'])
            for sha in old['source_hashes'].values():
                self.assertNotIn(sha, r.stdout)
            fresh = self.build(w, repo, project, [reference])['projection']
            self.assertNotEqual(old['source_hashes'], fresh['source_hashes'])
            self.assertEqual(self.check(w, repo, fresh)[0].returncode, 0)
            marker = bank / '.agent-memory-transaction.md'
            marker.write_text('Unknown transaction; preserve for inspection.\n')
            self.assertEqual(self.check(w, repo, fresh)[0].returncode, 1)
            self.assertTrue(marker.exists())

    def test_exact_shape_project_and_reference_substitution_refuse_even_with_current_source_hashes(self):
        with World() as w:
            repo, _, project, reference = self.fixture(w)
            projection = self.build(w, repo, project, [reference])['projection']
            variants = [{**projection, 'body': 'NEVER_ACCEPT_EXTRA_MEMORY'},
                        {**projection, 'project_id': str(uuid.uuid4())},
                        {**projection, 'source_hashes': {}},
                        {**projection, 'records': [{**reference, 'path': 'activeContext.md'}]},
                        {**projection, 'records': [{'id': str(uuid.uuid4()), 'version': 1, 'path': 'topics/result.md'}]},
                        {**projection, 'records': [{**reference, 'version': True, 'path': 'topics/result.md'}]}]
            for changed in variants:
                r, result = self.check(w, repo, changed)
                self.assertEqual(r.returncode, 1, result)
                self.assertFalse(result['valid'])
                self.assertNotIn('NEVER_ACCEPT_EXTRA_MEMORY', r.stdout)

    def test_canonical_deletion_rejects_older_projection_and_rebuild_cannot_recreate_record(self):
        with World() as w:
            repo, bank, project, reference = self.fixture(w)
            projection = self.build(w, repo, project, [reference])['projection']
            (bank / 'topics/result.md').unlink()
            (bank / 'topics').rmdir()
            router = bank / 'MEMORY.md'
            router.write_text(''.join(line for line in router.read_text().splitlines(keepends=True) if 'topics/result.md' not in line))
            terminal = {'id': reference['id'], 'scope': project, 'deleted-at': '2026-10-22T18:00:00Z',
                        'deletion-authority': str(uuid.uuid4()), 'reason-class': 'user-request'}
            tomb = self.good(w, 'tombstones.py', 'render', '--input', self.save(w, terminal))
            target = bank / tomb['path']
            target.parent.mkdir(parents=True)
            target.write_text(tomb['content'])
            row = '| [' + tomb['path'] + '](<' + tomb['path'] + '>) | Deletion guard | repository | deleted | Before rebuild |\n'
            router.write_text(router.read_text().replace('\n## Omitted contexts', row + '\n## Omitted contexts'))
            r, result = self.check(w, repo, projection)
            self.assertEqual(r.returncode, 1, result)
            self.assertNotIn(projection['source_hashes']['topics/result.md'], r.stdout)
            rebuilt = self.build(w, repo, project, [{**reference, 'body': 'DELETED_CACHED_BODY'}])['projection']
            self.assertEqual(rebuilt['records'], [])
            self.assertNotIn('topics/result.md', rebuilt['source_hashes'])
            self.assertNotIn('DELETED_CACHED_BODY', json.dumps(rebuilt))

    def test_same_bank_projection_checks_in_explicitly_linked_clone_without_machine_or_developer_namespace(self):
        with World() as w:
            repo, _, project, reference = self.fixture(w)
            projection = self.build(w, repo, project, [reference])['projection']
            clone = w.clone(repo, 'independent clone')
            w.git('remote', 'remove', 'origin', cwd=clone)
            dev = self.good(w, 'developer_identity.py', 'preview', '--create', '--label', 'Second disposable developer')
            self.good(w, 'developer_identity.py', 'apply', '--proposal', self.save(w, dev))
            link = self.good(w, 'scope_binding.py', 'preview', '--repo', clone, '--action', 'link', '--reason', 'Explicit same-project clone')
            self.good(w, 'scope_binding.py', 'apply', '--proposal', self.save(w, link))
            before = self.code(w, clone)
            r, result = self.check(w, clone, projection)
            self.assertEqual(r.returncode, 0, result)
            self.assertTrue(result['valid'])
            self.assertEqual(self.code(w, clone), before)

    def test_observed_write_conflict_replay_and_projection_freshness_preserve_later_corrections(self):
        with World() as w:
            repo, bank, project, reference = self.fixture(w)
            code = self.code(w, repo)
            projection = self.build(w, repo, project, [reference])['projection']
            content = w.inside('replacement.md')
            content.write_text('# Active context\n\nReviewed new investigation step.\n')
            proposal = self.good(w, 'bank_writes.py', 'preview', '--repo', repo,
                                 '--path', 'activeContext.md', '--content', content)
            other = bank / 'projectbrief.md'
            other.write_text(other.read_text() + '\nManual evidence that must survive.\n')
            r, result = self.command(w, 'bank_writes.py', 'apply', '--proposal', self.save(w, proposal))
            self.assertEqual(r.returncode, 1, result)
            self.assertNotIn('Reviewed new investigation step', (bank / 'activeContext.md').read_text())
            fresh = self.good(w, 'bank_writes.py', 'preview', '--repo', repo,
                              '--path', 'activeContext.md', '--content', content)
            done = self.good(w, 'bank_writes.py', 'apply', '--proposal', self.save(w, fresh))
            self.assertTrue(done['verified'])
            self.assertEqual(self.check(w, repo, projection)[0].returncode, 1)
            (bank / 'activeContext.md').write_text('# Active context\n\nA later correction must survive replay.\n')
            replay = self.good(w, 'bank_writes.py', 'apply', '--proposal', self.save(w, fresh))
            self.assertTrue(replay['already_applied'])
            self.assertFalse(replay['still_current'])
            self.assertIn('later correction', (bank / 'activeContext.md').read_text())
            self.assertIn('Manual evidence', other.read_text())
            self.assertEqual(self.code(w, repo), code)

    def test_bounded_and_deeply_malformed_input_refuses_without_echo_or_canonical_change(self):
        with World() as w:
            repo, bank, _, _ = self.fixture(w)
            before, code = self.state(bank), self.code(w, repo)
            path = w.inside('malformed-projection.json')
            for operation, raw in [('build', 'x' * 65537), ('check', 'x' * (2 * 1024 * 1024 + 1)),
                                   ('check', '[' * 2000 + '"SECRET_UNTRUSTED_NESTED_INPUT"' + ']' * 2000)]:
                path.write_text(raw)
                r, result = self.command(w, 'projection.py', operation, '--repo', repo, '--input', path)
                self.assertEqual(r.returncode, 1, result)
                self.assertFalse(result['valid'])
                self.assertNotIn('SECRET_UNTRUSTED', r.stdout + r.stderr)
            self.assertEqual(self.state(bank), before)
            self.assertEqual(self.code(w, repo), code)

    def test_interrupted_multi_file_write_is_detected_and_original_recovery_precedes_projection_rebuild(self):
        with World() as w:
            repo, bank, project, reference = self.fixture(w)
            code = self.code(w, repo)
            files = {name: raw.decode() for name, raw in self.state(bank).items()}
            files['activeContext.md'] += '\nA reviewed current step.\n'
            files['projectbrief.md'] += '\nA reviewed boundary condition.\n'
            p = self.good(w, 'bank_transactions.py', 'preview', '--repo', repo,
                          '--files', self.save(w, files), '--operation', 'replace')
            r, _ = self.command(w, 'bank_transactions.py', 'apply', '--proposal', self.save(w, p),
                env={'AGENT_MEMORY_FAULT_STAGE': 'transaction-after-write:activeContext.md', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(r.returncode, 86)
            self.assertTrue((bank / '.agent-memory-transaction.md').exists())
            r, _ = self.command(w, 'projection.py', 'build', '--repo', repo,
                                '--input', self.save(w, {'project_id': project, 'records': [reference]}))
            self.assertEqual(r.returncode, 1)
            unlock = self.good(w, 'bank_writes.py', 'recover', '--repo', repo)
            self.good(w, 'bank_writes.py', 'recover', '--repo', repo, '--proposal', self.save(w, unlock))
            recovery = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'finish')
            done = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--proposal', self.save(w, recovery))
            self.assertTrue(done['verified'])
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            self.assertEqual({k: v.decode() for k, v in self.state(bank).items()}, files)
            projected = self.build(w, repo, project, [reference])['projection']
            self.assertEqual(self.check(w, repo, projected)[0].returncode, 0)
            self.assertEqual(self.code(w, repo), code)


if __name__ == '__main__':
    unittest.main()
