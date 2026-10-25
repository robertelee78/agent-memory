"""Installed lifecycle journeys and executable bindings for the full matrix."""
import ast
import json
from pathlib import Path
import shutil
import unittest

from world import World
from contracts import test_installed_layout as layout
from contracts import test_retrieval as retrieval
from runtime_update_fixtures import advance, snapshot, receipt_path

ROOT = Path(__file__).resolve().parents[2]


class PortableLifecycleContract(unittest.TestCase):
    def command(self, world, cli, *args):
        return layout.command(world, [cli, '--json', *args])

    def git_state(self, world, repo):
        index = Path(world.git('rev-parse', '--path-format=absolute', '--git-path', 'index', cwd=repo).stdout.strip())
        return ((repo/'worker.py').read_bytes(), index.read_bytes(),
                world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                world.git('symbolic-ref', 'HEAD', cwd=repo).stdout)

    def test_copy_all_clients_keep_qualified_memory_through_update_optional_state_loss_uninstall_and_reinstall(self):
        with World() as w:
            helper = layout.InstalledLayoutContract(); channel, pack = layout.channel(w)
            repo, memory, cache, indices, projection = helper.project_with_derived_references(w)
            original_bank, original_git = snapshot(memory), self.git_state(w, repo)
            settings = [w.paths['codex']/'AGENTS.md', w.paths['claude']/'CLAUDE.md',
                        w.paths['config']/'opencode/AGENTS.md']
            for path in settings:
                path.parent.mkdir(parents=True, exist_ok=True); path.write_text('Retain original matrix user instructions.\n')
            custom = w.inside('portable custom skills with spaces')
            installed = layout.install(w, channel, 'copy', 'all', [custom]); self.assertTrue(installed['verified'])
            receipt = installed['receipt']; helper.assert_layout(w, receipt)
            cli = Path(receipt['bin_dir'])/'agent-memory'
            stable_receipt = receipt_path(w).read_bytes()
            repeated = self.command(w, cli, 'install'); self.assertFalse(repeated['changed'])
            self.assertEqual(receipt_path(w).read_bytes(), stable_receipt)
            scope = self.command(w, cli, 'identity', 'status', '--repo', repo)
            request = w.inside('qualified-recall.json')
            request.write_text(json.dumps(retrieval.RetrievalContract().request(scope['project_id'],
                query='', paths=['topics/layout.md'])))
            recalled = self.command(w, cli, 'get', '--repo', repo, '--request', request)
            self.assertEqual(len(recalled['results']), 1)
            self.assertTrue(self.command(w, cli, 'doctor', '--repo', repo, '--projection', projection)['ok'])
            revision = advance(w, channel, pack, shim=True)
            before_check = snapshot(w.root)
            checked = self.command(w, cli, 'update', '--check', '--repo', repo)
            self.assertTrue(checked['check_complete']); self.assertTrue(checked['update_available'])
            self.assertFalse(checked['verified']); self.assertEqual(snapshot(w.root), before_check)
            updated = self.command(w, cli, 'update', '--repo', repo)
            self.assertTrue(updated['verified']); self.assertEqual(updated['receipt']['source']['revision'], revision)
            self.assertEqual(updated['receipt']['source']['version'], '1.0.1'); self.assertTrue(updated['restart_clients'])
            helper.assert_layout(w, updated['receipt'])
            self.assertEqual(self.command(w, cli, 'get', '--repo', repo, '--request', request)['results'], recalled['results'])
            shutil.rmtree(cache); shutil.rmtree(indices)
            self.assertEqual(self.command(w, cli, 'get', '--repo', repo, '--request', request)['results'], recalled['results'])
            self.assertEqual(snapshot(memory), original_bank); self.assertEqual(self.git_state(w, repo), original_git)
            removed = self.command(w, cli, 'uninstall'); self.assertTrue(removed['verified']); self.assertFalse(cli.exists())
            # Real file-only resume while the command/managed source are absent.
            self.assertLessEqual((memory/'MEMORY.md').stat().st_size, 16384)
            self.assertIn('topics/layout.md', (memory/'MEMORY.md').read_text())
            self.assertIn('Shared learning remains', (memory/'topics/layout.md').read_text())
            for path in settings: self.assertEqual(path.read_text(), 'Retain original matrix user instructions.\n')
            restored = layout.install(w, channel, 'copy', 'all', [custom]); self.assertTrue(restored['verified'])
            self.assertEqual(restored['receipt']['source']['revision'], revision); helper.assert_layout(w, restored['receipt'])
            self.assertEqual(self.command(w, cli, 'get', '--repo', repo, '--request', request)['results'], recalled['results'])
            before_preview = snapshot(w.root)
            preview = self.command(w, cli, 'purge-data', '--repo', repo, '--path', memory, '--check')
            self.assertTrue(preview['preview']['confirmation']); self.assertEqual(snapshot(w.root), before_preview)
            self.assertTrue(self.command(w, cli, 'uninstall')['verified'])
            self.assertEqual(snapshot(memory), original_bank); self.assertEqual(self.git_state(w, repo), original_git)

    def test_link_custom_only_updates_without_optional_clients_identity_or_a_bank(self):
        with World() as w:
            helper = layout.InstalledLayoutContract(); channel, pack = layout.channel(w)
            defaults = {str(path): snapshot(path) for path in layout.discovery(w).values()}
            custom = [w.inside('first portable custom skills'), w.inside('second portable custom skills')]
            for target in custom:
                neighbor = target/'neighbor'; neighbor.mkdir(parents=True); (neighbor/'SKILL.md').write_text('# Keep neighbor\n')
            neighbors = {str(p/'neighbor'):snapshot(p/'neighbor') for p in custom}
            installed = layout.install(w, channel, 'link', 'none', custom); receipt = installed['receipt']
            self.assertTrue(installed['verified']); helper.assert_layout(w, receipt)
            self.assertEqual({t['path'] for t in receipt['targets'] if t['kind']=='link'},
                             {str(p/'agent-memory') for p in custom})
            self.assertFalse(any(t['kind']=='activation' for t in receipt['targets']))
            cli = Path(receipt['bin_dir'])/'agent-memory'
            self.assertEqual(self.command(w, cli, 'status')['bank']['state'], 'not_located')
            self.assertTrue(self.command(w, cli, 'doctor')['ok'])
            revision = advance(w, channel, pack)
            before = snapshot(w.root); checked = self.command(w, cli, 'update', '--check')
            self.assertTrue(checked['update_available']); self.assertEqual(snapshot(w.root), before)
            updated = self.command(w, cli, 'update'); self.assertTrue(updated['verified'])
            self.assertEqual(updated['receipt']['source']['revision'], revision); helper.assert_layout(w, updated['receipt'])
            repeated = self.command(w, cli, 'update'); self.assertFalse(repeated['changed']); self.assertEqual(repeated['restart_clients'], [])
            self.assertTrue(self.command(w, cli, 'uninstall')['verified'])
            self.assertEqual({str(path):snapshot(path) for path in layout.discovery(w).values()}, defaults)
            self.assertEqual({str(p/'neighbor'):snapshot(p/'neighbor') for p in custom}, neighbors)
            self.assertFalse((w.paths['repo']/'memory-bank').exists())
            self.assertFalse(Path(w.env['AGENT_MEMORY_CONFIG_HOME']).exists())

    def test_complete_approved_matrix_is_bound_to_registered_executable_cases(self):
        matrix = json.loads((ROOT/'tests/lifecycle-matrix.json').read_text())
        expected = {'fresh-repeat','modes','spaces-optional-clients','ownership-backups','sentinel-edits',
                    'update-guards','activation-interrupts','lost-responses','rollback-locks','removal-purge',
                    'bank-preservation','generated-state-loss','client-identity','pack-validation','isolation'}
        self.assertEqual(matrix['schema'], 'agent-memory/lifecycle-test-matrix-v1')
        self.assertEqual({row['requirement'] for row in matrix['requirements']}, expected)
        self.assertEqual(len(matrix['requirements']), len(expected))
        registry = {row['name']:row for row in json.loads((ROOT/'tests/contracts.json').read_text())['suites']}
        methods = {}
        for row in matrix['requirements']:
            self.assertTrue(row['cases'], row['requirement'])
            for case in row['cases']:
                entry = registry[case['suite']]
                self.assertEqual(entry['scope'], 'v0.0.1'); self.assertTrue(entry['path'])
                if entry['path'] not in methods:
                    tree = ast.parse((ROOT/entry['path']).read_text())
                    methods[entry['path']] = {cls.name+'.'+fn.name for cls in tree.body if isinstance(cls, ast.ClassDef)
                                             for fn in cls.body if isinstance(fn, ast.FunctionDef) and fn.name.startswith('test_')}
                self.assertIn(case['test'], methods[entry['path']], case)


if __name__ == '__main__':
    unittest.main()
