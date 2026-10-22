"""Resolve and repair real per-repository identity through public portable commands."""
import json
from pathlib import Path
import shutil
import sys
import time
import unittest
import uuid

from world import World
from identity_fixtures import bind
from record_fixtures import record_text

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'scripts'
CONTEXTS = ['projectbrief.md', 'productContext.md', 'activeContext.md', 'systemPatterns.md', 'techContext.md', 'progress.md']


class ScopeBindingContract(unittest.TestCase):
    def command(self, w, script, *args, env=None):
        script = Path(script)
        if not script.is_absolute():
            script = SCRIPTS / script
        r = w.run([sys.executable, '-I', script, *args], cwd=w.root, check=False, env=env)
        return r, json.loads(r.stdout) if r.stdout else None

    def save(self, w, value, name='scope-proposal.json'):
        path = w.inside(name)
        path.write_text(json.dumps(value))
        return path

    def good(self, w, script, *args, **kwargs):
        r, value = self.command(w, script, *args, **kwargs)
        self.assertEqual(r.returncode, 0, value or r.stderr)
        return value

    def developer(self, w, label='Fixture developer'):
        p = self.good(w, 'developer_identity.py', 'preview', '--create', '--label', label)
        return self.good(w, 'developer_identity.py', 'apply', '--proposal', self.save(w, p))['developer_id']

    def bank(self, w, repo, identity=None, remote=None):
        p = {'project-id': identity or str(uuid.uuid4()), 'router-updated': '2026-10-22T18:00:00Z',
             'contexts': {'activeContext.md': {'body': '# Focus\n\nSource-backed fixture context.\n',
                'purpose': 'Current focus', 'scope': 'repository', 'lifecycle': 'active', 'read-when': 'When resuming'}},
             'omitted': {name: 'No distinct source evidence supplied' for name in CONTEXTS if name != 'activeContext.md'}}
        if remote:
            p['canonical-remote'] = remote
        files = self.good(w, 'bank_contract.py', 'render', '--input', self.save(w, p))['files']
        bank = repo / 'memory-bank'
        bank.mkdir(exist_ok=True)
        for path, content in files.items():
            (bank / path).write_text(content)
        return bank, p['project-id']

    def preview(self, w, repo, action, **values):
        args = ['preview', '--repo', repo, '--action', action, '--reason', 'Explicit fixture binding decision']
        for key, value in values.items():
            args.extend(['--' + key.replace('_', '-'), value])
        return self.good(w, 'scope_binding.py', *args)

    def apply(self, w, p, **kwargs):
        return self.command(w, 'scope_binding.py', 'apply', '--proposal', self.save(w, p), **kwargs)

    def status(self, w, repo):
        return self.good(w, 'scope_binding.py', 'status', '--repo', repo)

    def canonical(self, bank):
        return {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*.md') if not p.name.startswith('.agent-memory')}

    def test_combined_status_readonly_missing_conflicting_and_known_attribution(self):
        with World() as w:
            repo = w.repository()
            bank, identity = self.bank(w, repo)
            before = self.canonical(bank)
            missing = self.status(w, repo)
            self.assertFalse(missing['resolved'])
            self.assertEqual(missing['project_id'], identity)
            self.assertIsNone(missing['developer_id'])
            self.assertFalse(Path(w.env['AGENT_MEMORY_CONFIG_HOME']).exists())
            self.assertEqual(self.canonical(bank), before)
            a = self.developer(w)
            p = self.preview(w, repo, 'bind')
            self.assertEqual(p['changed_memory_paths'], [])
            self.assertEqual(self.canonical(bank), before)
            self.assertFalse(Path(w.env['AGENT_MEMORY_STATE_HOME']).exists())
            r, done = self.apply(w, p)
            self.assertEqual(r.returncode, 0, done)
            self.assertTrue(done['verified'])
            self.assertEqual(done['status']['developer_id'], a)
            self.assertFalse(done['identity_authenticated'])
            self.assertEqual(self.canonical(bank), before)
            b = self.developer(w, 'Second developer')
            alias = self.good(w, 'developer_identity.py', 'preview', '--adopt', a, '--alias', 'git-author-email=first@example.invalid')
            self.good(w, 'developer_identity.py', 'apply', '--proposal', self.save(w, alias))
            selection = self.good(w, 'developer_identity.py', 'preview', '--select', b)
            self.good(w, 'developer_identity.py', 'apply', '--proposal', self.save(w, selection))
            w.git('config', 'user.email', 'first@example.invalid', cwd=repo)
            conflict = self.status(w, repo)
            self.assertFalse(conflict['resolved'])
            self.assertEqual(conflict['developer_id'], b)
            self.assertEqual(conflict['developer']['attribution_conflicts'][0]['matching_developer_ids'], [a])
            self.assertEqual(self.command(w, 'scope_binding.py', 'require', '--repo', repo)[0].returncode, 1)
            self.assertEqual(self.canonical(bank), before)

    def test_create_has_initialization_only_handoff_without_chicken_and_egg(self):
        with World() as w:
            repo = w.repository()
            self.developer(w)
            p = self.preview(w, repo, 'create')
            self.assertFalse((repo / 'memory-bank').exists())
            r, receipt = self.apply(w, p)
            self.assertEqual(r.returncode, 0, receipt)
            self.assertFalse(receipt['verified'])
            self.assertTrue(receipt['registry_verified'])
            self.assertTrue(receipt['initialization_pending'])
            self.assertTrue(receipt['initialization_context']['initialization_only'])
            self.assertFalse((repo / 'memory-bank').exists())
            self.assertEqual(self.command(w, 'scope_binding.py', 'require', '--repo', repo)[0].returncode, 1)
            context = self.good(w, 'scope_binding.py', 'require', '--repo', repo, '--initialize')['context']
            self.bank(w, repo, context['project_id'])  # Explicit bootstrap fixture; full initializer is another story.
            self.assertTrue(self.status(w, repo)['resolved'])
            self.assertEqual(self.command(w, 'scope_binding.py', 'require', '--repo', repo, '--initialize')[0].returncode, 1)
            r, replay = self.apply(w, p)
            self.assertEqual(r.returncode, 0, replay)
            self.assertTrue(replay['verified'])
            self.assertFalse(replay['initialization_pending'])

    def test_rebind_previews_bank_change_reads_both_stores_and_preserves_aliases_and_code(self):
        with World() as w:
            repo = w.repository()
            w.git('remote', 'add', 'origin', 'git@github.com:old/project.git', cwd=repo)
            bank, identity = self.bank(w, repo, remote='repo://github.com/old/project')
            bind(w, repo)
            (repo / 'feature.txt').write_text('Unfinished feature\n')
            w.git('add', 'feature.txt', cwd=repo)
            index, head = (repo / '.git/index').read_bytes(), (repo / '.git/HEAD').read_bytes()
            w.git('remote', 'set-url', 'origin', 'https://user:secret@github.com/new/project.git?secret=secret', cwd=repo)
            self.assertFalse(self.status(w, repo)['resolved'])
            before = self.canonical(bank)
            p = self.preview(w, repo, 'rebind')
            self.assertEqual(p['changed_memory_paths'], ['memory-bank/MEMORY.md'])
            self.assertEqual(before, self.canonical(bank))
            self.assertNotIn('secret', json.dumps(p))
            r, done = self.apply(w, p)
            self.assertEqual(r.returncode, 0, done)
            self.assertTrue(done['verified'])
            self.assertEqual(done['project_id'], identity)
            self.assertEqual(done['status']['project']['project_entry']['aliases'], ['repo://github.com/old/project'])
            self.assertEqual((bank / 'activeContext.md').read_bytes(), before['activeContext.md'])
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual((repo / '.git/HEAD').read_bytes(), head)
            self.assertEqual((repo / 'feature.txt').read_text(), 'Unfinished feature\n')

    def test_fork_changes_every_namespace_and_preserves_record_provenance_conditions_and_tombstone(self):
        with World() as w:
            repo = w.repository()
            bank, identity = self.bank(w, repo)
            bind(w, repo)
            record = record_text('# Lesson\n\nConditional source-backed observation.\n', identity, 'branch:feature')
            (bank / 'topics').mkdir()
            (bank / 'topics/lesson.md').write_text(record)
            index = self.good(w, 'bank_layout.py', 'render-index', '--input', self.save(w, {
                'path': 'topics/index.md', 'project-id': identity, 'updated': '2026-10-22T18:00:00Z',
                'routes': [{'path': 'topics/lesson.md', 'purpose': 'Conditional lesson', 'scope': 'branch:feature',
                            'lifecycle': 'active', 'read-when': 'When on feature'}]}))['content']
            (bank / 'topics/index.md').write_text(index)
            tomb_id = str(uuid.uuid4())
            tomb = {'id': tomb_id, 'scope': identity, 'deleted-at': '2026-10-22T18:00:00Z',
                    'deletion-authority': str(uuid.uuid4()), 'reason-class': 'sensitive'}
            tomb_path = 'history/tombstones/' + tomb_id + '.md'
            (bank / 'history/tombstones').mkdir(parents=True)
            (bank / tomb_path).write_text('---\n' + ''.join(f'{k}: {json.dumps(v)}\n' for k, v in tomb.items()) + '---\n')
            router = (bank / 'MEMORY.md').read_text().replace('\n## Omitted contexts\n',
                '\n| [topics/index.md](topics/index.md) | Lessons | repository | active | When recalling |\n'
                f'| [{tomb_path}]({tomb_path}) | Deletion | repository | deleted | During reconciliation |\n\n## Omitted contexts\n')
            (bank / 'MEMORY.md').write_text(router)
            self.good(w, 'bank_contract.py', 'validate', '--repo', repo)
            w.git('add', '.', cwd=repo)
            w.git('commit', '-m', 'Conditional lesson and deletion fixture', cwd=repo)
            other = w.clone(repo, 'copy')
            w.git('remote', 'remove', 'origin', cwd=other)
            self.assertFalse(self.status(w, other)['resolved'])
            p = self.preview(w, other, 'fork')
            r, done = self.apply(w, p)
            self.assertEqual(r.returncode, 0, done)
            self.assertTrue(done['verified'])
            target = done['project_id']
            self.assertNotEqual(target, identity)
            forked = other / 'memory-bank'
            self.assertEqual((forked / 'topics/lesson.md').read_text(), record.replace(json.dumps(identity), json.dumps(target)))
            self.assertEqual((forked / 'topics/index.md').read_text(), index.replace(json.dumps(identity), json.dumps(target)))
            self.assertEqual((forked / tomb_path).read_text(), (bank / tomb_path).read_text().replace(json.dumps(identity), json.dumps(target)))
            self.assertEqual((forked / 'activeContext.md').read_bytes(), (bank / 'activeContext.md').read_bytes())
            self.assertTrue(self.status(w, repo)['resolved'])
            self.assertTrue(self.status(w, other)['resolved'])
            self.good(w, 'bank_contract.py', 'validate', '--repo', other)

    def test_stale_developer_registry_bank_and_remote_proposals_never_overwrite(self):
        with World() as w:
            repo = w.repository()
            bank, _ = self.bank(w, repo)
            self.developer(w)
            p = self.preview(w, repo, 'bind')
            self.developer(w, 'Different current actor')
            before = self.canonical(bank)
            self.assertEqual(self.apply(w, p)[0].returncode, 1)
            self.assertFalse((Path(w.env['AGENT_MEMORY_CONFIG_HOME']) / 'projects.json').exists())
            p = self.preview(w, repo, 'bind')
            (bank / 'activeContext.md').write_text('# Focus\n\nNewer external evidence.\n')
            self.assertEqual(self.apply(w, p)[0].returncode, 1)
            # A config-only bind must also bind the complete preview read set.
            self.assertEqual((bank / 'activeContext.md').read_text(), '# Focus\n\nNewer external evidence.\n')
            self.assertNotEqual(self.canonical(bank), before)
            p = self.preview(w, repo, 'bind')
            w.git('remote', 'add', 'origin', 'https://github.com/owner/project.git', cwd=repo)
            self.assertEqual(self.apply(w, p)[0].returncode, 1)
            self.assertFalse((Path(w.env['AGENT_MEMORY_CONFIG_HOME']) / 'projects.json').exists())
            p = self.preview(w, repo, 'bind')
            registry_only = self.good(w, 'project_identity.py', 'preview', '--repo', repo, '--action', 'bind', '--reason', 'Another explicit registry decision')
            self.good(w, 'project_identity.py', 'apply', '--proposal', self.save(w, registry_only))
            config = Path(w.env['AGENT_MEMORY_CONFIG_HOME']) / 'projects.json'
            newer = config.read_bytes()
            self.assertEqual(self.apply(w, p)[0].returncode, 1)
            self.assertEqual(config.read_bytes(), newer)
            self.assertEqual((bank / 'activeContext.md').read_text(), '# Focus\n\nNewer external evidence.\n')

    def test_registry_only_interruption_and_torn_bank_resume_with_original_bundle(self):
        for stage in ('binding-after-registry', 'transaction-after-write:MEMORY.md'):
            with self.subTest(stage=stage), World() as w:
                repo = w.repository()
                w.git('remote', 'add', 'origin', 'https://github.com/owner/project.git', cwd=repo)
                bank, _ = self.bank(w, repo)  # Missing shared authority evidence needs a real bank update.
                self.developer(w)
                p = self.preview(w, repo, 'bind')
                r, failed = self.apply(w, p, env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': 'exit'})
                self.assertEqual(r.returncode, 86, failed)
                bundles = list(Path(w.env['AGENT_MEMORY_STATE_HOME']).rglob('binding.json'))
                self.assertEqual(len(bundles), 1)
                self.assertEqual(json.loads(bundles[0].read_bytes()), p['proposal'])
                self.assertFalse(self.status(w, repo)['resolved'])
                if (bank / '.agent-memory.lock').exists():
                    unlock = self.good(w, 'bank_writes.py', 'recover', '--repo', repo)
                    self.good(w, 'bank_writes.py', 'recover', '--repo', repo, '--proposal', self.save(w, unlock))
                r, done = self.apply(w, p)
                self.assertEqual(r.returncode, 0, done)
                self.assertTrue(done['verified'])
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse(bundles[0].exists())
                (bank / 'activeContext.md').write_text('# Focus\n\nA consequential later correction.\n')
                before = self.canonical(bank)
                self.assertEqual(self.apply(w, p)[0].returncode, 1)
                self.assertEqual(self.canonical(bank), before)

    def test_identity_is_pinned_from_preview_and_rechecked_under_the_bank_lock(self):
        with World() as w:
            repo = w.repository()
            bank, _ = self.bank(w, repo)
            bind(w, repo)
            content = w.inside('proposed.md')
            content.write_text('# Focus\n\nCaptured by the original developer.\n')
            p = self.good(w, 'bank_writes.py', 'preview', '--repo', repo, '--path', 'activeContext.md', '--content', content)
            original = self.canonical(bank)
            directory = w.inside('identity-rendezvous')
            stage = 'locked-before-inventory'
            w.env.update(AGENT_MEMORY_FAULT_STAGE=stage, AGENT_MEMORY_FAULT_MODE='pause', AGENT_MEMORY_FAULT_DIR=str(directory))
            child = w.spawn([sys.executable, '-I', SCRIPTS / 'bank_writes.py', 'apply', '--proposal', self.save(w, p)])
            for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):
                w.env.pop(key)
            deadline = time.monotonic() + 5
            while not (directory / (stage + '.ready')).exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue((directory / (stage + '.ready')).exists(), child.poll())
            self.developer(w, 'New explicitly selected developer')
            (directory / (stage + '.release')).touch()
            out, err = child.communicate(timeout=20)
            self.assertFalse(err)
            self.assertEqual(child.returncode, 1, out)
            self.assertIn('identity changed', json.loads(out)['error'])
            self.assertEqual(self.canonical(bank), original)
            self.assertEqual(self.command(w, 'bank_writes.py', 'apply', '--proposal', self.save(w, p))[0].returncode, 1)
            self.assertEqual(self.canonical(bank), original)

    def test_all_public_mediated_writers_refuse_unresolved_identity_and_raw_reads_remain_available(self):
        with World() as w:
            repo = w.repository()
            bank, _ = self.bank(w, repo)
            content = w.inside('replacement.md')
            content.write_text('# Focus\n\nProposed mediated write.\n')
            single = self.good(w, 'bank_writes.py', 'preview', '--repo', repo, '--path', 'activeContext.md', '--content', content)
            files = {k: v.decode() for k, v in self.canonical(bank).items()}
            files['activeContext.md'] = content.read_text()
            tx = self.good(w, 'bank_transactions.py', 'preview', '--repo', repo, '--files', self.save(w, files))
            before = self.canonical(bank)
            for helper, p in (('bank_writes.py', single), ('bank_transactions.py', tx)):
                r, refused = self.command(w, helper, 'apply', '--proposal', self.save(w, p))
                self.assertEqual(r.returncode, 1, refused)
                self.assertEqual(self.canonical(bank), before)
                self.assertFalse((bank / '.agent-memory.lock').exists())
            self.good(w, 'bank_contract.py', 'validate', '--repo', repo)
            self.assertFalse(Path(w.env['AGENT_MEMORY_STATE_HOME']).exists())
            bind(w, repo)
            self.assertEqual(self.command(w, 'bank_writes.py', 'apply', '--proposal', self.save(w, single))[0].returncode, 1)
            single = self.good(w, 'bank_writes.py', 'preview', '--repo', repo, '--path', 'activeContext.md', '--content', content)
            r, receipt = self.command(w, 'bank_writes.py', 'apply', '--proposal', self.save(w, single))
            self.assertEqual(r.returncode, 0, receipt)
            self.assertEqual(receipt['resolved_scope']['project_id'], self.status(w, repo)['project_id'])
            tx['proposal']['operation'] = 'bind'
            self.assertEqual(self.command(w, 'bank_transactions.py', 'apply', '--proposal', self.save(w, tx))[0].returncode, 1)

    def test_portable_status_uses_selected_repo_and_preserves_whitespace_paths_despite_ambient_git(self):
        with World() as w:
            repo = w.repository('repository with trailing space ')
            bank, identity = self.bank(w, repo)
            bind(w, repo)
            w.git('config', 'user.email', 'selected@example.invalid', cwd=repo)
            other = w.repository('different')
            w.git('config', 'user.email', 'wrong@example.invalid', cwd=other)
            pack = w.inside('portable pack')
            shutil.copytree(ROOT / 'skills/agent-memory', pack)
            w.env['GIT_DIR'] = str(other / '.git')
            w.env['GIT_CONFIG_COUNT'] = '1'
            w.env['GIT_CONFIG_KEY_0'] = 'user.email'
            w.env['GIT_CONFIG_VALUE_0'] = 'injected@example.invalid'
            r, current = self.command(w, pack / 'scripts/scope_binding.py', 'status', '--repo', repo, '--harness', 'custom', '--session', 'session-1')
            self.assertEqual(r.returncode, 0, current)
            self.assertTrue(current['resolved'], current)
            self.assertEqual(current['project_id'], identity)
            self.assertEqual(current['project']['evidence']['repo_root'], str(repo))
            observed = current['developer']['observations']
            self.assertIn('selected@example.invalid', [v['value'] for v in observed])
            self.assertNotIn('wrong@example.invalid', [v['value'] for v in observed])
            self.assertNotIn('injected@example.invalid', [v['value'] for v in observed])
            self.assertEqual(current['developer']['session_context'], {'harness': 'custom', 'session': 'session-1'})


if __name__ == '__main__':
    unittest.main()
