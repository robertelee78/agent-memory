"""Complete public comparison artifacts at their encoded consumer bounds.

Synthetic judgments exercise consumer mechanics, not semantic acceptance.
"""
import copy
import json
import unittest

from contracts import test_comparison
from identity_fixtures import bind
from record_fixtures import project_id
from world import World
import layout_recovery_fixtures as f


class LayoutComparisonByteBoundaries(unittest.TestCase):
    def fixture(self, world):
        command, _ = f.installed(world)
        repo, bank, _ = f.memory(world)
        helper = test_comparison.ComparisonContract('runTest')
        helper.pid = project_id((bank / 'MEMORY.md').read_text())
        helper.actor = bind(world, repo)['developer_id']
        for number in range(40):
            helper.add(bank, 'policy-%02d' % number,
                       body='# Attempt cap\n\nFor synthetic environment %02d retain attempt cap evidence before retrying.\n' % number)
        (repo / 'feature.txt').write_text('Original feature\n')
        world.git('add', '.', cwd=repo)
        world.git('commit', '-m', 'Synthetic baseline', cwd=repo)
        (repo / 'feature.txt').write_text('Staged feature\n')
        world.git('add', 'feature.txt', cwd=repo)
        request = f.save(world, helper.candidate())
        return command, repo, bank, helper, request

    def state(self, world, repo):
        return ({p.relative_to(repo).as_posix(): p.read_bytes() for p in repo.rglob('*')
                 if p.is_file() and '.git' not in p.parts},
                (repo / '.git/index').read_bytes(),
                world.git('rev-parse', 'HEAD', cwd=repo).stdout)

    def invoke(self, world, command, *args):
        return world.run([command, 'compare', *args], check=False, timeout=60)

    def test_disclosure_exact_encoded_limit_and_one_byte_over_preserve_sources(self):
        with World() as world:
            command, repo, bank, _, request = self.fixture(world)
            args = ('inspect', '--repo', repo, '--request', request)
            first = self.invoke(world, command, *args)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            rows = json.loads(first.stdout)['matches']
            self.assertEqual(len(rows), 40)
            delta = 65536 - sum(len(f.encoded(row)) for row in rows)
            fragment = 'é "'
            extra = len(f.encoded(fragment)) - len(f.encoded(''))
            self.assertGreater(delta, extra)
            target = bank / 'topics/policy-00.md'
            original = target.read_text()
            target.write_text(original + fragment + 'x' * (delta - extra))
            exact = self.invoke(world, command, *args)
            self.assertEqual(exact.returncode, 0, exact.stdout + exact.stderr)
            rows = json.loads(exact.stdout)['matches']
            self.assertEqual(sum(len(f.encoded(row)) for row in rows), 65536)
            target.write_text(target.read_text() + 'x')
            before = self.state(world, repo)
            refused = self.invoke(world, command, *args)
            self.assertNotEqual(refused.returncode, 0)
            self.assertFalse(json.loads(refused.stdout)['comparison_read_complete'])
            self.assertEqual(before, self.state(world, repo))

    def test_complete_proof_exact_encoded_limit_and_one_byte_over(self):
        with World() as world:
            command, repo, _, helper, request = self.fixture(world)
            before = self.state(world, repo)
            inspected = self.invoke(world, command, 'inspect', '--repo', repo, '--request', request)
            self.assertEqual(inspected.returncode, 0, inspected.stdout + inspected.stderr)
            view = json.loads(inspected.stdout)
            self.assertEqual(len(view['matches']), 40)
            judgment = helper.judgment(view, 'distinct-knowledge')
            for decision in judgment['decisions']:
                decision['rationale'] = 'Controlled context review: é " ' + 'x' * 1000
            proof = {'schema': 'agent-memory/comparison-proof-v2', 'judgment': judgment,
                     'disposition': 'create-distinct', 'targets': []}
            remaining = 65536 - len(f.encoded(proof))
            self.assertGreater(remaining, 0)
            for decision in judgment['decisions']:
                count = min(remaining, 2048 - len(decision['rationale'].encode()))
                decision['rationale'] += 'x' * count
                remaining -= count
            self.assertEqual(remaining, 0)
            self.assertEqual(len(f.encoded(proof)), 65536)
            reviewed = self.invoke(world, command, 'review', '--repo', repo, '--request', request,
                                   '--judgment', f.save(world, judgment))
            self.assertEqual(reviewed.returncode, 0, reviewed.stdout + reviewed.stderr)
            actual = json.loads(reviewed.stdout)['proof']
            self.assertEqual(actual, proof)
            checked = self.invoke(world, command, 'check', '--repo', repo, '--request', request,
                                  '--proof', f.save(world, actual))
            self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
            oversized = copy.deepcopy(actual)
            oversized['judgment']['decisions'][-1]['rationale'] += 'x'
            self.assertLessEqual(len(oversized['judgment']['decisions'][-1]['rationale'].encode()), 2048)
            self.assertEqual(len(f.encoded(oversized)), 65537)
            refused = self.invoke(world, command, 'check', '--repo', repo, '--request', request,
                                  '--proof', f.save(world, oversized))
            self.assertNotEqual(refused.returncode, 0)
            self.assertFalse(json.loads(refused.stdout)['comparison_read_complete'])
            self.assertEqual(before, self.state(world, repo))


if __name__ == '__main__':
    unittest.main()
