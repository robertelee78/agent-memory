"""Sharing immutable Git observations must not cache mutable worktree state."""
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'skills/agent-memory/scripts'))
from agent_memory import repository_config
from agent_memory.frontmatter import ContractError


class ImmutableUpdateInventory(unittest.TestCase):
    def test_shared_head_reuses_tree_but_rechecks_worktree_and_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp) / 'first'
            second = pathlib.Path(tmp) / 'second'
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            def git(*args):
                subprocess.run(['git', '-C', str(root), *args], check=True, capture_output=True)
            git('config', 'user.name', 'Fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            (root / 'README.md').write_text('fixture\n')
            git('add', 'README.md')
            git('commit', '-qm', 'fixture')
            git('worktree', 'add', '-q', '--detach', str(second))
            calls = []
            real = repository_config.git
            def tracked(repo, *args, **kwargs):
                calls.append(args)
                return real(repo, *args, **kwargs)
            cache = {}
            with patch.object(repository_config, 'git', side_effect=tracked):
                self.assertEqual(repository_config.git_roots(root, immutable_cache=cache), set())
                self.assertEqual(repository_config.git_roots(second, immutable_cache=cache), set())
                self.assertEqual(sum(args[0] == 'ls-tree' for args in calls), 1)
                (second / 'MEMORY.md').write_text('---\nschema: "agent-memory/unknown"\n---\n')
                with self.assertRaises(ContractError):
                    repository_config.git_roots(second, immutable_cache=cache)
                subprocess.run(['git', '-C', str(second), 'add', 'MEMORY.md'], check=True)
                (second / 'MEMORY.md').unlink()
                with self.assertRaises(ContractError):
                    repository_config.git_roots(second, immutable_cache=cache)
                (root / 'README.md').write_text('new tree\n')
                git('add', 'README.md')
                git('commit', '-qm', 'new tree')
                self.assertEqual(repository_config.git_roots(root, immutable_cache=cache), set())
                self.assertEqual(sum(args[0] == 'ls-tree' for args in calls), 2)

    def test_redirected_worktree_cannot_reuse_another_object_store(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp) / 'first'
            second = pathlib.Path(tmp) / 'second'
            foreign = pathlib.Path(tmp) / 'foreign'
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            def git(path, *args):
                return subprocess.run(['git', '-C', str(path), *args], check=True,
                                      capture_output=True, text=True).stdout.strip()
            git(root, 'config', 'user.name', 'Fixture')
            git(root, 'config', 'user.email', 'fixture@example.invalid')
            (root / 'README.md').write_text('fixture\n')
            git(root, 'add', 'README.md')
            git(root, 'commit', '-qm', 'fixture')
            oid = git(root, 'rev-parse', 'HEAD')
            git(root, 'worktree', 'add', '-q', '--detach', str(second))
            cache = {'common_dir': (root / '.git').resolve()}
            repository_config.settings(root, immutable_cache=cache)
            subprocess.run(['git', 'init', '-q', str(foreign)], check=True)
            (foreign / '.git' / 'HEAD').write_text(oid + '\n')
            (second / '.git').write_text('gitdir: ' + str(foreign / '.git') + '\n')
            with self.assertRaises(ContractError):
                repository_config.settings(second, immutable_cache=cache)


if __name__ == '__main__':
    unittest.main()
