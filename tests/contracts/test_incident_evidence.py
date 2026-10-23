"""Retained public incidents, actual CLI examples and normative guide mappings."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World

ROOT=Path(__file__).resolve().parents[2]


class IncidentEvidenceContract(unittest.TestCase):
    def save(self,w,value,name='evidence.json'):
        path=w.inside(name);path.write_text(json.dumps(value));return path

    def call(self,w,*args):
        r=w.run([sys.executable,'-I',ROOT/'scripts/guide_evidence.py',*args],check=False)
        return r,json.loads(r.stdout)

    def snapshot(self,w,pack):
        r,v=self.call(w,'snapshot','--pack',pack);self.assertEqual(r.returncode,0,v);return v

    def fixture(self,w):
        old=w.inside('old-pack');(old/'rules').mkdir(parents=True)
        (old/'SKILL.md').write_text('# Example skill\n\nOpen [the rules](rules/GUIDE.md).\n')
        (old/'rules/GUIDE.md').write_text('# Rules\n\nA session MUST preserve the original evidence.\n\nA session MUST NOT guess another developer\'s answer.\n\n```text\nThis example MAY be ignored by the normative prose collector.\n```\n')
        new=w.inside('new-pack');shutil.copytree(old,new)
        return old,new,self.snapshot(w,old)

    def check(self,w,old,new,before,mapping=None):
        args=['check','--before',self.save(w,before),'--before-pack',old,'--pack',new]
        if mapping is not None:args+=['--map',self.save(w,mapping,'map.json')]
        return self.call(w,*args)

    def test_real_runtime_guides_are_reachable_and_an_unchanged_tree_preserves_every_rule(self):
        with World() as w:
            pack=ROOT/'skills/agent-memory';before=self.snapshot(w,pack)
            self.assertGreater(len(before['rules']),100)
            r,v=self.check(w,pack,pack,before);self.assertEqual(r.returncode,0,v)
            self.assertEqual(len(v['rule_map']),len(before['rules']));self.assertTrue(v['passed'])
            self.assertFalse(v['semantic_equivalence_verified'])

    def test_moved_identical_rules_and_explicit_wording_map_keep_equal_force(self):
        with World() as w:
            old,new,before=self.fixture(w)
            (new/'renamed').mkdir();(new/'rules/GUIDE.md').rename(new/'renamed/GUIDE.md');(new/'rules').rmdir()
            (new/'SKILL.md').write_text('# Example skill\n\nOpen [the rules](renamed/GUIDE.md).\n')
            r,v=self.check(w,old,new,before);self.assertEqual(r.returncode,0,v)
            self.assertTrue(all(row['method']=='identical-text' for row in v['rule_map']))
            path=new/'renamed/GUIDE.md';path.write_text(path.read_text().replace('preserve the original evidence','retain every original source and its evidence'))
            after=self.snapshot(w,new)
            oldkey=next(k for k,r in before['rules'].items() if 'preserve the original' in r['text'])
            newkey=next(k for k,r in after['rules'].items() if 'retain every original' in r['text'])
            mapping={oldkey:{'new_rules':[newkey],'meaning_preserved':True,'reason':'The reviewed rewording retains the original source/evidence obligation without narrowing it.'}}
            r,v=self.check(w,old,new,before,mapping);self.assertEqual(r.returncode,0,v)
            self.assertEqual(len(v['rule_map']),2)

    def test_removed_weakened_inverted_and_unreachable_instructions_fail_even_with_a_mapping(self):
        for mutation in ('remove','weaken','invert','unreachable','lie-about-before'):
            with self.subTest(mutation=mutation),World() as w:
                old,new,before=self.fixture(w);path=new/'rules/GUIDE.md';text=path.read_text()
                if mutation=='remove':text=text.replace('A session MUST preserve the original evidence.','')
                elif mutation=='weaken':text=text.replace('MUST preserve','SHOULD preserve')
                elif mutation=='invert':text=text.replace('MUST preserve','MUST NOT preserve')
                elif mutation=='unreachable':(new/'SKILL.md').write_text('# Example skill\n\nNo rule route remains.\n')
                elif mutation=='lie-about-before':before['rules'].pop(next(iter(before['rules'])))
                path.write_text(text)
                if mutation in {'weaken','invert'}:
                    after=self.snapshot(w,new);a=next(k for k,r in before['rules'].items() if 'preserve' in r['text']);b=next(k for k,r in after['rules'].items() if 'preserve' in r['text'])
                    mapping={a:{'new_rules':[b],'meaning_preserved':True,'reason':'This false assertion cannot override an actual force/polarity failure.'}}
                else:mapping={}
                r,v=self.check(w,old,new,before,mapping);self.assertEqual(r.returncode,1,v);self.assertFalse(v['passed'])

    def test_examples_are_actual_public_exit_results_bound_to_the_complete_pack(self):
        with World() as w:
            r=w.run([sys.executable,'-I',ROOT/'scripts/capture_guide_examples.py']);value=json.loads(r.stdout)
            self.assertEqual([x['exit'] for x in value['examples']],[0,1,2])
            self.assertEqual(value['examples'][0]['observed_file_paths'],['MEMORY.md','projectbrief.md'])
            self.assertEqual(value['examples'][1]['observed_fields']['error'],'Render input needs project-id, router-updated, contexts and omitted')
            self.assertIn("invalid choice: 'unknown-operation'",value['examples'][2]['observed_stderr'])
            for path,digest in value['artifact_files'].items():
                self.assertEqual(hashlib.sha256((ROOT/'skills/agent-memory'/path).read_bytes()).hexdigest(),digest)
            self.assertFalse(value['live_credentials_required']);self.assertFalse(value['live_configuration_changed'])

    def test_actual_incident_runner_exercises_both_user_visible_public_regressions(self):
        with World() as w:
            catalog=json.loads((ROOT/'tests/incidents.json').read_bytes())
            self.assertEqual(len(catalog['incidents']),2)
            for incident in catalog['incidents']:
                self.assertEqual(incident['delivery']['kind'],'merge')
                self.assertEqual(incident['acceptance']['candidate'],incident['corrected_candidate'])
                self.assertEqual(incident['integration']['candidate'],incident['corrected_candidate'])
                self.assertEqual(incident['delivery']['commit'],incident['integration']['commit'])
                r=w.run([sys.executable,'-I',ROOT/'scripts/run_incident.py',incident['id']],check=False,timeout=180)
                value=json.loads(r.stdout);self.assertEqual(r.returncode,0,(value,r.stderr))
                self.assertTrue(value['passed']);self.assertEqual(value['work_key'],incident['work_key'])
                self.assertEqual(value['tests'],len(incident['tests']));self.assertFalse(value['live_credentials_required'])


if __name__=='__main__':unittest.main()
