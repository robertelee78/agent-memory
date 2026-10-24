"""Interrupted purge refusal must release only its proven stale lock, not data."""
import json
import sys

from world import World
from lifecycle_fault_fixtures import invoke, helper, read_trace
from runtime_update_fixtures import ROOT, bank, snapshot


class PurgeUnlockCases:
    def fixture(self, world):
        memory = bank(world, world.paths['repo'])
        actual = invoke(world, helper('purge-data', '--repo', world.paths['repo'], '--path', memory, '--check'))
        invalid = '0' * 64
        self.assertNotEqual(actual['preview']['confirmation'], invalid)
        return memory, helper('purge-data', '--repo', world.paths['repo'], '--path', memory, '--confirm', invalid)

    def replay_unlock(self, phase):
        with World() as world:
            memory, argv = self.fixture(world); before = snapshot(memory)
            trace = world.inside('refusal.jsonl')
            refused = invoke(world, argv, expected=1, env={'AGENT_MEMORY_FAULT_TRACE': str(trace)})
            self.assertFalse(refused['verified']); self.assertEqual(snapshot(memory), before)
            rows = read_trace(trace)
            selected = [r for r in rows if r['kind'] == 'purge-bank-unlock' and r['phase'] == phase]
            self.assertEqual(len(selected), 1)
        for event in selected:
            with World() as world:
                memory, argv = self.fixture(world); before = snapshot(memory)
                trace = world.inside('actual.jsonl')
                invoke(world, argv, expected=86, env={'AGENT_MEMORY_FAULT_STAGE': event['point'],
                    'AGENT_MEMORY_FAULT_MODE': 'exit', 'AGENT_MEMORY_FAULT_TRACE': str(trace)})
                observed = [json.loads(line) for line in trace.read_text().splitlines()]
                self.assertEqual(observed[-1], event)
                refused = invoke(world, argv, expected=1)
                self.assertFalse(refused['verified'])
                self.assertEqual(snapshot(memory), before)
                checked = invoke(world, [sys.executable, '-I', ROOT / 'scripts/bank_contract.py',
                                        'validate', '--repo', world.paths['repo']])
                self.assertTrue(checked['valid'])
                sys.stderr.write(json.dumps({'operation': 'purge-refusal-unlock', 'phase': phase,
                    'point': event['point'], 'kind': event['kind'], 'reached_exit': 86,
                    'bank_unchanged_and_readable': True, 'refusal_remains_refused': True}) + '\n')
