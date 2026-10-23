#!/usr/bin/env python3
"""Run one retained user-visible incident through its real public interfaces."""
import argparse
import importlib.util
import json
from pathlib import Path
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.dont_write_bytecode=True
sys.path.insert(0,str(ROOT/'tests'))
from world import World,require_world


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('incident');parser.add_argument('--worker',action='store_true');args=parser.parse_args(argv)
    catalog=json.loads((ROOT/'tests/incidents.json').read_bytes())
    entries=[x for x in catalog['incidents'] if x['id']==args.incident]
    if len(entries)!=1:parser.error('Select one known incident ID from tests/incidents.json')
    item=entries[0];path=(ROOT/item['suite']).resolve()
    if not path.is_relative_to(ROOT/'tests/contracts') or not path.is_file():raise ValueError('Incident must name an actual contract suite')
    if not args.worker:
        with World() as world:
            result=world.run([sys.executable,'-I',Path(__file__).resolve(),args.incident,'--worker'],check=False,timeout=300)
        sys.stdout.write(result.stdout);sys.stderr.write(result.stderr);return result.returncode
    require_world()
    spec=importlib.util.spec_from_file_location('incident_suite',path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    suite=unittest.TestSuite([getattr(module,item['class'])(name) for name in item['tests']])
    result=unittest.TextTestRunner(verbosity=2,stream=sys.stderr).run(suite)
    counts={'tests':result.testsRun,'errors':len(result.errors),'failures':len(result.failures),'skipped':len(result.skipped),
            'expected_failures':len(result.expectedFailures),'unexpected_successes':len(result.unexpectedSuccesses)}
    passed=counts['tests']>0 and all(value==0 for key,value in counts.items() if key!='tests')
    print(json.dumps({'schema':'agent-memory/incident-result-v1','incident':item['id'],'work_key':item['work_key'],
                      'passed':passed,**counts,'live_credentials_required':False,'live_configuration_changed':False},sort_keys=True))
    return 0 if passed else 1


if __name__=='__main__':sys.exit(main())
