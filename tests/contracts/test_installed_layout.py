"""Integrated installed lifecycle and storage boundaries through real commands."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from identity_fixtures import bind
from record_fixtures import record_text
from runtime_update_fixtures import ROOT, PACK, version, advance, bank, snapshot, receipt_path


def command(world, argv, expected=0):
    result = world.run(argv, check=False, timeout=120)
    if result.returncode != expected:
        raise AssertionError((result.returncode, result.stdout, result.stderr))
    return json.loads(result.stdout)


def channel(world):
    root = world.repository('trusted local channel with spaces')
    pack = root / 'skills/agent-memory'
    shutil.copytree(PACK, pack); version(pack, '1.0.0')
    world.git('add', '.', cwd=root); world.git('commit', '-m', 'Disposable actual runtime', cwd=root)
    return root, pack


def discovery(world):
    return {'agents': world.paths['home'] / '.agents/skills',
            'codex': world.paths['codex'] / 'skills',
            'claude': world.paths['claude'] / 'skills',
            'opencode': world.paths['config'] / 'opencode/skills'}


def install(world, root, mode, presets, custom=()):
    return command(world, [sys.executable, '-I', ROOT / 'scripts/installer.py', 'install',
                          '--remote', root, '--ref', 'main', '--mode', mode, '--presets', presets,
                          *[part for path in custom for part in ('--target-dir', path)]])


class InstalledLayoutContract(unittest.TestCase):
    def assert_layout(self, world, receipt):
        code = Path(receipt['source']['path']); state = receipt_path(world).parent
        self.assertEqual(code, world.paths['data'] / 'agent-memory/source')
        self.assertEqual(receipt['source']['pack'], str(code / 'skills/agent-memory'))
        self.assertEqual(state, world.paths['state'] / 'agent-memory/install')
        self.assertFalse(state.is_relative_to(code)); self.assertFalse(code.is_relative_to(state))
        self.assertEqual(receipt['data_roots'], [])
        self.assertEqual(receipt['runtime_state'], str(state.parent))
        self.assertEqual(receipt['bin_dir'], str(world.paths['home'] / '.local/bin'))
        for target in receipt['targets']:
            path = Path(target['path'])
            self.assertFalse(path.is_relative_to(world.paths['repo']))
            self.assertFalse(path.is_relative_to(world.paths['cache']))
            if target['kind'] in {'copy', 'link'}:
                measured = command(world, [sys.executable, '-I', path / 'scripts/artifact.py', 'inspect'])
                self.assertEqual(measured['artifact_digest'], receipt['source']['artifact_digest'])
                if receipt['mode'] == 'link':
                    self.assertEqual(path.resolve(), code / 'skills/agent-memory')
                else:
                    self.assertFalse(path.is_symlink())
        for backup in receipt['backups']:
            path = Path(backup['path'])
            self.assertFalse(path.is_relative_to(code)); self.assertTrue(path.exists())

    def project_with_derived_references(self, world):
        repo = world.repository(); memory = bank(world, repo)
        project = '8e0b66aa-2be5-4fa3-b47f-1245474aa7dd'
        text = record_text('# Layout fixture\n\nShared learning remains in canonical Markdown.\n', project)
        record_id = json.loads(next(line.split(': ', 1)[1] for line in text.splitlines() if line.startswith('id: ')))
        (memory / 'topics').mkdir(); (memory / 'topics/layout.md').write_text(text)
        router = memory / 'MEMORY.md'
        row = '| [topics/layout.md](<topics/layout.md>) | Layout fixture | repository | active | When inspecting storage |\n'
        router.write_text(router.read_text().replace('\n## Omitted contexts', row + '\n## Omitted contexts'))
        bind(world, repo)
        request = world.inside('projection-request.json')
        request.write_text(json.dumps({'project_id': project, 'records': [{'id': record_id, 'version': 1}]}))
        built = command(world, [sys.executable, '-I', ROOT / 'scripts/projection.py', 'build', '--repo', repo, '--input', request])
        self.assertEqual(len(built['projection']['records']), 1)
        self.assertFalse(built['canonical_writes_performed'])
        cache = world.paths['cache'] / 'agent-memory/cache'; cache.mkdir(parents=True)
        # Opaque disposable data checks lifecycle non-ownership, not a cache backend.
        (cache / 'fixture-only.json').write_text('{"fixture_only": true}\n')
        indices = world.paths['cache'] / 'agent-memory/indices'; indices.mkdir()
        projection = indices / 'references.json'; projection.write_text(json.dumps(built['projection']))
        self.assertFalse(projection.is_relative_to(memory)); self.assertFalse(projection.is_relative_to(cache))
        code = repo / 'worker.py'; code.write_text('value = 1\n')
        world.git('add', '.', cwd=repo); world.git('commit', '-m', 'Canonical project and source fixture', cwd=repo)
        code.write_text('value = 2  # staged unrelated feature\n'); world.git('add', 'worker.py', cwd=repo)
        return repo, memory, cache, indices, projection

    def test_all_clients_share_one_artifact_and_installed_lifecycle_preserves_distinct_data(self):
        for mode in ('link', 'copy'):
            with self.subTest(mode=mode), World() as world:
                root, pack = channel(world)
                repo, memory, cache, indices, projection = self.project_with_derived_references(world)
                settings = [world.paths['codex'] / 'AGENTS.md', world.paths['claude'] / 'CLAUDE.md',
                            world.paths['config'] / 'opencode/AGENTS.md']
                for path in settings:
                    path.parent.mkdir(parents=True, exist_ok=True); path.write_text('Keep existing client instructions.\n')
                roots = discovery(world); custom = world.inside('custom harness skills with spaces')
                neighbors = []
                for directory in [*roots.values(), custom]:
                    neighbor = directory / 'neighbor-skill'; neighbor.mkdir(parents=True)
                    (neighbor / 'SKILL.md').write_text('# Unrelated skill\n'); neighbors.append(neighbor)
                protected = [memory, cache, indices, world.paths['config'] / 'agent-memory',
                             world.paths['state'] / 'agent-memory/writes', repo / '.git', repo / 'worker.py', *neighbors]
                self.assertTrue(all(p.exists() for p in protected))
                before = {str(p): snapshot(p) for p in protected}
                result = install(world, root, mode, 'all', [custom]); self.assertTrue(result['verified'])
                receipt = result['receipt']; self.assert_layout(world, receipt)
                actual = {Path(t['path']) for t in receipt['targets'] if t['kind'] in {'copy', 'link'}}
                self.assertEqual(actual, {p / 'agent-memory' for p in [*roots.values(), custom]})
                self.assertTrue({'agents', 'codex', 'claude', 'opencode'} <= set(receipt['enabled_adapters']))
                self.assertEqual(len(set(receipt['enabled_adapters'])), 5)
                cli = Path(receipt['bin_dir']) / 'agent-memory'
                for name in ('install', 'uninstall', 'update', 'version', 'doctor', 'status'):
                    help_result = world.run([cli, '--json', name, '--help'])
                    self.assertEqual(help_result.returncode, 0)
                for argv in ([cli, '--json', 'install', '--check'], [cli, '--json', 'update', '--check'],
                             [cli, '--json', 'uninstall', '--check'], [cli, 'version', '--json'],
                             [cli, 'status', '--repo', repo, '--json'],
                             [cli, 'doctor', '--repo', repo, '--projection', projection, '--json']):
                    prior = snapshot(world.root); command(world, argv)
                    self.assertEqual(snapshot(world.root), prior)
                self.assertFalse(command(world, [cli, '--json', 'install'])['changed'])
                self.assertFalse(command(world, [cli, '--json', 'update'])['changed'])
                revision = advance(world, root, pack)
                updated = command(world, [cli, '--json', 'update']); self.assertTrue(updated['verified'])
                receipt = updated['receipt']; self.assertEqual(receipt['source']['revision'], revision)
                self.assert_layout(world, receipt)
                self.assertTrue(receipt['backups'])
                retained_backups = {p['path']: snapshot(Path(p['path'])) for p in receipt['backups']}
                self.assertEqual({str(p): snapshot(p) for p in protected}, before)
                removed = command(world, [cli, '--json', 'uninstall']); self.assertTrue(removed['verified'])
                self.assertFalse(removed['preserved']); self.assertFalse(cli.exists())
                self.assertFalse(Path(receipt['source']['path']).exists())
                self.assertTrue(receipt_path(world).is_file()); self.assertTrue(receipt_path(world).parent.joinpath('journal.json').is_file())
                self.assertTrue(json.loads(receipt_path(world).read_text())['uninstalled'])
                self.assertEqual({p: snapshot(Path(p)) for p in retained_backups}, retained_backups)
                self.assertEqual({str(p): snapshot(p) for p in protected}, before)
                for path in settings:
                    self.assertEqual(path.read_text(), 'Keep existing client instructions.\n')
                # Derived file loss leaves native file-only bank inspection useful.
                shutil.rmtree(cache); shutil.rmtree(indices)
                current = command(world, [sys.executable, '-I', ROOT / 'scripts/bank_state.py', 'inspect', '--repo', repo])
                self.assertEqual(current['project_id'], '8e0b66aa-2be5-4fa3-b47f-1245474aa7dd')
                self.assertEqual(snapshot(memory), before[str(memory)])
                restored = install(world, root, mode, 'all', [custom]); self.assertTrue(restored['verified'])
                self.assert_layout(world, restored['receipt'])
                self.assertEqual(snapshot(memory), before[str(memory)])

    def test_custom_only_discovery_does_not_create_default_client_links_or_guidance(self):
        for mode in ('link', 'copy'):
            with self.subTest(mode=mode), World() as world:
                root, _ = channel(world); defaults = {str(p): snapshot(p) for p in discovery(world).values()}
                custom = [world.inside('first custom skills'), world.inside('second custom skills')]
                result = install(world, root, mode, 'none', custom); receipt = result['receipt']
                self.assert_layout(world, receipt)
                targets = [t for t in receipt['targets'] if t['kind'] in {'link', 'copy'}]
                self.assertEqual({t['path'] for t in targets}, {str(p / 'agent-memory') for p in custom})
                self.assertFalse(any(t['kind'] == 'activation' for t in receipt['targets']))
                self.assertEqual({str(p): snapshot(p) for p in discovery(world).values()}, defaults)
                value = command(world, [Path(receipt['bin_dir']) / 'agent-memory', '--json', 'version'])
                self.assertEqual(value['installation']['enabled_adapters'], receipt['enabled_adapters'])
                self.assertEqual(len(set(value['installation']['enabled_adapters'])), 2)

    def test_no_client_mode_still_exposes_installed_lifecycle_without_a_bank_or_identity(self):
        for mode in ('link', 'copy'):
            with self.subTest(mode=mode), World() as world:
                root, _ = channel(world); before = {str(p): snapshot(p) for p in discovery(world).values()}
                receipt = install(world, root, mode, 'none')['receipt']; self.assert_layout(world, receipt)
                self.assertEqual(receipt['enabled_adapters'], [])
                self.assertFalse(any(t['kind'] in {'link', 'copy', 'activation'} for t in receipt['targets']))
                cli = Path(receipt['bin_dir']) / 'agent-memory'
                self.assertEqual(command(world, [cli, '--json', 'status'])['bank']['state'], 'not_located')
                self.assertTrue(command(world, [cli, '--json', 'doctor'])['ok'])
                self.assertFalse(command(world, [cli, '--json', 'update'])['changed'])
                self.assertEqual({str(p): snapshot(p) for p in discovery(world).values()}, before)
                self.assertFalse((world.paths['repo'] / 'memory-bank').exists())
                self.assertFalse(Path(world.env['AGENT_MEMORY_CONFIG_HOME']).exists())
                self.assertTrue(command(world, [cli, '--json', 'uninstall'])['verified'])
                self.assertTrue(receipt_path(world).exists())
