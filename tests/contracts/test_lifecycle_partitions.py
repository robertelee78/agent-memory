"""All independent fault batches are mandatory, correctly routed and exhaustive."""
import ast
import json
from pathlib import Path
import unittest

from fault_partitions import GROUPS, registered

ROOT = Path(__file__).resolve().parents[2]


class LifecyclePartitionContract(unittest.TestCase):
    def test_every_observed_interruption_partition_is_mandatory_and_correctly_routed(self):
        entries = json.loads((ROOT/'tests/contracts.json').read_text())['suites']
        registered(entries)
        registry = {entry['name']:entry for entry in entries}
        for group in GROUPS.values():
            for index,name in enumerate(group['suites']):
                entry = registry[name]
                self.assertEqual(entry['path'],'tests/contracts/test_'+name.replace('-','_')+'.py')
                tree = ast.parse((ROOT/entry['path']).read_text())
                calls = [node for node in ast.walk(tree) if isinstance(node,ast.Call)
                         and isinstance(node.func,ast.Attribute) and node.func.attr=='replay']
                self.assertEqual(len(calls),1,name)
                call = calls[0]
                expected = [group['phase']] if group['operation']=='update' else [
                    group['operation'],group['mode'],group['phase']]
                self.assertEqual([ast.literal_eval(arg) for arg in call.args],expected,name)
                self.assertEqual({kw.arg:ast.literal_eval(kw.value) for kw in call.keywords},
                                 {'partition':index},name)


if __name__=='__main__':unittest.main()
