"""Disposable cache loss/corruption must heal to useful canonical recall."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from contracts import test_discovery as discovery_fixtures


class RecallCatalogContract(unittest.TestCase):
    fixture=discovery_fixtures.DiscoveryContract.fixture
    call=discovery_fixtures.DiscoveryContract.call
    good=discovery_fixtures.DiscoveryContract.good
    save=discovery_fixtures.DiscoveryContract.save
    trace=discovery_fixtures.DiscoveryContract.trace
    recall=discovery_fixtures.DiscoveryContract.recall

    def ready(self,w):
        repo,bank=self.fixture(w)
        meta,path=self.h.add(bank,'attempt-policy',body='# Attempt policy\n\nRepeat transient requests at most three times.\n')
        trace=self.trace(w,repo,bank,[path]);request=self.recall(trace,path)
        before=self.h.state(w,repo,bank)
        first=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
        self.assertEqual([r['record']['id'] for r in first['results']],[meta['id']])
        self.assertFalse(first['optional_indexes_used']);self.assertTrue(first['metrics']['catalog_updated'])
        self.assertEqual(before,self.h.state(w,repo,bank))
        pointer=next(w.paths['state'].rglob('recall-catalog-v1/current.json'))
        return repo,bank,meta,request,pointer,before

    def test_warm_semantic_selection_matches_cold_sources_without_storing_learning_bodies(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            warm=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertTrue(warm['optional_indexes_used']);self.assertEqual(warm['results'][0]['record'],meta)
            self.assertEqual(warm['results'][0]['body'],'# Attempt policy\n\nRepeat transient requests at most three times.\n')
            generation=pointer.parent/json.loads(pointer.read_text())['file']
            self.assertNotIn(b'Repeat transient requests at most three times.',generation.read_bytes())
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_missing_cache_automatically_rebuilds_and_returns_the_same_useful_memory(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            shutil.rmtree(pointer.parent)
            rebuilt=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertFalse(rebuilt['optional_indexes_used']);self.assertTrue(rebuilt['metrics']['catalog_updated'])
            self.assertEqual(rebuilt['results'][0]['record']['id'],meta['id'])
            self.assertTrue(self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))['optional_indexes_used'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_caller_modified_metadata_cannot_forge_a_successful_empty_selection(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            generation=pointer.parent/json.loads(pointer.read_text())['file']
            header,raw=generation.read_bytes().split(b'\n',1);header=json.loads(header);payload=json.loads(raw)
            payload['checked']['entries']=[]
            raw=json.dumps(payload,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()
            header.update(payload_bytes=len(raw),payload_sha256=hashlib.sha256(raw).hexdigest())
            generation.write_bytes(json.dumps(header,separators=(',',':')).encode()+b'\n'+raw)
            rebuilt=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertFalse(rebuilt['optional_indexes_used']);self.assertTrue(rebuilt['metrics']['catalog_updated'])
            self.assertEqual(rebuilt['results'][0]['record']['id'],meta['id'])
            self.assertTrue(self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))['optional_indexes_used'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_common_installed_cli_reuses_the_same_validated_catalog(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            installed=self.good(w,'installer.py','install','--development-source',self.pack,'--presets','none')
            cli=Path(installed['receipt']['bin_dir'])/'am'
            for _ in range(2):
                result=w.run([cli,'search','--repo',repo,'--request',self.save(w,request)])
                recalled=json.loads(result.stdout)
                self.assertTrue(recalled['optional_indexes_used'])
                self.assertEqual(recalled['results'][0]['record']['id'],meta['id'])
            self.assertEqual(before,self.h.state(w,repo,bank))
