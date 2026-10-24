"""Separate actor registries/clones and one installed portable public interface."""
from contextlib import contextmanager
import json
from pathlib import Path
import sys
import time

import installed_cli_fixtures as installed
from refresh_fixtures import RefreshFixture
from contracts import test_comparison as comparison
from contracts import test_semantic_sharing as semantic


class MultiMachineFixture(RefreshFixture):
    @contextmanager
    def actor(self, world, name):
        original = world.env.copy(); root = world.inside(name)
        for key, leaf in (('AGENT_MEMORY_CONFIG_HOME','config'), ('AGENT_MEMORY_STATE_HOME','state'),
                          ('AGENT_MEMORY_BACKUP_HOME','backups')):
            path = root/leaf; path.mkdir(parents=True, exist_ok=True, mode=0o700); world.env[key] = str(path)
        try:
            yield
        finally:
            world.env.clear(); world.env.update(original)

    def call(self, world, helper, *args):
        started = time.monotonic()
        self.event('public-command-start', helper=helper, operation=str(args[0]) if args else '')
        try:
            result = installed.invoke(world, self.cli, helper, *args)
        except BaseException as exc:
            self.event('public-command-error', helper=helper, elapsed_seconds=time.monotonic()-started,
                       error=type(exc).__name__)
            raise
        self.event('public-command-end', helper=helper, returncode=result[0].returncode,
                   elapsed_seconds=time.monotonic()-started)
        return result

    def bind_actor(self, world, repo, harness, session, adopt=None):
        current = self.good(world, 'scope_binding.py', 'status', '--repo', repo, '--harness', harness, '--session', session)
        if current['resolved']:
            self.assertEqual(current['developer']['session_context'], {'harness':harness,'session':session})
            return current
        if current['developer']['setup_required']:
            proposal = self.good(world, 'developer_identity.py', 'preview',
                                 *(['--adopt', adopt] if adopt else ['--create']), '--label', 'Isolated matrix developer')
            self.good(world, 'developer_identity.py', 'apply', '--proposal', self.save(world, proposal))
        current = self.good(world, 'scope_binding.py', 'status', '--repo', repo)
        if not current['resolved']:
            proposal = self.good(world, 'scope_binding.py', 'preview', '--repo', repo, '--action', 'bind',
                                 '--reason', 'Explicit same-repository isolated matrix binding')
            self.good(world, 'scope_binding.py', 'apply', '--proposal', self.save(world, proposal))
        current = self.good(world, 'scope_binding.py', 'status', '--repo', repo, '--harness', harness, '--session', session)
        self.assertEqual(current['developer']['session_context'], {'harness':harness,'session':session})
        return current

    def feature(self, world, repo, branch, value):
        if branch: world.git('checkout', '-b', branch, cwd=repo)
        (repo/'code.py').write_text('VALUE = '+str(value)+'  # unfinished feature ancestor\n')
        world.git('add', 'code.py', cwd=repo); world.git('commit', '-m', 'Private fixture feature ancestor', cwd=repo)
        (repo/'code.py').write_text('VALUE = '+str(value+1)+'  # staged unfinished feature\n')
        world.git('add', 'code.py', cwd=repo); (repo/'unrelated.txt').write_text('Keep '+str(value)+' session note.\n')

    def setup_actors(self, world):
        a, bank, seed, base = self.fixture(world)
        self.cli = installed.install(world)
        # Establish shared baseline fixture data before any preservation snapshot.
        world.git('reset', '--hard', base, cwd=a)
        baseline = '# Project\n\n## Build\nUse the isolated build.\n\n## Review\nReview scoped sources.\n'
        (bank/'projectbrief.md').write_text(baseline)
        world.git('add', 'memory-bank', cwd=a); world.git('commit', '-m', 'Shared matrix sections', cwd=a)
        world.git('push', 'origin', 'HEAD:main', cwd=a); world.git('pull', '--ff-only', cwd=seed)
        policy = self.github(world, a, bank, seed); remote = Path(json.loads(policy.read_text())['remote'])
        world.git('fetch', 'origin', cwd=a); world.git('reset', '--hard', 'origin/main', cwd=a)
        authority = world.git('rev-parse', 'HEAD', cwd=a).stdout.strip()
        b = world.clone(remote, 'machine B clone'); c = world.clone(remote, 'machine C clone')
        for repo in (b,c): world.git('remote', 'set-url', 'origin', 'https://github.com/fixture/memory.git', cwd=repo)
        sibling = world.inside('machine B separate worktree')
        world.git('worktree', 'add', '-b', 'B-worktree', sibling, authority, cwd=b)
        for repo, branch, value in ((a,'A-feature',10),(b,'B-feature',20),(c,'C-feature',30),(sibling,None,40)):
            self.feature(world, repo, branch, value)
        actor_a = self.bind_actor(world, a, 'claude', 'A-session')['developer_id']
        with self.actor(world, 'profile-B'):
            actor_b = self.bind_actor(world, b, 'codex', 'B-session')['developer_id']
            same_b = self.bind_actor(world, sibling, 'opencode', 'B-worktree-session')
            self.assertEqual(same_b['developer_id'], actor_b)
        with self.actor(world, 'profile-C'):
            same_a = self.bind_actor(world, c, 'custom', 'C-session', adopt=actor_a)
            self.assertEqual(same_a['developer_id'], actor_a)
        self.assertNotEqual(actor_a, actor_b)
        return a, b, c, sibling, actor_a, actor_b, baseline

    def fact(self, bank, name, body, developer, harness, session, conditions=None):
        metadata, path = semantic.SemanticSharingContract().add(bank, name, body, developer=developer,
                                                               conditions=conditions or {})
        metadata['origin'].update(harness=harness, session=session)
        (bank/path).write_text(comparison.render(metadata, body))
        return metadata, path

    def publication(self, world, repo, decide=None, dependencies=None, consolidate=None):
        view = self.good(world, 'reconcile_memory.py', 'inspect', '--repo', repo)
        judgment = self.judgment(view, decide=decide)
        if dependencies: judgment['dependencies'] = dependencies
        if consolidate: judgment['consolidations'] = consolidate(view)
        self.reviewed = self.good(world, 'reconcile_memory.py', 'review', '--repo', repo,
                                  '--input', self.save(world, judgment))
        return self.good(world, 'publish_memory.py', 'prepare', '--repo', repo,
                         '--input', self.save(world, self.reviewed['proof']))['request']

    def refresh(self, world, repo, recall, decide=None, dependencies=None):
        view = self.good(world, 'refresh_memory.py', 'inspect', '--repo', repo)
        judgment = self.judgment(view['inspection'], decide=decide)
        if dependencies: judgment['dependencies'] = dependencies
        self.reviewed = self.good(world, 'reconcile_memory.py', 'review', '--repo', repo,
                                  '--input', self.save(world, judgment))
        request = {'proof':self.reviewed['proof'],'recall':recall}
        proposal = self.good(world, 'refresh_memory.py', 'prepare', '--repo', repo,
                             '--input', self.save(world, request))['proposal']
        return self.apply_refresh(world, proposal), proposal

    def protected(self, world, repo):
        return self.state(world, repo, repo/'memory-bank')[1:], (repo/'unrelated.txt').read_bytes()

    def event(self, name, **data):
        print(json.dumps({'case':name,**data}, sort_keys=True), file=sys.stderr, flush=True)
