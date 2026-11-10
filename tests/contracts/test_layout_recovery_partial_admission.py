"""Reported-size recovery retains every uncredited source and its provenance."""
import json
import unittest

from world import World
import layout_recovery_fixtures as f
from contracts.test_admission import AdmissionContract
from contracts.test_comparison import unpack


class PartialRecoveryAdmissionContract(unittest.TestCase):
    def test_oversized_bank_refuses_ordinary_read_and_capture_without_mutation(self):
        with World() as world:
            command, _ = f.installed(world)
            repo, bank, _ = f.memory(world, oversized=True, handoff=True)
            _, inspected = f.inspect(world, command, repo)
            identity = inspected['manifest']['identity']['scope_context']
            helper = AdmissionContract('runTest')
            helper.pid, helper.actor = identity['project_id'], identity['developer_id']
            capture = helper.request('verified-repository', 'durable-discovery')
            capture['repository_evidence'] = {'path': 'synthetic-policy.md',
                                              'sha256': f.sha((repo / 'synthetic-policy.md').read_bytes())}
            capture['candidate']['metadata']['evidence'] = ['synthetic-policy.md']
            request = {'project_id': identity['project_id'], 'query': '', 'ids': [], 'paths': [],
                       'context': {}, 'kinds': [], 'minimum_trust': 'reviewed', 'limit': 8,
                       'include_context': True}
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            index = (repo / '.git/index').read_bytes()
            for args in (('writes', 'inspect', '--repo', repo),
                         ('search', '--repo', repo, '--request', f.save(world, request)),
                         ('capture', '--repo', repo, '--request', f.save(world, capture))):
                result = world.run([command, *args], check=False, timeout=60,
                                   env={'AGENT_MEMORY_NO_RECALL_CATALOG': '1'})
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes()
                                  for p in bank.rglob('*') if p.is_file()}, before)
                self.assertEqual((repo / '.git/index').read_bytes(), index)

    def test_one_of_eight_is_admitted_without_consuming_the_other_sources(self):
        with World() as world:
            command, source = f.installed(world)
            repo, bank, native = f.memory(world, oversized=True, handoff=True)
            (repo / 'synthetic-policy.md').write_text('# Independent synthetic source\n\n' + '\n\n'.join(f.POLICIES) + '\n')
            originals = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            self.assertEqual(len(originals['handoff-prompt.md']), 28478)
            _, inspected = f.inspect(world, command, repo)
            proposal, _ = f.prepare(world, command, source, repo, inspected, native)
            applied = f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            pending = set(applied['pending_unit_ids'])
            self.assertEqual(len(pending), 8)
            directory = f.private_directory(world, bank, inspected['operation_id'])
            initial = json.loads((directory / 'ledger/00000000.json').read_text())
            initial_units = {unit['unit_id']: unit for unit in initial['units']}
            canonical = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                         if p.is_file() and not p.name.startswith('.agent-memory')}
            admission, transaction, target = f.admission_proposal(world, command, source, repo, bank,
                                                                  inspected, full=True, policy_number=0)
            decisions = transaction['admission_evidence']['comparison_proof']['judgment']['pending_decisions']
            credited = {row['unit_id'] for row in decisions if row['coverage_claim']['coverage'] == 'full'}
            self.assertEqual(len(credited), 1)
            f.public(world, command, 'admission', 'apply', '--proposal', admission)
            status = f.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
            self.assertEqual(set(status['pending_unit_ids']), pending - credited)
            self.assertFalse(status['semantic_recovery_complete'])
            head = json.loads((directory / 'ledger-head.json').read_text())
            ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
            for unit in ledger['units']:
                previous = initial_units[unit['unit_id']]
                self.assertEqual(unit['source_ranges'], previous['source_ranges'])
                self.assertEqual(unit['provenance'], previous['provenance'])
                if unit['unit_id'] in credited:
                    self.assertEqual(unit['state'], 'admitted')
                else:
                    self.assertEqual(unit['state'], previous['state'])
            for item in inspected['manifest']['sources']:
                self.assertEqual((directory / item['blob']).read_bytes(), originals[item['path']])
            for path, content in canonical.items():
                if path != 'MEMORY.md':
                    self.assertEqual((bank / path).read_bytes(), content)
            metadata, body = unpack((bank / target).read_text())
            self.assertIn(f.POLICIES[0], body)
            self.assertEqual(metadata['origin'], transaction['admission_evidence']['capture']['request']['candidate']['metadata']['origin'])
            # A completed operation ID cannot authorize a different output.
            changed = json.loads(admission.read_text())
            changed['proposal']['files'][target] += '\nDifferent requested content.\n'
            completed = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            f.public(world, command, 'admission', 'apply', '--proposal', f.save(world, changed), expected=1)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes()
                              for p in bank.rglob('*') if p.is_file()}, completed)


if __name__ == '__main__':
    unittest.main()
