"""A private frontier transition must not be answered from stale catalog coverage."""
import json
import uuid
import unittest

from world import World
import layout_recovery_fixtures as f


class RecoveryCatalogContract(unittest.TestCase):
    def test_ledger_only_retirement_refreshes_enabled_and_disabled_recall(self):
        with World() as w:
            command, _, repo, bank, _, inspected, applied = f.small_pending(
                w, body='# Temporary scratch\n\nDisposable non-guidance work state.\n')
            request = {'project_id': inspected['manifest']['project_id'], 'query': '',
                'ids': [], 'paths': ['projectbrief.md'], 'context': {}, 'kinds': [],
                'minimum_trust': 'reviewed', 'limit': 8, 'include_context': True}

            def trace():
                output = w.inside('discovery-' + uuid.uuid4().hex + '.json')
                envelope = f.public(w, command, 'discover', 'start', '--repo', repo,
                    '--intent', 'recall', '--task', 'Review the project orientation.', '--private-output', output)
                value = json.loads(output.read_text()) if output.exists() else envelope
                selected = value['discovery']
                selected['sources'] = [{'path': 'projectbrief.md',
                    'sha256': f.sha((bank / 'projectbrief.md').read_bytes())}]
                selected.update(complete=True, rationale='Read the selected project orientation source.')
                request['discovery'] = selected

            def recall(disabled):
                if disabled:
                    w.env['AGENT_MEMORY_NO_RECALL_CATALOG'] = '1'
                else:
                    w.env.pop('AGENT_MEMORY_NO_RECALL_CATALOG', None)
                return f.public(w, command, 'search', '--repo', repo, '--request', f.save(w, request))

            def canonical():
                return {p.relative_to(bank).as_posix(): f.sha(p.read_bytes()) for p in bank.rglob('*')
                    if p.is_file() and not p.name.startswith('.agent-memory')}

            trace()
            cold = recall(True)
            recall(False)
            warm = recall(False)
            self.assertTrue(warm['optional_indexes_used'], 'integration must exercise an authenticated warm hit')
            pointer = next(w.paths['state'].rglob('recall-catalog-v1/current.json'))
            self.assertTrue(pointer.exists())
            for result in (cold, warm):
                self.assertEqual(result['semantic_coverage'], 'incomplete-recovery')
                self.assertEqual(result['pending_recovery_unit_ids'], applied['pending_unit_ids'])
                self.assertEqual(result['pending_recovery_units'], 1)
                self.assertTrue(result['recovery_required_actions'])
            self.assertEqual(cold['results'], warm['results'])
            before = canonical()
            proposal = f.discard_proposal(w, command, repo, bank, inspected, applied['pending_unit_ids'][0])
            f.call(w, command, 'resolve', '--repo', repo, '--proposal', proposal)
            self.assertFalse(pointer.exists(), 'frontier-only retirement must invalidate the authenticated pointer')
            self.assertEqual(canonical(), before)
            trace()
            enabled = recall(False)
            disabled = recall(True)
            self.assertEqual(enabled['results'], cold['results'])
            self.assertEqual(enabled['results'], disabled['results'])
            for result in (enabled, disabled):
                self.assertEqual(result['semantic_coverage'], 'complete-canonical-observation')
                self.assertEqual(result['pending_recovery_unit_ids'], [])
                self.assertEqual(result['pending_recovery_units'], 0)
                self.assertEqual(result['recovery_required_actions'], [])


if __name__ == '__main__':
    unittest.main()
