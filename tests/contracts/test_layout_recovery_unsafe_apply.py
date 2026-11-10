"""Recovery apply must not consume changed unsafe private source evidence."""
import unittest

from world import World
import layout_recovery_fixtures as f


def snapshot(path):
    return {item.relative_to(path).as_posix(): item.read_bytes()
            for item in path.rglob('*') if item.is_file()}


class RecoveryUnsafeApplyContract(unittest.TestCase):
    def test_changed_secret_or_oversized_pending_original_refuses_apply_before_mutation(self):
        secret = 'password=PRIVATE_FRONTIER_SECRET_987654321\n'
        for name, replacement in (('secret', secret.encode()), ('oversized', b'x' * 16385)):
            with self.subTest(case=name), World() as world:
                command, source = f.installed(world)
                repo, bank, native = f.memory(world)
                feature = repo / 'feature.txt'
                feature.write_text('Committed fixture feature\n')
                world.git('add', 'feature.txt', cwd=repo)
                world.git('commit', '-m', 'Synthetic feature base', cwd=repo)
                feature.write_text('Staged fixture feature\n')
                world.git('add', 'feature.txt', cwd=repo)
                feature.write_text('Unstaged fixture feature\n')
                (bank / 'handoff-prompt.md').write_text(f.policy_body(0))
                _, observed = f.inspect(world, command, repo)
                proposal, _ = f.prepare(world, command, source, repo, observed, native)
                directory = f.private_directory(world, bank, observed['operation_id'])
                original = next(row for row in observed['manifest']['sources']
                                if row['path'] == 'handoff-prompt.md')
                private_blob = directory / original['blob']
                private_blob.write_bytes(replacement)
                index = repo / '.git/index'
                index_before = index.read_bytes() if index.exists() else None
                bank_before = snapshot(bank)
                feature_before = feature.read_bytes()
                private_before = snapshot(directory)
                head_before = world.git('rev-parse', 'HEAD', cwd=repo).stdout
                process = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo,
                                     '--proposal', proposal], check=False, timeout=60)
                self.assertNotEqual(process.returncode, 0)
                self.assertNotIn(secret.strip(), process.stdout + process.stderr)
                self.assertEqual(snapshot(bank), bank_before)
                self.assertEqual(snapshot(directory), private_before)
                self.assertEqual(index.read_bytes() if index.exists() else None, index_before)
                self.assertEqual(feature.read_bytes(), feature_before)
                self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head_before)
