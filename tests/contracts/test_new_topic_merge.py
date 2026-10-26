"""New shared indexes can converge without inventing a shared prose baseline."""
import json
import sys
import unittest

from world import World
from record_fixtures import project_id
from contracts import test_semantic_sharing as fixtures


class NewTopicMergeContract(unittest.TestCase):
    def index(self,w,bank,name,*,purpose='Archive policy',introduction=None,source=None,stamp='2026-10-23T00:00:00Z'):
        _,path=self.h.add(bank,name,'# Archive policy\n\nRetain the independently evidenced '+name+' policy.\n')
        if source is not None:(bank/path).write_bytes(source)
        request={'path':'topics/shared-policy.md','project-id':project_id((bank/'MEMORY.md').read_text()),
            'updated':stamp,'routes':[{'path':path,'purpose':purpose,'scope':'repository','lifecycle':'active',
                'read-when':'When inspecting this archive policy dimension'}]}
        content=json.loads(w.run([sys.executable,'-I',self.h.pack/'scripts/bank_layout.py','render-index',
            '--input',self.h.save(w,request)]).stdout)['content']
        if introduction is not None:content=content.replace('\n## Routes\n','\n'+introduction+'\n\n## Routes\n')
        (bank/'topics/shared-policy.md').write_text(content)
        router=bank/'MEMORY.md';lines=router.read_text().splitlines(keepends=True)
        router.write_text(''.join('| [topics/shared-policy.md](<topics/shared-policy.md>) | Archive policy map | repository | active | When inspecting archive policy |\n'
            if '['+path+']' in line else line for line in lines))
        return path

    def reviewed(self,w,repo):
        view=self.h.good(w,'inspect','--repo',repo)
        return self.h.review(w,repo,self.h.judgment(view))

    def test_new_index_with_identical_introduction_and_distinct_routes_preserves_both_sources(self):
        with World() as w:
            self.h=fixtures.SemanticSharingContract();repo,bank,peer,base=self.h.fixture(w)
            first=self.index(w,bank,'retention')
            second=self.index(w,peer/'memory-bank','inspection',stamp='2026-10-24T00:00:00Z')
            self.h.publish(w,peer);before=self.h.state(w,repo,bank)
            result,plan=self.reviewed(w,repo);self.assertEqual(result.returncode,0,(plan,result.stderr))
            self.assertFalse(plan['conflicts_pending'])
            for candidate in (plan['refresh_files'],plan['publish_files']):
                self.assertEqual(candidate[first],(bank/first).read_text())
                self.assertEqual(candidate[second],(peer/'memory-bank'/second).read_text())
                self.assertIn(first,candidate['topics/shared-policy.md']);self.assertIn(second,candidate['topics/shared-policy.md'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_absent_prose_baseline_does_not_license_different_new_introductions(self):
        with World() as w:
            self.h=fixtures.SemanticSharingContract();repo,bank,peer,base=self.h.fixture(w)
            self.index(w,bank,'retention',introduction='The shared policy requires a yellow finish.')
            self.index(w,peer/'memory-bank','inspection',introduction='The shared policy requires a blue finish.')
            self.h.publish(w,peer);before=self.h.state(w,repo,bank)
            result,plan=self.reviewed(w,repo)
            self.assertEqual(result.returncode,1,plan);self.assertEqual(before,self.h.state(w,repo,bank))

    def test_absent_route_baseline_does_not_choose_between_different_captions_for_one_target(self):
        with World() as w:
            self.h=fixtures.SemanticSharingContract();repo,bank,peer,base=self.h.fixture(w)
            path=self.index(w,bank,'shared-source',purpose='Retention interpretation')
            self.index(w,peer/'memory-bank','shared-source',purpose='Inspection interpretation',source=(bank/path).read_bytes())
            self.h.publish(w,peer);before=self.h.state(w,repo,bank)
            result,plan=self.reviewed(w,repo)
            self.assertEqual(result.returncode,1,plan);self.assertEqual(before,self.h.state(w,repo,bank))
