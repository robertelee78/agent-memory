"""Static catalog completeness and no candidate-code execution through public inspect."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from migration_fixtures import ROOT, LEGACY, MIDDLE, NATIVE, catalog


class MigrationCatalogContract(unittest.TestCase):
    def test_bundled_chain_is_statically_inspected_without_importing_candidate_code(self):
        for damage in (None, 'missing-step', 'cycle', 'ambiguous', 'missing-code', 'unadvertised-source', 'decorator'):
            with self.subTest(damage=damage), World() as w:
                pack = w.inside('candidate-pack'); shutil.copytree(ROOT/'skills/agent-memory', pack); catalog(pack)
                canary = w.inside('candidate-was-executed')
                module = pack/'scripts/agent_memory/migration_fixture.py'
                module.write_text(module.read_text()+f'\nfrom pathlib import Path\nPath({str(canary)!r}).write_text("not allowed during inspect")\n')
                path = pack/'schemas/migrations.json'; value = json.loads(path.read_text())
                if damage == 'missing-step': value['steps'].pop()
                elif damage == 'cycle':
                    value['steps'].append({'id':'test-only-cycle', 'from':MIDDLE, 'to':LEGACY,
                                           'converter':value['steps'][0]['converter']})
                elif damage == 'ambiguous':
                    value['steps'].append({'id':'test-only-shortcut', 'from':LEGACY, 'to':NATIVE,
                                           'converter':value['steps'][0]['converter']})
                elif damage == 'missing-code': module.unlink()
                elif damage == 'unadvertised-source':
                    compat = pack/'schemas/runtime-compatibility.json'; spec = json.loads(compat.read_text())
                    spec['migrator_source_schemas'] = []; compat.write_text(json.dumps(spec))
                elif damage == 'decorator':
                    module.write_text(module.read_text().replace('def first(files):', '@staticmethod\ndef first(files):'))
                path.write_text(json.dumps(value))
                result = w.run([sys.executable, '-I', ROOT/'scripts/artifact.py', 'inspect', '--pack', pack], check=False)
                checked = json.loads(result.stdout)
                self.assertEqual(result.returncode, 0 if damage is None else 1, checked)
                self.assertEqual(checked['valid'], damage is None)
                self.assertFalse(checked['candidate_code_executed']); self.assertFalse(canary.exists())

    def test_complete_unreachable_diamond_graph_is_bounded_without_enumerating_every_path(self):
        with World() as w:
            pack = w.inside('candidate-pack'); shutil.copytree(ROOT/'skills/agent-memory', pack); catalog(pack)
            path = pack/'schemas/migrations.json'; value = json.loads(path.read_text())
            previous = ['test-dead-root']
            dead = set(previous)
            for depth in range(15):
                following = ['test-dead-'+str(depth)+'a', 'test-dead-'+str(depth)+'b']; dead.update(following)
                for source in previous:
                    for target in following:
                        value['steps'].append({'id':'edge-'+str(len(value['steps'])), 'from':'agent-memory/'+source,
                                               'to':'agent-memory/'+target, 'converter':value['steps'][0]['converter']})
                previous = following
            validator = value['codecs'][1]['validator']
            value['codecs'] += [{'schema':'agent-memory/'+name,'validator':validator} for name in sorted(dead)]
            path.write_text(json.dumps(value))
            result = w.run([sys.executable, '-I', ROOT/'scripts/artifact.py', 'inspect', '--pack', pack],
                           check=False, timeout=10)
            checked = json.loads(result.stdout); self.assertEqual(result.returncode, 0, checked)
            self.assertTrue(checked['valid']); self.assertFalse(checked['candidate_code_executed'])


if __name__ == '__main__':
    unittest.main()
