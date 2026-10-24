"""Interrupt the real error-cleanup branch, then recover and retry publicly."""
import json
import sys

from world import World
from lifecycle_fault_fixtures import setup, invoke, helper, read_trace, save_baseline, restore, semantics
from runtime_update_fixtures import snapshot, receipt_path


class CleanupWriteCases:
    def replay_cleanup(self, phase):
        with World() as world:
            argv, memory = setup(world, 'copy'); before = snapshot(memory)
            base, parts = save_baseline(world)
            trace = world.inside('success.jsonl')
            successful = invoke(world, argv, env={'AGENT_MEMORY_FAULT_TRACE': str(trace)})
            expected = semantics(successful['receipt'])
            rows = read_trace(trace)
            publication = next(r for r in rows if r['kind'] == 'record-publication')
            anchor = next(r for r in rows if r['step'] == publication['step'] - 1 and r['phase'] == 'after')
            self.assertEqual(anchor['kind'], 'file-stage')
            restore(world, base, parts)
            error_trace = world.inside('error.jsonl')
            failed = invoke(world, argv, expected=1, env={
                'AGENT_MEMORY_FAULT_ERROR_STAGE': anchor['point'], 'AGENT_MEMORY_FAULT_TRACE': str(error_trace)})
            self.assertFalse(failed['verified'])
            error_rows = read_trace(error_trace)
            selected = [r for r in error_rows if r['kind'] == 'stage-cleanup' and r['phase'] == phase]
            self.assertEqual(len(selected), 1)
            for event in selected:
                restore(world, base, parts)
                actual = world.inside('actual.jsonl')
                invoke(world, argv, expected=86, env={
                    'AGENT_MEMORY_FAULT_ERROR_STAGE': anchor['point'],
                    'AGENT_MEMORY_FAULT_STAGE': event['point'], 'AGENT_MEMORY_FAULT_MODE': 'exit',
                    'AGENT_MEMORY_FAULT_TRACE': str(actual)})
                observed = [json.loads(line) for line in actual.read_text().splitlines()]
                self.assertEqual(observed[-1], event)
                self.assertTrue(invoke(world, helper('recover'))['verified'])
                self.assertEqual(semantics(invoke(world, argv)['receipt']), expected)
                stable = receipt_path(world).read_bytes()
                self.assertTrue(invoke(world, argv)['verified'])
                self.assertEqual(receipt_path(world).read_bytes(), stable)
                self.assertEqual(snapshot(memory), before)
                sys.stderr.write(json.dumps({'operation': 'install-error-cleanup', 'phase': phase,
                    'point': event['point'], 'kind': event['kind'], 'reached_exit': 86,
                    'retry_matches_clean_semantics': True, 'bank_unchanged': True}) + '\n')
