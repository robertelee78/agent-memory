"""Real finite-consumer/canonical-writer ordering; not native qualification."""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest
import uuid
from unittest.mock import patch

from world import World
from contracts import test_admission, test_omp_boundary


# These launch the installed code and coordinate actual lock ownership; they do
# not replace any observer, policy, source or writer guard or inject exceptions.
OBSERVER_LAUNCHER = r'''
import os, pathlib, runpy, sys, time
installer=sys.argv.pop(1); am=sys.argv.pop(1); directory=pathlib.Path(os.environ['FENCE_TEST_DIR'])
def trace(frame,event,arg):
 if event=='call':
  return trace if frame.f_code.co_filename.endswith('/agent_memory/omp_stream.py') and frame.f_code.co_name=='on_started' else None
 if event=='return' and frame.f_locals.get('sequence')==1:
  (directory/'observer-started').touch()
  end=time.monotonic()+25
  while not (directory/'observer-release').exists():
   if time.monotonic()>=end: raise RuntimeError('Observer fixture release timed out')
   time.sleep(.01)
 return trace
sys.settrace(trace);sys.orig_argv=[sys.executable,am];sys.argv=[am]+sys.argv[1:]
runpy.run_path(installer,run_name='__main__')
'''
WRITER_LAUNCHER = r'''
import os, pathlib, runpy, sys, time
installer=sys.argv.pop(1); am=sys.argv.pop(1); directory=pathlib.Path(os.environ['FENCE_TEST_DIR'])
def trace(frame,event,arg):
 path=frame.f_code.co_filename; name=frame.f_code.co_name
 if event=='call':
  if path.endswith('/agent_memory/turn_sources.py') and name=='canonical_writer_barrier':
   (directory/'writer-entering').touch(); return trace
  if path.endswith('/agent_memory/local_registry.py') and name=='exclusive':
   if frame.f_locals.get('operation')=='bank-capture': (directory/'writer-entering').touch()
   return trace
  return None
 if event=='return' and name=='exclusive' and frame.f_locals.get('operation')=='bank-capture' and arg is not None:
  (directory/'writer-locked').touch()
  end=time.monotonic()+25
  while not (directory/'writer-release').exists():
   if time.monotonic()>=end: raise RuntimeError('Writer fixture release timed out')
   time.sleep(.01)
 return trace
sys.settrace(trace);sys.orig_argv=[sys.executable,am];sys.argv=[am]+sys.argv[1:]
runpy.run_path(installer,run_name='__main__')
'''

HOLD_SOURCE_BARRIER = r'''
import pathlib,sys,time
sys.path.insert(0,sys.argv[1])
from agent_memory.turn_sources import source_barrier
repo=sys.argv[2];directory=pathlib.Path(sys.argv[3])
with source_barrier(repo):
 (directory/'holder-ready').touch()
 end=time.monotonic()+55
 while not (directory/'holder-release').exists():
  if time.monotonic()>=end:raise RuntimeError('Source barrier holder release timed out')
  time.sleep(.01)
'''


class InstalledCanonicalFence(unittest.TestCase):
    def wait_path(self, path, seconds=20):
        deadline = time.monotonic() + seconds
        while not path.exists():
            if time.monotonic() >= deadline:
                self.fail('Timed out waiting for controlled boundary: ' + path.name)
            time.sleep(.01)

    def test_actual_capture_and_installed_observer_in_both_orders(self):
        from agent_memory import scope_binding
        for writer_first in (False, True):
            with self.subTest(writer_first=writer_first), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo, receipt, _ = test_omp_boundary.NativeSmokeQualification().fixture(world)
                control = world.inside('fence-control'); control.mkdir(mode=0o700)
                world.env['FENCE_TEST_DIR'] = str(control)
                contract = test_admission.AdmissionContract()
                contract.pack = world.inside('portable comparison pack')
                scope = scope_binding.require(repo)
                contract.pid, contract.actor = scope['project_id'], scope['developer_id']
                proposal = contract.prepared(world, repo)
                proposal_path = world.inside('fence-proposal.json')
                proposal_path.write_text(json.dumps(proposal)); proposal_path.chmod(0o600)
                observer_script = world.inside('observer-launcher.py'); observer_script.write_text(OBSERVER_LAUNCHER)
                writer_script = world.inside('writer-launcher.py'); writer_script.write_text(WRITER_LAUNCHER)
                am = Path(receipt['bin_dir']) / 'am'
                installer = Path(receipt['source']['pack']) / 'scripts/installer.py'
                session, lifecycle = str(uuid.uuid4()), str(uuid.uuid4())
                human = test_omp_boundary.NativeSources().entry('fence-human', 'user', 'Controlled governed source.', attribution='user')
                request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'ingest',
                           'session': session, 'lifecycle': lifecycle,
                           'payload': test_omp_boundary.observer_payload(repo, session, [human])}
                observed, errors = [], []
                def observe():
                    try:
                        observed.append(test_omp_boundary.observe_installed(world, am, repo, receipt, request,
                            launcher=[sys.executable, '-I', '-B', observer_script, installer, am, 'omp-boundary'], timeout=60))
                    except BaseException as exc:
                        errors.append(exc)
                observer = threading.Thread(target=observe, daemon=True)
                writer = None
                try:
                    if not writer_first:
                        observer.start(); self.wait_path(control/'observer-started')
                    writer = subprocess.Popen([sys.executable, '-I', '-B', str(writer_script), str(installer), str(am),
                        '--json', 'admission', 'apply', '--proposal', str(proposal_path)],
                        cwd=repo, env=world.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    self.wait_path(control/'writer-entering')
                    if writer_first:
                        self.wait_path(control/'writer-locked')
                        observer.start()
                        time.sleep(1)
                        self.assertFalse((control/'observer-started').exists(), 'Observer started while canonical writer held its fence')
                        (control/'writer-release').touch()
                        self.wait_path(control/'observer-started')
                        (control/'observer-release').touch()
                    else:
                        # The writer is at its actual gate. An unfenced baseline
                        # reaches writer-locked while the observer remains paused.
                        time.sleep(2)
                        self.assertFalse((control/'writer-locked').exists(), 'Canonical writer entered during a started observer')
                        (control/'observer-release').touch()
                        self.wait_path(control/'writer-locked')
                        (control/'writer-release').touch()
                    out, err = writer.communicate(timeout=35)
                    self.assertEqual(writer.returncode, 0, err.decode(errors='replace'))
                    self.assertTrue(json.loads(out)['admission_verified'])
                    observer.join(35)
                    self.assertFalse(observer.is_alive(), 'Finite observer did not terminate')
                    self.assertEqual(errors, [])
                    self.assertEqual(len(observed), 1)
                    self.assertEqual(observed[0].returncode, 0)
                    self.assertTrue(json.loads(observed[0].stdout)['verified'])
                finally:
                    (control/'observer-release').touch(); (control/'writer-release').touch()
                    if writer is not None:
                        try: writer.communicate(timeout=35)
                        except subprocess.TimeoutExpired:
                            writer.kill(); writer.communicate()
                    if observer.ident is not None:
                        observer.join(35)

    def prepare_capture(self, world):
        from agent_memory import scope_binding
        repo, receipt, _ = test_omp_boundary.NativeSmokeQualification().fixture(world)
        contract = test_admission.AdmissionContract()
        contract.pack = world.inside('portable comparison pack')
        scope = scope_binding.require(repo)
        contract.pid, contract.actor = scope['project_id'], scope['developer_id']
        proposal = contract.prepared(world, repo)
        path = world.inside('fence-proposal.json')
        path.write_text(json.dumps(proposal)); path.chmod(0o600)
        return repo, receipt, path

    def snapshot(self, world, repo):
        from agent_memory import layout_recovery_state
        bank = repo/'memory-bank'
        base = layout_recovery_state.paths(repo)[2].parent
        files = {str(path.relative_to(bank)): path.read_bytes()
                 for path in bank.rglob('*') if path.is_file()}
        records = {str(path.relative_to(base)): path.read_bytes()
                   for name in ('transactions', 'operations')
                   for path in (base/name).rglob('*') if path.is_file()}
        return files, records, (repo/'.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout

    def test_public_capture_exhausts_real_shared_barrier_without_canonical_write(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, receipt, proposal = self.prepare_capture(world)
            control = world.inside('held-barrier'); control.mkdir(mode=0o700)
            holder_script = world.inside('held-barrier.py'); holder_script.write_text(HOLD_SOURCE_BARRIER)
            pack = Path(receipt['source']['pack'])/'scripts'
            holder = subprocess.Popen([sys.executable, '-I', '-B', str(holder_script), str(pack),
                                       str(repo), str(control)], env=world.env, cwd=repo,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                self.wait_path(control/'holder-ready')
                before = self.snapshot(world, repo)
                started = time.monotonic()
                result = subprocess.run([str(Path(receipt['bin_dir'])/'am'), '--json', 'admission',
                                         'apply', '--proposal', str(proposal)], env=world.env,
                                        cwd=repo, capture_output=True, timeout=60)
                elapsed = time.monotonic() - started
                self.assertNotEqual(result.returncode, 0)
                refusal = json.loads(result.stdout)
                self.assertFalse(refusal['admission_verified'])
                self.assertFalse(refusal['canonical_writes_performed'])
                self.assertGreaterEqual(elapsed, 29.0)
                self.assertFalse((repo/'memory-bank/.agent-memory.lock').exists())
                self.assertEqual(self.snapshot(world, repo), before)
            finally:
                (control/'holder-release').touch()
                try:
                    _, errors = holder.communicate(timeout=20)
                except subprocess.TimeoutExpired:
                    holder.kill(); _, errors = holder.communicate()
                    self.fail('Source barrier holder failed to terminate')
                self.assertEqual(holder.returncode, 0, errors.decode(errors='replace'))

    def test_public_capture_refuses_unknown_marker_and_shared_upgrade(self):
        from agent_memory import layout_recovery_state, turn_sources
        from agent_memory.frontmatter import ContractError
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, receipt, proposal = self.prepare_capture(world)
            with turn_sources.source_barrier(repo):
                with self.assertRaises(ContractError):
                    with turn_sources.canonical_writer_barrier(repo):
                        self.fail('A shared barrier cannot be upgraded')
            marker = layout_recovery_state.paths(repo)[2].parent/'turn-sources-compat-v1.json'
            original = marker.read_bytes()
            invalid = b'{\"schema\":\"unknown\"}'
            marker.write_bytes(invalid)
            try:
                before = self.snapshot(world, repo)
                result = subprocess.run([str(Path(receipt['bin_dir'])/'am'), '--json', 'admission',
                                         'apply', '--proposal', str(proposal)], env=world.env,
                                        cwd=repo, capture_output=True, timeout=60)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(marker.read_bytes(), invalid)
                self.assertFalse((repo/'memory-bank/.agent-memory.lock').exists())
                self.assertEqual(self.snapshot(world, repo), before)
            finally:
                marker.write_bytes(original)
