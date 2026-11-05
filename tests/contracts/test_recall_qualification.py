"""Offline controller gates over real public results; no model/performance claim."""
import copy
import importlib.util
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

    def controller(self, world, root, program, *args):
        return world.run([sys.executable, '-I', '-B', '-c',
            'import sys\nsys.path.insert(0, sys.argv[1])\n' + program,
            root / 'scripts/evaluations', *args], timeout=30, check=False)

    def assert_controller(self, world, root, program, *args):
        process = self.controller(world, root, program, *args)
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
 legitimate=rows(case,public[key],good)
 assert quality_outcomes(legitimate,[32],[case],'semantic') == ([],True)
 false_finish=rows(case,public[key],bad)
 # Even identical false claims in all builds must fail absolute quality.
 assert not quality_outcomes(false_finish,[32],[case],'semantic')[1]
 legitimate[-1]['finish_outcome']=bad
 # Across-build parity also checks the outcome, independently of scoring.
 assert quality_outcomes(legitimate,[32],[case],'semantic')[0]
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
import copy
from recall_quality_qualification import complete_provider_evidence,quality_outcomes
requested='anthropic/claude-opus-5-5'
exact={'usage':{'input_tokens':1},'reported_cost_usd':0.01,'model':requested,'model_usage':None}
assert complete_provider_evidence(exact,requested)
mapped={**exact,'model':None,'model_usage':{'claude-opus-5-5':{'canonicalModel':'claude-opus-5-5','provider':'firstParty'}}}
assert complete_provider_evidence(mapped,requested)
wrong={**exact,'model':'anthropic/claude-sonnet-5-5'}
mixed={**mapped,'model':'anthropic/claude-sonnet-5-5'}
canonical=copy.deepcopy(mapped);canonical['model_usage']['claude-opus-5-5']['canonicalModel']='claude-sonnet-5-5'
unvalidated=copy.deepcopy(mapped);unvalidated['model_usage']['claude-opus-5-5'].pop('provider')
extra=copy.deepcopy(mapped);extra['model_usage']['claude-sonnet-5-5']={'provider':'firstParty','canonicalModel':'claude-sonnet-5-5'}
for call in (wrong,mixed,canonical,unvalidated,extra,{**exact,'model':None}):
 assert not complete_provider_evidence(call,requested)
 rows=[{'build':b,'count':32,'case':'no-match','selected':[],'selected_qualified':{},'public_results':[[]],
        'missed':[],'unexpected':[],'finish_outcome':{'complete':True,'clarification_required':False},
        'checks':{'complete_provider_evidence':complete_provider_evidence(call,requested)},'failure':None} for b in ('A','B','C')]
 assert not quality_outcomes(rows,[32],['no-match'],'semantic')[1]
assert not complete_provider_evidence(mapped,requested+':high')
print('exact validated provider identity accepted; wrong, mixed and unvalidated identities rejected')
''')

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
