"""Stored-source cutover guards; not proof of a native-client journey."""
from argparse import Namespace
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest
from unittest.mock import patch

from world import World
from contracts import test_admission
from runtime_update_fixtures import receipt_path as owned_receipt_path

RUNTIME = Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'
sys.path.insert(0, str(RUNTIME))
from agent_memory import omp_boundary, omp_installation, turn_sources
from agent_memory.frontmatter import ContractError


class StoredSourceLifecycle(unittest.TestCase):
    def test_legacy_rollout_boolean_cannot_authorize_new_capability(self):
        with World() as world:
            agent_dir = str(world.inside('native-root'))
            saved = omp_installation.binding(agent_dir, reader_capability='omp-harness-v1')
            old = {'omp_binding': saved}
            arguments = Namespace(omp_agent_dir=None, omp_peers_upgraded=False)
            with self.assertRaises(ContractError):
                omp_installation.configuration(arguments, old, 'omp', False)
            self.assertEqual(saved['peer_reader_capability'], 'omp-harness-v1')
            arguments.omp_peers_upgraded = True
            renewed = omp_installation.configuration(arguments, old, 'omp', False)
            self.assertEqual(renewed['peer_reader_capability'], 'omp-stored-source-v2')
            self.assertEqual(renewed['agent_dir'], agent_dir)
            self.assertEqual(old['omp_binding'], saved)

    def test_observer_verification_cannot_enroll_or_promote_legacy_marker(self):
        with World() as world:
            repo, _ = test_admission.AdmissionContract().fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                base = omp_boundary._base(repo)
                marker = base / 'turn-sources-compat-v1.json'
                with self.assertRaises((ContractError, FileNotFoundError)):
                    omp_boundary.verify_marker(repo)
                self.assertFalse(marker.exists())
                self.assertFalse((base / 'omp-bank-binding-v1.json').exists())
                omp_boundary._activate_marker(repo, fresh_only=True)
                context = omp_boundary.verify_marker(repo)
                self.assertEqual(context.root, repo)
                self.assertEqual(omp_boundary.required_source_capabilities(repo),
                                 {'omp-harness-v1', 'omp-stored-source-v2'})
                historical = {**omp_boundary.MARKER, 'minimum_reader': 'omp-harness-v1'}
                marker.write_bytes(turn_sources.encoded(historical))
                marker.chmod(0o600)
                before = marker.read_bytes()
                with self.assertRaises(ContractError):
                    omp_boundary.verify_marker(repo)
                self.assertEqual(marker.read_bytes(), before)
                self.assertEqual(omp_boundary.required_source_capabilities(repo), {'omp-harness-v1'})
                with self.assertRaises(ContractError):
                    omp_boundary._activate_marker(repo, fresh_only=True)
                self.assertEqual(marker.read_bytes(), before)
                omp_boundary._activate_marker(repo)
                self.assertEqual(json.loads(marker.read_bytes()), omp_boundary.MARKER)
                turn_sources.check_source_compatibility(base)
                marker.write_bytes(turn_sources.encoded({**historical, 'minimum_reader': 'future-reader'}))
                with self.assertRaises(ContractError):
                    turn_sources.check_source_compatibility(base)

    def test_both_contended_lock_families_raise_typed_busy_before_body(self):
        child = '''
import pathlib,sys
sys.path.insert(0,sys.argv[1])
from agent_memory import turn_sources
path=pathlib.Path(sys.argv[3])
operation=turn_sources.private_lock(path) if sys.argv[2]=='private' else turn_sources.locked(path)
try:
    with operation:
        raise RuntimeError('contended lock body executed')
except turn_sources.SourceTransportBusy:
    sys.exit(0)
sys.exit(3)
'''
        with World() as world:
            directory = world.inside('source-locks')
            turn_sources.private_mkdir(directory)
            for family in ('private', 'completion'):
                path = directory / 'source.lock' if family == 'private' else directory
                held = turn_sources.private_lock(path) if family == 'private' else turn_sources.locked(path)
                with self.subTest(family=family), held:
                    result = subprocess.run([sys.executable, '-I', '-B', '-c', child, str(RUNTIME), family, str(path)],
                                            capture_output=True, text=True, timeout=20,
                                            cwd=world.root, env=world.env)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                # Releasing contention must restore actual access, not leave a busy flag.
                available = turn_sources.private_lock(path) if family == 'private' else turn_sources.locked(path)
                with available:
                    (directory / (family + '-entered')).write_text('entered')
                self.assertEqual((directory / (family + '-entered')).read_text(), 'entered')

    def test_receipt_commit_separates_bank_activation_from_rollback(self):
        cases = (('install-before-receipt', 'exit', False),
                 ('install-after-receipt', 'exit', True),
                 ('install-before-stored-source-activation', 'error', True))
        for stage, mode, committed in cases:
            with self.subTest(stage=stage), World() as world:
                # Match the existing disposable recovery fixture's realizable
                # group setup before creating any source or ownership evidence.
                for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                    if directory.stat().st_gid != os.getegid():
                        os.chown(directory, -1, os.getegid())
                repo, _ = test_admission.AdmissionContract().fixture(world)
                pack = world.inside('runtime-source')
                shutil.copytree(RUNTIME.parent, pack)
                root = world.inside('explicit-native-root')
                root.mkdir()
                command = [sys.executable, '-I', '-B', pack / 'scripts/installer.py']
                install = [*command, 'install', '--development-source', pack, '--presets', 'omp',
                           '--omp-agent-dir', root, '--omp-peers-upgraded',
                           '--bin-dir', world.inside('bin'), '--mode', 'copy']
                with patch.dict(os.environ, world.env, clear=True):
                    base = omp_boundary._base(repo)
                marker = base / 'turn-sources-compat-v1.json'
                binding = base / 'omp-bank-binding-v1.json'
                failed = world.run(install, cwd=repo, check=False, timeout=60,
                                   env={'AGENT_MEMORY_FAULT_STAGE': stage,
                                        'AGENT_MEMORY_FAULT_MODE': mode})
                self.assertEqual(failed.returncode, 86 if mode == 'exit' else 1,
                                 failed.stdout + failed.stderr)
                receipt_path = owned_receipt_path(world)
                # Neither bank enrollment nor marker promotion is allowed before
                # the receipt commit, or between commit and postcommit activation.
                self.assertFalse(marker.exists())
                self.assertFalse(binding.exists())
                self.assertEqual(receipt_path.exists(), committed)
                committed_bytes = receipt_path.read_bytes() if committed else None
                if committed:
                    with patch.dict(os.environ, world.env, clear=True):
                        with self.assertRaises((ContractError, FileNotFoundError)):
                            omp_boundary.verify_marker(repo)
                if mode == 'error':
                    pending = json.loads(failed.stdout)
                    self.assertFalse(pending['verified'])
                    self.assertEqual(pending['stored_source_activation'], 'pending')
                    self.assertTrue(pending['receipt_committed'])
                recovered = json.loads(world.run([*command, 'recover'], cwd=repo, timeout=60).stdout)
                self.assertTrue(recovered['verified'], recovered)
                if committed:
                    self.assertEqual(receipt_path.read_bytes(), committed_bytes)
                    self.assertEqual(json.loads(marker.read_bytes()), omp_boundary.MARKER)
                    with patch.dict(os.environ, world.env, clear=True):
                        self.assertEqual(omp_boundary.verify_marker(repo).root, repo)
                    unchanged = json.loads(world.run(install, cwd=repo, timeout=60).stdout)
                    self.assertTrue(unchanged['verified'], unchanged)
                    self.assertFalse(unchanged['changed'])
                    self.assertEqual(receipt_path.read_bytes(), committed_bytes)
                else:
                    self.assertEqual(recovered['status'], 'rolled-back')
                    self.assertFalse(marker.exists())
                    self.assertFalse(binding.exists())
                    self.assertFalse(receipt_path.exists())

    def test_prior_committed_activation_recovers_after_a_later_install_rolls_back(self):
        with World() as world:
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            repo, _ = test_admission.AdmissionContract().fixture(world)
            first_pack = world.inside('first-runtime-source')
            second_pack = world.inside('second-runtime-source')
            shutil.copytree(RUNTIME.parent, first_pack)
            shutil.copytree(RUNTIME.parent, second_pack)
            root = world.inside('explicit-native-root')
            root.mkdir()
            command = [sys.executable, '-I', '-B', first_pack / 'scripts/installer.py']
            arguments = ['install', '--presets', 'omp', '--omp-agent-dir', root,
                         '--omp-peers-upgraded', '--bin-dir', world.inside('bin'), '--mode', 'copy']
            pending = world.run([*command, *arguments, '--development-source', first_pack],
                                cwd=repo, check=False, timeout=60,
                                env={'AGENT_MEMORY_FAULT_STAGE': 'install-before-stored-source-activation',
                                     'AGENT_MEMORY_FAULT_MODE': 'error'})
            self.assertEqual(pending.returncode, 1, pending.stdout + pending.stderr)
            self.assertTrue(json.loads(pending.stdout)['receipt_committed'])
            receipt_path = owned_receipt_path(world)
            committed = receipt_path.read_bytes()
            with patch.dict(os.environ, world.env, clear=True):
                marker = omp_boundary._base(repo) / 'turn-sources-compat-v1.json'
            self.assertFalse(marker.exists())
            # A distinct source plan creates a real later transaction without
            # modifying either the active runtime or its retained source.
            failed = world.run([*command, *arguments, '--development-source', second_pack],
                               cwd=repo, check=False, timeout=60,
                               env={'AGENT_MEMORY_FAULT_STAGE': 'install-before-receipt',
                                    'AGENT_MEMORY_FAULT_MODE': 'error'})
            self.assertEqual(failed.returncode, 1, failed.stdout + failed.stderr)
            self.assertEqual(receipt_path.read_bytes(), committed)
            self.assertFalse(marker.exists())
            recovered = json.loads(world.run([*command, 'recover'], cwd=repo, timeout=60).stdout)
            self.assertTrue(recovered['verified'], recovered)
            self.assertEqual(recovered['status'], 'rolled-back')
            self.assertEqual(recovered['stored_source_activation']['status'], 'activated')
            self.assertEqual(receipt_path.read_bytes(), committed)
            with patch.dict(os.environ, world.env, clear=True):
                self.assertEqual(omp_boundary.verify_marker(repo).root, repo)
            unchanged = json.loads(world.run([*command, *arguments, '--development-source', first_pack],
                                              cwd=repo, timeout=60).stdout)
            self.assertTrue(unchanged['verified'], unchanged)
            self.assertFalse(unchanged['changed'])
            self.assertEqual(receipt_path.read_bytes(), committed)

    def test_real_binding_proof_rejects_forgery_and_mid_instance_drift_before_source(self):
        child = r'''
import copy, json, pathlib, pickle, sys, uuid
sys.path.insert(0,sys.argv[1])
from agent_memory import installation, omp_boundary, omp_installation, omp_stream, omp_transport, turn_sources
from agent_memory.frontmatter import ContractError
repo=pathlib.Path(sys.argv[2])
request=json.loads(pathlib.Path(sys.argv[3]).read_bytes())
state,_=installation.locations()
receipt_path=state/'receipt.json'
original=receipt_path.read_bytes()
receipt=installation.read_receipt(state)
proof=omp_installation.verified_observer_binding(receipt['source']['artifact_digest'],
    receipt['omp_binding']['agent_dir'],receipt=receipt)
base=omp_boundary._base(repo)
omp_boundary.stream_startup(repo,request['session'],str(uuid.uuid4()),request['lifecycle'],receipt=receipt)
assert omp_boundary.verify_marker(repo).root==repo
assert omp_installation.verify_observer_binding(repo,receipt,proof).root==repo
started=[]
def refuse(operation):
    try:
        operation()
    except Exception:
        return
    raise AssertionError('Unqualified binding authorized the operation')
def observe(candidate,selected=receipt):
    return omp_transport.observe(repo,request,receipt=selected,verified_binding=candidate,
        on_started=lambda: started.append(True))
class Lookalike:
    _seal=True
class Subclass(omp_installation.ObserverBinding):
    pass
refuse(lambda: observe(Lookalike()))
refuse(lambda: observe(object.__new__(omp_installation.ObserverBinding)))
refuse(lambda: observe(object.__new__(Subclass)))
for field in ('installed_at','artifact_digest'):
    different=copy.deepcopy(receipt)
    if field=='artifact_digest':
        different['source'][field]='f'*64
    else:
        different[field]='2030-01-01T00:00:00+00:00'
    refuse(lambda: observe(proof,different))
for operation in (lambda:pickle.dumps(proof),lambda:copy.copy(proof),lambda:copy.deepcopy(proof)):
    refuse(operation)
assert not started and not (base/'omp-source-v1').exists()
class Recipient:
    live=True
    def __init__(self):self.rows=[]
    def send(self,kind,sequence=0,**value):
        self.rows.append((kind,sequence,value));return True
identity={'view_id':str(uuid.uuid4()),'leaf_id':request['payload']['view']['leaf_id'],
    'native_ids':sorted(row['id'] for row in request['payload']['view']['native_parents']),'tool_ids':[]}
def exercise_drift(kind,frame_kind):
    instance=str(uuid.uuid4())
    binding=omp_boundary.stream_startup(repo,request['session'],instance,request['lifecycle'],receipt=receipt)
    coverage=omp_stream.Coverage(str(repo),request['session'],instance,request['lifecycle'],binding,receipt=receipt)
    recipient=Recipient()
    clocks=omp_stream.ClockWindows(instance,lambda:True)
    executor=omp_stream.FiniteExecutor(coverage,recipient,clocks,receipt)
    hint_identity={'kind':'hint','session':request['session'],'header_id':request['session'],
        'cwd':str(repo),'leaf_id':None}
    hint={'schema':'agent-memory/omp-observer-request-v2','action':'hint',
        'session':request['session'],'lifecycle':request['lifecycle'],
        'payload':{key:value for key,value in hint_identity.items() if key!='kind'}}
    hint_envelope={'request':hint,'event_ordinal':0,'identity':hint_identity}
    executor.execute('REQUEST',1,None,turn_sources.encoded(hint_envelope))
    outcomes=sorted((coverage.directory/'outcomes').glob('*.json'))
    assert len(outcomes)==1 and json.loads(outcomes[0].read_bytes())['code']=='hint-recorded'
    accepted_before=list(recipient.rows)
    marker=base/'turn-sources-compat-v1.json'
    bank_binding=base/'omp-bank-binding-v1.json'
    path={'receipt':receipt_path,'marker':marker,'bank-binding':bank_binding}[kind]
    before=path.read_bytes()
    value=json.loads(before)
    if kind=='receipt':value['installed_at']='2030-01-01T00:00:00+00:00'
    elif kind=='marker':value['minimum_reader']='future-reader'
    else:value['project_id']=str(uuid.uuid4())
    path.write_bytes(turn_sources.encoded(value))
    try:
        refuse(lambda:omp_installation.verify_observer_binding(repo,receipt,proof))
        envelope=({'request':request,'event_ordinal':request['payload']['event_ordinal'],'identity':identity}
                  if frame_kind=='source' else hint_envelope)
        refuse(lambda:executor.execute('REQUEST',2,None,turn_sources.encoded(envelope)))
        later_responses=recipient.rows[len(accepted_before):]
        assert not any(row[2].get('result',{}).get('verified') is True
                       or row[2].get('code')=='hint-recorded' for row in later_responses)
        for outcome in (coverage.directory/'outcomes').glob('*.json'):
            if outcome not in outcomes:
                assert json.loads(outcome.read_bytes())['outcome']!='verified'
        assert not list((coverage.directory/'started').glob('*.json'))
        assert not (coverage.directory/'closed.json').exists()
        assert not (base/'omp-source-v1').exists()
    finally:
        path.write_bytes(before)
        coverage.close_live()
    current=omp_stream.inventory(repo)
    actual=next(row for row in current['streams'] if row['instance_id']==instance)
    assert {'code':'open-stream'} in actual['pending'] and not actual['closed']
    assert original==receipt_path.read_bytes()
for kind in ('receipt','marker','bank-binding'):
    for frame_kind in ('source','hint'):
        exercise_drift(kind,frame_kind)
assert not started and not (base/'omp-source-v1').exists()
request=copy.deepcopy(request)
request['session']=str(uuid.uuid4())
request['lifecycle']=str(uuid.uuid4())
request['payload']['view']['session']=request['session']
request['payload']['view']['header']['id']=request['session']
request['action']='seal'
request['payload']['request_wake']=False
observed=observe(proof)
assert observed['verified'] is True and started==[True]
rows,faults=omp_transport.pending_inventory(repo)
assert faults==[]
retained=[]
for row in rows:
    if row.get('session')==request['session'] and 'observation_id' in row:
        result=omp_transport.source(repo,row['session'],row['lifecycle'],row['observation_id'])
        if 'content' in result.get('source',{}):
            retained.append(result['source']['content'])
assert retained==['Synthetic proof consumer source.']
print(json.dumps({'forged_proofs_refused':True,'serialization_refused':True,
    'receipt_marker_bank_drift_refused':True,'valid_source_retained':True,'streams_pending':True}))
'''
        with World() as world:
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            repo, _ = test_admission.AdmissionContract().fixture(world)
            pack = world.inside('proof-runtime-source')
            shutil.copytree(RUNTIME.parent, pack)
            root = world.inside('explicit-proof-native-root')
            root.mkdir()
            command = [sys.executable, '-I', '-B', pack / 'scripts/installer.py']
            installed = json.loads(world.run([*command, 'install', '--development-source', pack,
                '--presets', 'omp', '--omp-agent-dir', root, '--omp-peers-upgraded',
                '--mode', 'copy', '--bin-dir', world.inside('proof-bin')], cwd=repo, timeout=60).stdout)
            self.assertTrue(installed['verified'], installed)
            from contracts import test_omp_boundary
            import uuid
            session, lifecycle = str(uuid.uuid4()), str(uuid.uuid4())
            entry = {'type':'message','id':'proof-human','parentId':None,
                     'message':{'role':'user','attribution':'user','content':'Synthetic proof consumer source.'}}
            request = {'schema':'agent-memory/omp-observer-request-v2','action':'ingest',
                       'session':session,'lifecycle':lifecycle,
                       'payload':test_omp_boundary.observer_payload(repo,session,[entry])}
            input_path = world.inside('proof-request.json')
            input_path.write_text(json.dumps(request))
            result = world.run([sys.executable, '-I', '-B', '-c', child,
                                installed['receipt']['source']['pack'] + '/scripts', repo, input_path],
                               cwd=repo, check=False, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
