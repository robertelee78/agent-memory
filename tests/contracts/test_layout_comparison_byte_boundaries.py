"""Complete public comparison artifacts at their encoded consumer bounds.

Synthetic judgments exercise consumer mechanics, not semantic acceptance.
"""
import copy
import json
from pathlib import Path
import sys
import uuid
from unittest.mock import patch
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

    def test_complete_mixed_64_rows_and_65th_refusal_below_byte_limit(self):
        # Canonical-only rows hit the byte bound before 64. Real private units
        # make the independent row boundary reachable without truncating either.
        scripts = Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'
        with patch.object(sys, 'path', [str(scripts), *sys.path]):
            from agent_memory import bank_contract, bank_layout
        with World() as world:
            command, pack = f.installed(world)
            repo, bank, _ = f.memory(world)
            helper = test_comparison.ComparisonContract('runTest')
            helper.pid = project_id((bank / 'MEMORY.md').read_text())
            helper.actor = bind(world, repo)['developer_id']
            meta, routes, omitted = bank_contract.parse_router((bank / 'MEMORY.md').read_text())
            names = ['p%02d' % number for number in range(50)] + ['z-extra']
            for name in names:
                _, relative = helper.add(bank, name, body='cap')
                metadata, _ = test_comparison.unpack((bank / relative).read_text())
                metadata.update(kind='result', status='expired', scope='task:x',
                                source={'actor': 'f', 'kind': 'f'}, evidence=['e'],
                                **{'state-reason': 'f'})
                for optional in ('admission-reason', 'trust', 'visibility'):
                    metadata.pop(optional, None)
                metadata['origin'].update(harness='f', session='f', **{'code-state': 'unknown'})
                (bank / relative).write_text(test_comparison.render(
                    metadata, 'other' if name == 'z-extra' else 'cap'))
            for number, group in enumerate((names[:26], names[26:])):
                index = 'topics/index%d.md' % number
                children = [{'path': 'topics/' + name + '.md', 'purpose': 'Synthetic historical source',
                             'scope': 'task:x', 'lifecycle': 'expired',
                             'read-when': 'When comparing fixture meaning'} for name in group]
                (bank / index).write_text(bank_layout.index_text(index, {
                    'schema': 'agent-memory/topic-index-v1', 'project-id': helper.pid,
                    'updated': meta['router-updated']}, children))
                routes.append({'path': index, 'purpose': 'Synthetic group', 'scope': 'task:x',
                               'lifecycle': 'active', 'read-when': 'When comparing fixture meaning'})
            (bank / 'MEMORY.md').write_text(bank_contract.router_text(meta, routes, omitted))
            for path in bank_contract.CONTEXTS:
                (bank / path).write_text('cap')
            (bank / 'MEMORY.md').write_text(bank_contract.add_context_routes(
                (bank / 'MEMORY.md').read_text(), list(omitted), meta['router-updated']))
            (repo / 'e').write_text('Synthetic structural fixture, not actual semantic confirmation.\n')
            (repo / 'feature.txt').write_text('Original feature\n')
            world.git('add', '.', cwd=repo)
            world.git('commit', '-m', 'Synthetic mixed comparison boundary', cwd=repo)
            (repo / 'feature.txt').write_text('Staged feature\n')
            world.git('add', 'feature.txt', cwd=repo)
            intended = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            (bank / 'pending-source.md').write_text('p' * 8)
            _, inspected = f.inspect(world, command, repo)
            request = f.review_request(world, command, pack, repo, inspected, intended,
                                       pending_paths=('pending-source.md',))
            pending = next(unit for unit in request['unit_map']['units']
                           if unit['disposition'] == 'pending-admission')
            units = [unit for unit in request['unit_map']['units'] if unit is not pending]
            # Eight disjoint original byte ranges, one physical deletion. This
            # reuses the multi-unit source convention, not eight setup rewrites.
            for number in range(8):
                unit = copy.deepcopy(pending)
                unit.update(unit_id=str(uuid.uuid4()), reason='Unconfirmed')
                unit['source_ranges'][0].update(start=number, end=number + 1)
                units.append(unit)
            request['unit_map']['units'] = units
            request['review']['unit_map_sha256'] = f.sha(f.encoded(request['unit_map']))
            request['review']['unit_decisions'] = [
                {'unit_id': unit['unit_id'], 'disposition': unit['disposition'],
                 'rationale': unit['reason']} for unit in units]
            proposal = world.inside('mixed-proposal.json')
            f.call(world, command, 'prepare', '--repo', repo, '--operation',
                   inspected['operation_id'], '--input', f.save(world, request), '--output', proposal)
            applied = f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            directory = f.private_directory(world, bank, inspected['operation_id'])
            def protected():
                return (self.state(world, repo),
                        {p.relative_to(directory).as_posix(): p.read_bytes()
                         for p in (directory / 'originals').rglob('*') if p.is_file()},
                        (directory / 'ledger/00000000.json').read_bytes(),
                        (directory / 'ledger-head.json').read_bytes())
            before = protected()
            candidate = helper.candidate()
            candidate['searches'] = ['cap', 'absent']
            candidate_file = f.save(world, candidate)
            private = world.inside('mixed-inspect.json')
            result = self.invoke(world, command, 'inspect', '--repo', repo,
                                 '--request', candidate_file, '--private-output', private)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            view = json.loads(private.read_text())
            self.assertEqual(len(view['matches']) + len(view['pending_matches']), 64)
            self.assertEqual({row['unit_id'] for row in view['pending_matches']},
                             set(applied['pending_unit_ids']))
            disclosure = sum(len(f.encoded(row)) for row in view['matches'] + view['pending_matches'])
            self.assertLess(disclosure + max(len(f.encoded(row)) for row in view['matches']), 65536)
            judgment = {'observation': view['observation'],
                'decisions': [{'path': row['path'], 'classification': 'uncertain',
                               'rationale': 'Structural review only, not semantic confirmation.'}
                              for row in view['matches']],
                'pending_decisions': [{'unit_id': row['unit_id'], 'classification': 'uncertain',
                    'rationale': 'Unconfirmed original source remains pending.',
                    'coverage_claim': {'unit_id': row['unit_id'], 'covered_source_ranges': [],
                                      'target_record_refs': [], 'coverage': 'none'}}
                    for row in view['pending_matches']],
                'rationale': 'Complete bounded structural comparison; no admission authority.'}
            proof = {'schema': 'agent-memory/comparison-proof-v2', 'judgment': judgment,
                     'disposition': 'review-required', 'targets': []}
            ledger = json.loads((directory / 'ledger/00000000.json').read_text())
            head = json.loads((directory / 'ledger-head.json').read_text())
            review = {'schema': 'agent-memory/layout-coverage-review-v1',
                'review_id': str(uuid.uuid4()), 'operation_id': inspected['operation_id'],
                'reviewer_developer_id': helper.actor, 'reviewer_harness': 'controlled-reviewer',
                'reviewer_session': 'mixed-' + str(uuid.uuid4()),
                'review_time': inspected['manifest']['inspected_at'],
                'manifest_sha256': ledger['manifest_sha256'], 'unit_map_sha256': ledger['unit_map_sha256'],
                'ledger_head_sha256': head['sha256'], 'comparison_proof_input_sha256': f.sha(f.encoded(proof)),
                'intended_output_sha256': f.sha(f.encoded(intended)),
                'claims': [row['coverage_claim'] for row in judgment['pending_decisions']],
                'rationale': 'Controlled independent structural review; all original units remain pending.',
                'decision': 'approve'}
            reviewed = world.inside('mixed-reviewed.json')
            result = self.invoke(world, command, 'review', '--repo', repo, '--request', candidate_file,
                                 '--judgment', f.save(world, judgment),
                                 '--coverage-review', f.save(world, review), '--private-output', reviewed)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            actual = json.loads(reviewed.read_text())['proof']
            result = self.invoke(world, command, 'check', '--repo', repo, '--request', candidate_file,
                                 '--proof', f.save(world, actual),
                                 '--private-output', world.inside('mixed-checked.json'))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            candidate['searches'] = ['cap', 'other']
            refused = self.invoke(world, command, 'inspect', '--repo', repo,
                                  '--request', f.save(world, candidate),
                                  '--private-output', world.inside('mixed-65.json'))
            self.assertNotEqual(refused.returncode, 0)
            self.assertFalse(json.loads(refused.stdout)['comparison_read_complete'])
            self.assertEqual(protected(), before)


if __name__ == '__main__':
    unittest.main()
