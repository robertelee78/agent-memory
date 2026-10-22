"""Public optional-record loading, index bounds and provenance preservation."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World
from record_fixtures import record_text, project_id

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / 'scripts/bank_layout.py'


def metadata(text):
    header, body = text[4:].split('\n---\n', 1)
    return {key: json.loads(value) for key, value in (line.split(': ', 1) for line in header.splitlines())}, body


def content(meta, body):
    return '---\n' + ''.join(f'{key}: {json.dumps(value)}\n' for key, value in meta.items()) + '---\n' + body


class LayoutContract(unittest.TestCase):
    def command(self, w, operation, *, request=None, repo=None, helper=HELPER, **options):
        args = [sys.executable, '-I', helper, operation]
        if request is not None:
            source = w.inside('layout-request.json')
            source.write_text(json.dumps(request))
            args += ['--input', source]
        if repo is not None:
            args += ['--repo', repo]
        for key, value in options.items():
            args += ['--' + key, value]
        p = w.run(args, check=False)
        return p, json.loads(p.stdout)

    def route(self, path, scope='repository', lifecycle='active'):
        return {'path': path, 'purpose': 'Useful fixture context', 'scope': scope,
                'lifecycle': lifecycle, 'read-when': 'When examining the fixture component'}

    def root_route(self, files, route):
        row = '| [' + route['path'] + '](<' + route['path'] + '>) | ' + ' | '.join(route[k] for k in ('purpose', 'scope', 'lifecycle', 'read-when')) + ' |\n'
        files['MEMORY.md'] = files['MEMORY.md'].replace('\n## Omitted contexts', row + '\n## Omitted contexts')

    def fixture(self, w):
        repo = w.repository()
        proposal = json.loads((ROOT / 'skills/agent-memory/templates/router-proposal.example.json').read_bytes())
        proposal['project-id'] = str(uuid.uuid4())
        source = w.inside('router.json')
        source.write_text(json.dumps(proposal))
        p = w.run([sys.executable, '-I', ROOT / 'scripts/bank_contract.py', 'render', '--input', source])
        return repo, json.loads(p.stdout)['files'], proposal['project-id']

    def write(self, repo, files):
        bank = repo / 'memory-bank'
        bank.mkdir(exist_ok=True)
        for path, body in files.items():
            target = bank / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(body)
        return bank

    def index(self, w, path, project, routes):
        p, result = self.command(w, 'render-index', request={'path': path, 'project-id': project,
                                  'updated': '2026-10-22T22:00:00Z', 'routes': routes})
        self.assertEqual(p.returncode, 0, result)
        return result['content']

    def test_all_optional_directories_preserve_record_identity_provenance_conditions_and_nonbinding_procedures(self):
        with World() as w:
            repo, files, project = self.fixture(w)
            paths = [('topics/decision.md', 'project-decision', 'active'),
                     ('episodes/retry.md', 'result', 'active'),
                     ('procedures/retry.md', 'procedural-candidate', 'active'),
                     ('history/revoked.md', 'semantic-fact', 'revoked')]
            for path, kind, state in paths:
                files[path] = record_text('# Bounded lesson\n\n[Current authority](../../docs/adr/retry.md).\n',
                                          project, 'component:src/retry', kind=kind, status=state)
                meta, body = metadata(files[path])
                meta['conditions'] = {'branch': 'feature/retry', 'environment': 'fixture', 'prerequisite': None}
                files[path] = content(meta, body)
                self.root_route(files, self.route(path, 'component:src/retry', state))
            bank = self.write(repo, files)
            w.git('add', '.', cwd=repo)
            w.git('commit', '-m', 'Complete typed fixture', cwd=repo)
            (repo / 'unfinished.py').write_text('unfinished = True\n')
            w.git('add', 'unfinished.py', cwd=repo)
            index = (repo / '.git/index').read_bytes()
            p, result = self.command(w, 'inspect', repo=repo)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['project_id'], project)
            self.assertFalse(result['admission_verified'])
            self.assertFalse(result['writes_performed'])
            entries = {e['path']: e for e in result['entries']}
            for path, kind, state in paths:
                meta, _ = metadata(files[path])
                self.assertEqual(entries[path]['id'], meta['id'])
                self.assertEqual(entries[path]['conditions'], meta['conditions'])
                self.assertEqual(entries[path]['kind'], kind)
                self.assertFalse(entries[path]['binding_procedure'])
                self.assertEqual((bank / path).read_text(), files[path])
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertFalse(Path(w.env['AGENT_MEMORY_STATE_HOME']).exists())

    def nested(self, w):
        repo, files, project = self.fixture(w)
        path = 'episodes/retry notes.md'
        meta, body = metadata(record_text('# Conditional result\n\nRetain the original bounded probe.\n', project, 'component:src/retry', kind='result'))
        meta['conditions'] = {'branch': 'feature/retry'}
        files[path] = content(meta, body)
        files['topics/retries/index.md'] = self.index(w, 'topics/retries/index.md', project, [self.route(path, 'component:src/retry')])
        files['topics/index.md'] = self.index(w, 'topics/index.md', project, [self.route('topics/retries/index.md', 'component:src/retry')])
        self.root_route(files, self.route('topics/index.md', 'branch:feature/retry'))
        return repo, files, project, path

    def test_two_nested_indices_resolve_correct_relative_links_and_retain_inherited_conditions_after_clone(self):
        with World() as w:
            repo, files, project, path = self.nested(w)
            self.assertIn('(<../../episodes/retry notes.md>)', files['topics/retries/index.md'])
            bank = self.write(repo, files)
            p, result = self.command(w, 'inspect', repo=repo)
            self.assertEqual(p.returncode, 0, result)
            leaf = next(e for e in result['entries'] if e['path'] == path)
            self.assertEqual(leaf['scope_chain'], ['branch:feature/retry', 'component:src/retry', 'component:src/retry'])
            self.assertEqual(leaf['conditions'], {'branch': 'feature/retry'})
            p, checked = self.command(w, 'validate-index', file=bank / 'topics/retries/index.md', path='topics/retries/index.md')
            self.assertEqual(p.returncode, 0, checked)
            w.git('add', '.', cwd=repo)
            w.git('commit', '-m', 'Bounded nested routes', cwd=repo)
            for name in ('state', 'cache', 'data'):
                shutil.rmtree(w.paths[name])
            clone = w.clone(repo, 'layout-clone')
            p, fresh = self.command(w, 'inspect', repo=clone)
            self.assertEqual(p.returncode, 0, fresh)
            self.assertEqual(fresh['entries'], result['entries'])
            self.assertEqual((clone / 'memory-bank' / path).read_text(), files[path])

    def test_missing_orphan_cyclic_duplicate_and_overdeep_routes_refuse_without_repair(self):
        for case in ('missing', 'orphan', 'cycle', 'duplicate', 'deep'):
            with self.subTest(case=case), World() as w:
                repo, files, project, path = self.nested(w)
                if case == 'missing':
                    del files[path]
                elif case == 'orphan':
                    files['topics/orphan.md'] = record_text('# Not reachable\n', project)
                elif case == 'cycle':
                    files['topics/retries/index.md'] = self.index(w, 'topics/retries/index.md', project, [self.route('topics/index.md')])
                elif case == 'duplicate':
                    self.root_route(files, self.route(path, 'component:src/retry'))
                else:
                    files['topics/third.md'] = self.index(w, 'topics/third.md', project, [self.route(path, 'component:src/retry')])
                    files['topics/retries/index.md'] = self.index(w, 'topics/retries/index.md', project, [self.route('topics/third.md')])
                bank = self.write(repo, files)
                before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*.md')}
                p, output = self.command(w, 'inspect', repo=repo)
                self.assertEqual(p.returncode, 1, output)
                self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*.md')}, before)
                self.assertFalse(Path(w.env['AGENT_MEMORY_STATE_HOME']).exists())

    def test_metadata_project_status_unadmitted_unknown_schema_and_inherited_scope_mismatches_refuse(self):
        for case in ('project', 'status', 'candidate', 'schema', 'plain', 'inheritance', 'procedure'):
            with self.subTest(case=case), World() as w:
                repo, files, project, path = self.nested(w)
                meta, body = metadata(files[path])
                if case == 'project':
                    meta['project-id'] = str(uuid.uuid4())
                elif case == 'status':
                    meta.update(status='revoked', **{'state-reason': 'Not current'})
                elif case == 'candidate':
                    meta['status'] = 'candidate'
                    files['topics/retries/index.md'] = self.index(w, 'topics/retries/index.md', project, [self.route(path, 'component:src/retry', 'candidate')])
                elif case == 'schema':
                    meta['schema'] = 'agent-memory/record-v999'
                elif case == 'inheritance':
                    meta['conditions'] = {}
                elif case == 'procedure':
                    path = 'procedures/retry.md'
                    files.pop('episodes/retry notes.md')
                    files['topics/retries/index.md'] = self.index(w, 'topics/retries/index.md', project, [self.route(path, 'component:src/retry')])
                files[path] = 'SECRET_LAYOUT_CANARY' if case == 'plain' else content(meta, body)
                self.write(repo, files)
                p, output = self.command(w, 'inspect', repo=repo)
                self.assertEqual(p.returncode, 1, output)
                self.assertNotIn('SECRET_LAYOUT_CANARY', p.stdout + p.stderr)

    def test_index_byte_row_and_unicode_framing_limits_are_enforced_before_a_valid_result(self):
        with World() as w:
            project = str(uuid.uuid4())
            base = {'path': 'topics/index.md', 'project-id': project, 'updated': '2026-10-22T22:00:00Z'}
            for separator in ('\u0085', '\u2028', '\u2029', '\t'):
                for field in ('purpose', 'read-when', 'scope', 'path'):
                    route = self.route('topics/example.md')
                    route[field] += separator + 'extra'
                    p, output = self.command(w, 'render-index', request={**base, 'routes': [route]})
                    self.assertEqual(p.returncode, 1, (field, repr(separator), output))
            p, output = self.command(w, 'render-index', request={**base, 'routes': [self.route(f'topics/{n}.md') for n in range(65)]})
            self.assertEqual(p.returncode, 1, output)
            text = self.index(w, base['path'], project, [self.route('topics/example.md')])
            oversized = w.inside('oversized-index.md')
            oversized.write_text(text.replace('project-id: ', 'project-id: ' + ' ' * 17000))
            p, output = self.command(w, 'validate-index', file=oversized, path=base['path'])
            self.assertEqual(p.returncode, 1, output)

    def test_tombstones_and_duplicate_record_versions_cannot_hide_or_recreate_current_content(self):
        for case in ('safe', 'resurrection', 'duplicate', 'nonmarkdown'):
            with self.subTest(case=case), World() as w:
                repo, files, project = self.fixture(w)
                identity = str(uuid.uuid4())
                marker = 'history/tombstones/' + identity + '.md'
                files[marker] = content({'id': identity, 'scope': project, 'deleted-at': '2026-10-22T22:00:00Z',
                                         'deletion-authority': str(uuid.uuid4()), 'reason-class': 'sensitive'}, '')
                self.root_route(files, self.route(marker, lifecycle='deleted'))
                if case in ('resurrection', 'duplicate'):
                    meta, body = metadata(record_text('# SECRET_DELETION_CANARY\n', project))
                    if case == 'resurrection':
                        meta['id'] = identity
                    files['topics/one.md'] = content(meta, body)
                    self.root_route(files, self.route('topics/one.md'))
                    if case == 'duplicate':
                        files['topics/two.md'] = files['topics/one.md']
                        self.root_route(files, self.route('topics/two.md'))
                bank = self.write(repo, files)
                if case == 'nonmarkdown':
                    (bank / 'history/tombstones/hidden.txt').write_text('SECRET_DELETION_CANARY')
                p, output = self.command(w, 'inspect', repo=repo)
                self.assertEqual(p.returncode, 0 if case == 'safe' else 1, output)
                self.assertNotIn('SECRET_DELETION_CANARY', p.stdout + p.stderr)

    def test_source_index_locations_reject_controls_and_preserve_ordinary_unicode_and_spaces(self):
        with World() as w:
            project = str(uuid.uuid4())
            routes = [self.route('episodes/retry.md')]
            good = self.index(w, 'topics/café index.md', project, routes)
            source = w.inside('valid-index.md')
            source.write_text(good)
            for codepoint in (0, 9, 10, 13, 28, 127, 133, 8232, 8233):
                path = 'topics/in' + chr(codepoint) + 'dex.md'
                p, rendered = self.command(w, 'render-index', request={'path': path, 'project-id': project,
                                           'updated': '2026-10-22T22:00:00Z', 'routes': routes})
                self.assertEqual(p.returncode, 1, codepoint)
                self.assertFalse(rendered['valid'])
                if codepoint == 0:
                    # The OS itself rejects NUL in argv; render exercises the
                    # public JSON-input route that can actually carry it.
                    continue
                p, checked = self.command(w, 'validate-index', file=source, path=path)
                self.assertEqual(p.returncode, 1, codepoint)
                self.assertFalse(checked['valid'])
            p, checked = self.command(w, 'validate-index', file=source, path='topics/café index.md')
            self.assertEqual(p.returncode, 0, checked)
            self.assertEqual(source.read_text(), good)

    def test_optional_history_event_is_not_current_guidance_and_duplicate_or_deletion_labels_refuse(self):
        for case in ('safe', 'duplicate', 'deleted', 'body', 'current-only-in-history'):
            with self.subTest(case=case), World() as w:
                repo, files, project = self.fixture(w)
                files['topics/current.md'] = record_text('# Current lesson\n\nComplete without optional history.\n', project)
                current, _ = metadata(files['topics/current.md'])
                self.root_route(files, self.route('topics/current.md'))
                event = {'schema': 'agent-memory/history-event-v1', 'id': str(uuid.uuid4()), 'project-id': project,
                         'kind': 'memory-admitted', 'record': {'id': current['id'], 'version': 1},
                         'actor': 'fixture-observer', 'recorded-at': '2026-10-22T22:00:00Z', 'evidence': ['fixture://admission-review']}
                files['history/event.md'] = content(event, 'SECRET_HISTORY_CANARY' if case == 'body' else '')
                self.root_route(files, self.route('history/event.md', lifecycle='deleted' if case == 'deleted' else 'active'))
                if case == 'duplicate':
                    files['history/copy.md'] = files['history/event.md']
                    self.root_route(files, self.route('history/copy.md'))
                elif case == 'current-only-in-history':
                    files['history/active.md'] = files['topics/current.md']
                    files.pop('topics/current.md')
                    files['MEMORY.md'] = files['MEMORY.md'].replace('topics/current.md', 'history/active.md')
                bank = self.write(repo, files)
                p, result = self.command(w, 'inspect', repo=repo)
                self.assertEqual(p.returncode, 0 if case == 'safe' else 1, result)
                self.assertNotIn('SECRET_HISTORY_CANARY', p.stdout + p.stderr)
                if case == 'safe':
                    entry = next(e for e in result['entries'] if e['path'] == 'history/event.md')
                    self.assertFalse(entry['current_guidance'])
                    # Explicitly remove optional history and its now-obsolete
                    # route; the complete current lesson remains byte-identical.
                    (bank / 'history/event.md').unlink()
                    router = bank / 'MEMORY.md'
                    router.write_text('\n'.join(line for line in router.read_text().split('\n') if '[history/event.md]' not in line))
                    p, after = self.command(w, 'inspect', repo=repo)
                    self.assertEqual(p.returncode, 0, after)
                    self.assertEqual((bank / 'topics/current.md').read_text(), files['topics/current.md'])

    def test_copied_artifact_preserves_index_and_code_and_refuses_busy_or_interrupted_banks(self):
        with World() as w:
            repo, files, project, path = self.nested(w)
            bank = self.write(repo, files)
            pack = w.inside('copied layout pack')
            shutil.copytree(ROOT / 'skills/agent-memory', pack)
            helper = pack / 'scripts/bank_layout.py'
            p, result = self.command(w, 'inspect', repo=repo, helper=helper)
            self.assertEqual(p.returncode, 0, result)
            for name in ('.agent-memory.lock', '.agent-memory-transaction.md'):
                (bank / name).write_text('Preserve unknown interruption evidence\n')
                p, output = self.command(w, 'inspect', repo=repo, helper=helper)
                self.assertEqual(p.returncode, 1, output)
                self.assertEqual((bank / name).read_text(), 'Preserve unknown interruption evidence\n')
                (bank / name).unlink()
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}, files)


if __name__ == '__main__':
    unittest.main()
