"""Offline controller gates over real public results; no model/performance claim."""
import copy
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import py_compile
import shutil
import sys
import unittest

from world import World
from identity_fixtures import bind

ROOT = Path(__file__).resolve().parents[2]
EVALUATIONS = ROOT / 'scripts/evaluations'


class RecallQualificationContract(unittest.TestCase):
    def stage(self, world, name='controller-source'):
        root = world.inside(name)
        shutil.copytree(EVALUATIONS, root / 'scripts/evaluations',
                        ignore=shutil.ignore_patterns('__pycache__', '*.pyc', '*.pyo'))
        return root

    def controller(self, world, root, program, *args, timeout=30):
        return world.run([sys.executable, '-I', '-B', '-c',
            'import sys\nsys.path.insert(0, sys.argv[1])\n' + program,
            root / 'scripts/evaluations', *args], timeout=timeout, check=False)

    def assert_controller(self, world, root, program, *args, timeout=30):
        process = self.controller(world, root, program, *args, timeout=timeout)
        self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
        return process

    def literal_results(self, world):
        # Reuse the frozen template and corpus without changing any fixture gate.
        spec = importlib.util.spec_from_file_location('qualification_literal_fixture', EVALUATIONS / 'labeled_recall_fixture.py')
        fixture = importlib.util.module_from_spec(spec)
        exec(compile(Path(spec.origin).read_bytes(), spec.origin, 'exec'), fixture.__dict__)
        frozen = json.loads((ROOT / 'docs/research/data/2026-10-26-recall-native/literal-quality/1000-labels.json').read_text())
        files, _, cases = fixture.corpus(frozen['fixed_record_sources'][0]['metadata'], 32)
        repo = world.repository()
        pack = world.inside('public-pack')
        shutil.copytree(ROOT / 'skills/agent-memory', pack, ignore=shutil.ignore_patterns('__pycache__', '*.pyc', '*.pyo'))
        bank = repo / 'memory-bank'; bank.mkdir()
        for name, body in files.items():
            path = bank / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(body)
        original_run = world.run
        def run(argv, **kwargs):
            argv = list(map(str, argv))
            if argv[0] == sys.executable and '-B' not in argv:
                argv.insert(1, '-B')
            kwargs.setdefault('timeout', 30)
            return original_run(argv, **kwargs)
        world.run = run
        bind(world, repo)
        results = {}
        for name in ('two-relevant', 'cross-file-contradiction', 'unknown-prerequisite', 'no-match'):
            case = next(case for case in cases if case['case'] == name)
            request = world.inside('request.json'); request.write_text(json.dumps(case['request']))
            process = world.run([sys.executable, '-I', '-B', pack / 'scripts/retrieve.py', '--repo', repo, '--request', request])
            results[name] = json.loads(process.stdout)
        self.assertEqual(len(results['two-relevant']['results']), 2)
        self.assertEqual(len(results['cross-file-contradiction']['results']), 2)
        self.assertEqual(results['unknown-prerequisite']['results'], [])
        self.assertEqual(results['no-match']['results'], [])
        return results

    def test_real_public_results_cannot_hide_order_finish_or_recovery_action_changes(self):
        with World() as world:
            root = self.stage(world)
            public = self.literal_results(world)
            path = world.inside('public-results.json'); path.write_text(json.dumps(public))
            self.assert_controller(world, root, '''
import copy,json
from pathlib import Path
from recall_quality_qualification import quality_outcomes, semantic_outcome_checks, public_projection as quality_public_projection
from recall_refresh_qualification import public_projection
public=json.loads(Path(sys.argv[2]).read_text())
def rows(case,result,finish=None):
 qualified={r['path']:r for r in result['results']};selected=sorted(qualified)
 checks=semantic_outcome_checks(case,finish) if finish else {'public_operation_succeeded':True}
 return [{'build':build,'count':32,'case':case,'selected':selected,'selected_qualified':qualified,
          'public_results':[quality_public_projection(result)], 'missed':[],'unexpected':[],
          'finish_outcome':copy.deepcopy(finish),'checks':checks,'failure':None} for build in ('A','B','C')]
ordered=rows('two-relevant',public['two-relevant'])
assert quality_outcomes(ordered,[32],['two-relevant'],'literal') == ([],True)
ordered[-1]['public_results'][0]['results'].reverse()
regressions,passed=quality_outcomes(ordered,[32],['two-relevant'],'literal')
assert regressions and not passed
for field,value in (('semantic_coverage','incomplete-recovery'),('recovery_required_actions',['resolve-layout-recovery']),
                    ('code_working_bytes_verified',True)):
 changed=rows('two-relevant',public['two-relevant'])
 changed[-1]['public_results'][0][field]=value
 assert quality_outcomes(changed,[32],['two-relevant'],'literal')[0]
for case,key,good,bad in (
 ('wall-disagreement','cross-file-contradiction',{'complete':True,'clarification_required':True},{'complete':True,'clarification_required':False}),
 ('unknown-prerequisite','unknown-prerequisite',{'complete':False,'clarification_required':True},{'complete':True,'clarification_required':False}),
 ('unknown-session','unknown-prerequisite',{'complete':False,'clarification_required':True},{'complete':True,'clarification_required':False}),
 ('no-match','no-match',{'complete':True,'clarification_required':False},{'complete':False,'clarification_required':False})):
 # These counterfactual flag gates use literal public requests; the separate
 # semantic replay test below exercises actual discovery-qualified requests.
 legitimate=rows(case,public[key],good)
 assert quality_outcomes(legitimate,[32],[case],'literal') == ([],True)
 false_finish=rows(case,public[key],bad)
 # Even identical false claims in all builds must fail absolute quality.
 assert not quality_outcomes(false_finish,[32],[case],'literal')[1]
 legitimate[-1]['finish_outcome']=bad
 # Across-build parity also checks the outcome, independently of scoring.
 assert quality_outcomes(legitimate,[32],[case],'literal')[0]
first={'recall':public['two-relevant']};second=copy.deepcopy(first)
assert 'recovery_required_actions' in first['recall']
second['recall']['recovery_required_actions']=['resolve-layout-recovery']
assert public_projection(first) != public_projection(second)
assert public_projection(first) == public_projection(copy.deepcopy(first))
print('actual public order, semantic outcomes and recovery-action gates passed')
''', path)

    def test_wrong_or_mixed_executing_models_leave_quality_unqualified(self):
        with World() as world:
            root = self.stage(world)
            self.assert_controller(world, root, '''
import copy,json
from pathlib import Path
from recall_quality_qualification import complete_provider_evidence,quality_outcomes
requested='anthropic/claude-opus-5-5'
receipt=json.loads((Path(sys.argv[2])/'docs/research/data/2026-10-25-recall-final/semantic-1000/wall-disagreement.json').read_text())
native=receipt['model_usage_and_cost'][0]
exact={**native,'model':requested,'model_usage':None}
assert complete_provider_evidence(exact,requested)
mapped={**native,'model':None}
assert complete_provider_evidence(mapped,requested)
mixed_spelling={**mapped,'model':requested}
assert complete_provider_evidence(mixed_spelling,requested)
namespaced_key={**mapped,'model_usage':{requested:{'canonicalModel':'claude-opus-5-5','provider':'firstParty'}}}
assert complete_provider_evidence(namespaced_key,requested)
assert complete_provider_evidence({**namespaced_key,'model':'claude-opus-5-5'},requested)
wrong={**exact,'model':'anthropic/claude-sonnet-5-5'}
mixed={**mapped,'model':'anthropic/claude-sonnet-5-5'}
canonical=copy.deepcopy(mapped);canonical['model_usage']['claude-opus-5-5']['canonicalModel']='claude-sonnet-5-5'
unvalidated=copy.deepcopy(mapped);unvalidated['model_usage']['claude-opus-5-5'].pop('provider')
extra=copy.deepcopy(mapped);extra['model_usage']['claude-sonnet-5-5']={'provider':'firstParty','canonicalModel':'claude-sonnet-5-5'}
wrong_provider=copy.deepcopy(mixed_spelling);wrong_provider['model_usage']['claude-opus-5-5']['provider']='other'
namespaced_canonical=copy.deepcopy(mixed_spelling);namespaced_canonical['model_usage']['claude-opus-5-5']['canonicalModel']=requested
alias={**mixed_spelling,'model':requested+':high'}
invalid_type={**mixed_spelling,'model':{'name':requested}}
for call in (wrong,mixed,canonical,unvalidated,extra,wrong_provider,namespaced_canonical,alias,invalid_type,
             {**exact,'model':None},{**mixed_spelling,'usage':None},{**mixed_spelling,'reported_cost_usd':None}):
 assert not complete_provider_evidence(call,requested)
 rows=[{'build':b,'count':32,'case':'no-match','selected':[],'selected_qualified':{},'public_results':[],
        'source_reads':[],'native_replays':[],'missed':[],'unexpected':[],'finish_outcome':{'complete':True,'clarification_required':False},
        'checks':{'complete_provider_evidence':complete_provider_evidence(call,requested)},'failure':None} for b in ('A','B','C')]
 assert not quality_outcomes(rows,[32],['no-match'],'semantic')[1]
assert not complete_provider_evidence(mapped,requested+':high')
print('exact validated provider identity accepted; wrong, mixed and unvalidated identities rejected')
''', ROOT)

    def stale_cache(self, target):
        original = target.read_bytes(); stat = target.stat()
        malicious = b"raise RuntimeError('STALE_HELPER_EXECUTED')\n"
        self.assertGreater(len(original), len(malicious))
        target.write_bytes(malicious + b'#' * (len(original) - len(malicious)))
        os.utime(target, ns=(stat.st_atime_ns, stat.st_mtime_ns))
        cache = Path(py_compile.compile(str(target), doraise=True))
        target.write_bytes(original)
        os.utime(target, ns=(stat.st_atime_ns, stat.st_mtime_ns))
        return cache

    def test_matching_stale_binding_helper_is_rejected_before_every_local_import(self):
        with World() as world:
            root = self.stage(world)
            cache = self.stale_cache(root / 'scripts/evaluations/recall_comparison_builds.py')
            before = cache.read_bytes()
            for name in ('recall_quality_qualification.py', 'recall_refresh_qualification.py',
                         'recall_navigation_qualification.py', 'recall_qualification_matrix.py'):
                with self.subTest(controller=name):
                    process = world.run([sys.executable, '-B', root / 'scripts/evaluations' / name, '--help'],
                                        timeout=30, check=False)
                    self.assertNotEqual(process.returncode, 0)
                    self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)
                    self.assertNotIn('STALE_HELPER_EXECUTED', process.stderr)
                    self.assertEqual(cache.read_bytes(), before)

    def test_target_world_cache_rejected_before_import_and_clean_source_still_binds(self):
        with World() as world:
            controller = self.stage(world)
            target = world.repository('target-source')
            (target / 'tests').mkdir()
            shutil.copy2(ROOT / 'tests/world.py', target / 'tests/world.py')
            (target / '.gitignore').write_text('__pycache__/\n*.pyc\n')
            for name in ('recall_quality_qualification.py', 'recall_refresh_qualification.py',
                         'recall_navigation_qualification.py', 'recall_qualification_matrix.py',
                         'recall_comparison_builds.py'):
                process = world.run([sys.executable, '-B', controller / 'scripts/evaluations' / name, '--help'],
                                    timeout=30, check=False)
                self.assertEqual(process.returncode, 0, process.stderr)
            world.git('add', '.', cwd=target); world.git('commit', '-m', 'Source binding control', cwd=target)
            self.assert_controller(world, controller, '''
from pathlib import Path
from recall_comparison_builds import source
binding=source(Path(sys.argv[2]))
assert 'tests/world.py' in binding['tracked']
print('bytecode-free source binding passed')
''', target)
            cache = self.stale_cache(target / 'tests/world.py'); before = cache.read_bytes()
            output = world.inside('rejected-navigation')
            process = world.run([sys.executable, '-B', controller / 'scripts/evaluations/recall_navigation_qualification.py',
                '--root', target, '--out', output, '--count', '1000', '--timeout', '1'], timeout=30, check=False)
            self.assertNotEqual(process.returncode, 0)
            self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)
            self.assertNotIn('STALE_HELPER_EXECUTED', process.stderr)
            self.assertEqual(cache.read_bytes(), before)
            # The comparison-build entry point itself also guards before imports.
            local = controller / 'scripts/evaluations/labeled_recall_probe.py'
            self.stale_cache(local)
            process = world.run([sys.executable, '-B', controller / 'scripts/evaluations/recall_comparison_builds.py', '--help'],
                                timeout=30, check=False)
            self.assertNotEqual(process.returncode, 0)
            self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)
            self.assertNotIn('STALE_HELPER_EXECUTED', process.stderr)

    def linked_cache(self, world, target, name):
        cache = self.stale_cache(target)
        external = world.inside(name)
        cache.parent.rename(external)
        cache.parent.symlink_to(external, target_is_directory=True)
        return cache, cache.read_bytes()

    def test_linked_stale_helper_cache_rejected_by_all_bootstraps_without_following_it(self):
        with World() as world:
            root = self.stage(world)
            cache, before = self.linked_cache(world, root / 'scripts/evaluations/recall_comparison_builds.py', 'binding-cache')
            for name in ('recall_quality_qualification.py', 'recall_refresh_qualification.py',
                         'recall_navigation_qualification.py', 'recall_qualification_matrix.py'):
                with self.subTest(controller=name):
                    process = world.run([sys.executable, '-B', root / 'scripts/evaluations' / name, '--help'],
                                        timeout=30, check=False)
                    self.assertNotEqual(process.returncode, 0)
                    self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)
                    self.assertNotIn('STALE_HELPER_EXECUTED', process.stderr)
                    self.assertTrue(cache.parent.is_symlink())
                    self.assertEqual(cache.read_bytes(), before)
            # The comparison entry point must guard its own first helper too.
            comparison = self.stage(world, 'comparison-source')
            probe, original = self.linked_cache(world, comparison / 'scripts/evaluations/labeled_recall_probe.py', 'probe-cache')
            process = world.run([sys.executable, '-B', comparison / 'scripts/evaluations/recall_comparison_builds.py', '--help'],
                                timeout=30, check=False)
            self.assertNotEqual(process.returncode, 0)
            self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)
            self.assertNotIn('STALE_HELPER_EXECUTED', process.stderr)
            self.assertEqual(probe.read_bytes(), original)

    def test_ignored_linked_target_caches_and_cycles_are_rejected_before_fixture_import(self):
        with World() as world:
            controller = self.stage(world)
            for directory in ('tests', 'scripts/evaluations', 'skills/agent-memory/scripts/agent_memory'):
                with self.subTest(directory=directory):
                    target = world.repository('target-' + directory.replace('/', '-'))
                    helper = target / directory / 'world.py'
                    helper.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(ROOT / 'tests/world.py', helper)
                    (target / '.gitignore').write_text('__pycache__\n*.pyc\n')
                    world.git('add', '.', cwd=target); world.git('commit', '-m', 'Source binding control', cwd=target)
                    cache, before = self.linked_cache(world, helper, 'external-' + directory.replace('/', '-'))
                    self.assertEqual(world.git('status', '--porcelain', '--untracked-files=all', cwd=target).stdout, '')
                    process = self.controller(world, controller, '''
from pathlib import Path
from recall_comparison_builds import source
source(Path(sys.argv[2]))
''', target)
                    self.assertNotEqual(process.returncode, 0)
                    self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)
                    self.assertEqual(cache.read_bytes(), before)
                    if directory == 'tests':
                        output = world.inside('rejected-linked-navigation')
                        process = world.run([sys.executable, '-B', controller / 'scripts/evaluations/recall_navigation_qualification.py',
                            '--root', target, '--out', output, '--count', '1000', '--timeout', '1'], timeout=30, check=False)
                        self.assertNotEqual(process.returncode, 0)
                        self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)
                        self.assertNotIn('STALE_HELPER_EXECUTED', process.stderr)
                        self.assertEqual(cache.read_bytes(), before)
            cycle = world.repository('cycle-source')
            (cycle / 'tests').mkdir()
            (cycle / 'tests/__pycache__').symlink_to(cycle / 'tests', target_is_directory=True)
            process = self.controller(world, controller, '''
from pathlib import Path
from recall_comparison_builds import source
source(Path(sys.argv[2]))
''', cycle)
            self.assertNotEqual(process.returncode, 0)
            self.assertIn('Qualification helper bytecode is not source-bound', process.stderr)

    def test_source_binding_hashes_data_link_bytes_without_following_or_executing_links(self):
        with World() as world:
            controller = self.stage(world)
            target = world.repository('linked-data-source')
            data = target / 'docs/research/data'
            data.mkdir(parents=True)
            link = data / 'dangling.json'
            link.symlink_to('missing-target.json')
            world.git('add', '.', cwd=target); world.git('commit', '-m', 'Retain negative data fixture', cwd=target)
            program = '''
import json
from pathlib import Path
from recall_comparison_builds import source
try:
 binding=source(Path(sys.argv[2]))
except ValueError:
 raise SystemExit(2)
print(json.dumps(binding))
'''
            first = json.loads(self.assert_controller(world, controller, program, target).stdout)
            entry = first['tracked']['docs/research/data/dangling.json']
            self.assertEqual(entry['kind'], 'symlink')
            self.assertEqual(entry['sha256'], hashlib.sha256(b'missing-target.json').hexdigest())
            self.assertFalse((data / 'missing-target.json').exists())
            link.unlink(); link.symlink_to('different-missing-target.json')
            refused = self.controller(world, controller, program, target)
            self.assertEqual(refused.returncode, 2, refused.stderr)
            world.git('add', '.', cwd=target); world.git('commit', '-m', 'Changed fixture link identity', cwd=target)
            second = json.loads(self.assert_controller(world, controller, program, target).stdout)
            self.assertNotEqual(second['tracked']['docs/research/data/dangling.json'], entry)
            for directory in ('tests', 'scripts', 'skills/agent-memory'):
                with self.subTest(executable_root=directory):
                    executable = target / directory / 'linked.py'
                    executable.parent.mkdir(parents=True, exist_ok=True)
                    executable.symlink_to('missing-executable.py')
                    world.git('add', '.', cwd=target); world.git('commit', '-m', 'Untrusted executable link', cwd=target)
                    refused = self.controller(world, controller, program, target)
                    self.assertEqual(refused.returncode, 2, refused.stderr)
                    self.assertFalse((executable.parent / 'missing-executable.py').exists())
                    executable.unlink()
                    world.git('add', '.', cwd=target); world.git('commit', '-m', 'Remove executable link control', cwd=target)

    def test_independent_pagination_batches_compare_every_equivalent_exact_native_request(self):
        with World() as world:
            root = self.stage(world)
            self.assert_controller(world, root, '''
import copy,json,shutil,time
from pathlib import Path
from recall_quality_qualification import (SEMANTIC_SOURCE, native_replay_matches, public_projection,
    quality_outcomes, replay_source_reads, semantic_cases, semantic_outcome_checks)
from labeled_recall_probe import inventory
from semantic_navigation_fixture import corpus
source=Path(sys.argv[2]);sys.path.insert(0,str(source/'tests'))
from world import World
from identity_fixtures import bind
name,question,context,labels=next(c for c in semantic_cases(source/SEMANTIC_SOURCE) if c[0]=='pagination')
frozen=json.loads((source/'docs/research/data/2026-10-26-recall-native/literal-quality/1000-labels.json').read_text())
files,records,_=corpus(frozen['fixed_record_sources'][0]['metadata'],64)
paths=[r['path'] for r in records if r['label'] in labels]
assert len(paths)==2
with World() as actual:
 repo=actual.repository()
 (repo/'code.py').write_text('VALUE = 1\\n')
 actual.git('add','code.py',cwd=repo);actual.git('commit','-m','Native replay source',cwd=repo)
 (repo/'code.py').write_text('VALUE = 2 # staged\\n');actual.git('add','code.py',cwd=repo)
 bank=repo/'memory-bank';bank.mkdir()
 for path,body in files.items():
  target=bank/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(body)
 original_run=actual.run
 def run(argv,**kwargs):
  argv=list(map(str,argv))
  if argv[0]==sys.executable and '-B' not in argv:argv.insert(1,'-B')
  kwargs.setdefault('timeout',30)
  return original_run(argv,**kwargs)
 actual.run=run;bind(actual,repo)
 packs={}
 for build in ('A','B','C'):
  packs[build]=actual.inside('public-pack-'+build)
  shutil.copytree(source/'skills/agent-memory',packs[build],ignore=shutil.ignore_patterns('__pycache__','*.pyc','*.pyo'))
 initial=inventory(bank);index=(repo/'.git/index').read_bytes();head=actual.git('rev-parse','HEAD',cwd=repo).stdout
 pack_bytes={build:inventory(pack) for build,pack in packs.items()}
 commands=[]
 def write(value,name):
  path=actual.inside(name+'.json');path.write_text(json.dumps(value));return path
 def public(helper,*argv,from_pack=packs['A'],purpose='observed'):
  before=time.perf_counter()
  inputs={key:json.loads(Path(argv[argv.index(key)+1]).read_text()) for key in ('--request','--input') if key in argv}
  process=actual.run([sys.executable,'-I','-B',from_pack/'scripts'/helper,*argv])
  commands.append({'purpose':purpose,'seconds':time.perf_counter()-before,'argv':list(map(str,argv)),
                   'inputs':inputs,'pack':str(from_pack),'stdout':process.stdout,'stderr':process.stderr,'exit':process.returncode})
  return json.loads(process.stdout),process.stdout+process.stderr
 state,_=public('discover_memory.py','start','--repo',repo,'--intent','recall','--task',question,
                '--context',write(context,'context'))
 for node in ('topics/g/pagination.md','topics/n/pagination-0-1.md'):
  state,_=public('discover_memory.py','node','--repo',repo,'--input',write(state['discovery'],'navigation'),
                 '--path',node)
 original_state=copy.deepcopy(state)
 observations=[]
 for build,batches in (('A',[paths]),('B',[[paths[0]],[paths[1]]]),('C',[[paths[1]],[paths[0]]])):
  state=copy.deepcopy(original_state)
  reads,observed,qualified=[],[],{}
  for batch in batches:
   request={'project_id':state['discovery']['binding']['project_id'],'query':question,'ids':[],
            'paths':batch,'context':context,'kinds':['semantic-fact'],'minimum_trust':'reviewed','limit':16,'include_context':False}
   read={'request':copy.deepcopy(request),'discovery':copy.deepcopy(state['discovery']),
         'rationale':'Inspect all independently routed pagination evidence.','command_number':len(commands)+1}
   state,_=public('discover_memory.py','read','--repo',repo,'--input',write(read['discovery'],'navigation'),
                  '--request',write(request,'request'),'--rationale',read['rationale'],from_pack=packs[build])
   reads.append(read);observed.append(public_projection(state['qualification']))
   qualified.update({r['path']:r for r in state['qualification']['results']})
  first=len(commands)
  replays=replay_source_reads(reads,public,packs,repo,write)
  assert len(commands)-first==3*len(batches)
  assert all(c['purpose']=='native-replay' for c in commands[first:])
  for offset,read in enumerate(reads):
   for label,command in zip(('A','B','C'),commands[first+3*offset:first+3*(offset+1)]):
    assert command['inputs']=={'--request':read['request'],'--input':read['discovery']}
    assert command['pack']==str(packs[label])
    assert command['argv'][-1]==read['rationale']
  finish={'complete':True,'clarification_required':False}
  observations.append({'build':build,'count':64,'case':name,'selected':sorted(qualified),'selected_qualified':qualified,
    'public_results':observed,'source_reads':reads,'native_replays':replays,'missed':[],'unexpected':[],
    'finish_outcome':finish,'checks':semantic_outcome_checks(name,finish),'failure':None})
 assert [len(row['public_results']) for row in observations]==[1,2,2]
 assert quality_outcomes(observations,[64],[name],'semantic')==([],True)
 assert all(native_replay_matches(row) for row in observations)
 # Original native order is retained; reversing either an observed response
 # or a replay is not made equivalent by relevance's sorted path accounting.
 for target in ('observed','replay'):
  changed=copy.deepcopy(observations)
  result=changed[0]['public_results'][0] if target=='observed' else changed[0]['native_replays'][0]['public_results']['C']
  result['results'].reverse()
  assert not quality_outcomes(changed,[64],[name],'semantic')[1]
 for field,value in (('semantic_coverage','incomplete-recovery'),('pending_recovery_units',1),
                     ('pending_recovery_unit_ids',['unresolved']),('recovery_required_actions',['resolve-layout-recovery']),
                     ('code_working_bytes_verified',True)):
  changed=copy.deepcopy(observations)
  changed[0]['native_replays'][0]['public_results']['C'][field]=value
  assert not quality_outcomes(changed,[64],[name],'semantic')[1]
 changed=copy.deepcopy(observations)
 changed[0]['native_replays'][0]['public_results']['C']['results'][0]['record']['version']+=1
 assert not quality_outcomes(changed,[64],[name],'semantic')[1]
 for field in ('native_replays','source_reads'):
  changed=copy.deepcopy(observations);changed[1][field].pop()
  assert not quality_outcomes(changed,[64],[name],'semantic')[1]
 changed=copy.deepcopy(observations);changed[0].pop('native_replays')
 assert not quality_outcomes(changed,[64],[name],'semantic')[1]
 changed=copy.deepcopy(observations);changed[0]['source_reads'][0]['request']['paths'].reverse()
 assert not quality_outcomes(changed,[64],[name],'semantic')[1]
 assert inventory(bank)==initial and (repo/'.git/index').read_bytes()==index
 assert actual.git('rev-parse','HEAD',cwd=repo).stdout==head
 assert (repo/'code.py').read_text()=='VALUE = 2 # staged\\n'
 assert all(inventory(pack)==pack_bytes[build] for build,pack in packs.items())
print('actual independent pagination batches, exact A/B/C replays, ordering/authority regressions and protected state passed')
''', ROOT, timeout=90)
