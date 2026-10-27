"""Owned hook merges preserve unrelated client configuration."""
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'skills/agent-memory/scripts'))
from agent_memory import client_hooks
from agent_memory.frontmatter import ContractError


class ClientHookContract(unittest.TestCase):
    def test_owned_entry_roundtrip_preserves_foreign_edits(self):
        old = {'description': 'foreign', 'hooks': {'Stop': [{'hooks': [{'type': 'command', 'command': 'foreign'}]}],
                                                   'UserPromptSubmit': [{'hooks': [{'type': 'command', 'command': 'other'}]}]}}
        mine = client_hooks.entry("python3 '/path with spaces/hook.py'")
        raw = client_hooks.merge(json.dumps(old).encode(), None, mine)
        current = json.loads(raw); current['new-user-setting'] = 'preserved'
        updated = client_hooks.merge(json.dumps(current).encode(), mine, mine)
        removed = json.loads(client_hooks.remove(updated, mine))
        self.assertEqual(removed, {**old, 'new-user-setting': 'preserved'})

    def test_unowned_collision_and_mutated_or_duplicate_owned_entries_refuse(self):
        mine = client_hooks.entry('python3 owned.py')
        initial = client_hooks.merge(b'', None, mine)
        with self.assertRaises(ContractError): client_hooks.merge(initial, None, mine)
        value = json.loads(initial); value['hooks']['UserPromptSubmit'][0]['hooks'][0]['command'] = 'changed'
        with self.assertRaises(ContractError): client_hooks.remove(json.dumps(value).encode(), mine)
        value = json.loads(initial); value['hooks']['UserPromptSubmit'].append(mine)
        with self.assertRaises(ContractError): client_hooks.remove(json.dumps(value).encode(), mine)
        self.assertIsNone(client_hooks.remove(initial, mine, originally_absent=True))

    def test_ambiguous_json_is_refused(self):
        with self.assertRaises(ContractError): client_hooks.merge(b'{"hooks":{},"hooks":{}}', None, client_hooks.entry('x'))
        with self.assertRaises(ContractError): client_hooks.merge(b'{"hooks":[]}', None, client_hooks.entry('x'))


if __name__ == '__main__': unittest.main()
