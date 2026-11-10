"""Installed finite outcome versus a real concurrent canonical capture."""
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
from contracts import test_omp_boundary, test_omp_writer_fence


OBSERVER = r'''
import os,pathlib,runpy,sys,time
installer=sys.argv.pop(1);am=sys.argv.pop(1);directory=pathlib.Path(os.environ['FENCE_TEST_DIR'])
def trace(frame,event,arg):
 if event=='call':
  if frame.f_code.co_filename.endswith('/agent_memory/omp_stream.py') and frame.f_code.co_name=='outcome' and frame.f_locals.get('outcome')=='verified':
   (directory/'outcome-entering').touch()
   end=time.monotonic()+20
   while not (directory/'writer-entering').exists():
    if time.monotonic()>=end:raise RuntimeError('Writer did not arrive at gate')
    time.sleep(.01)
   time.sleep(1)
   failed=[False]
   def outcome_trace(frame,event,arg):
    if event=='exception':failed[0]=True
    if event=='return' and not failed[0]:(directory/'outcome-recorded').touch()
    return outcome_trace
   return outcome_trace
 return None
sys.settrace(trace);sys.orig_argv=[sys.executable,am];sys.argv=[am]+sys.argv[1:]
runpy.run_path(installer,run_name='__main__')
'''

WRITER = r'''
import os,pathlib,runpy,sys,time
installer=sys.argv.pop(1);am=sys.argv.pop(1);directory=pathlib.Path(os.environ['FENCE_TEST_DIR'])
def trace(frame,event,arg):
 path=frame.f_code.co_filename;name=frame.f_code.co_name
 if event=='call':
  if path.endswith('/agent_memory/turn_sources.py') and name=='canonical_writer_barrier':
   (directory/'writer-entering').touch();return trace
  if path.endswith('/agent_memory/local_registry.py') and name=='exclusive':return trace
  return None
 if event=='return' and name=='exclusive' and frame.f_locals.get('operation')=='bank-capture' and arg is not None:
  (directory/'writer-locked').touch()
  end=time.monotonic()+33
  while not (directory/'outcome-recorded').exists() and time.monotonic()<end:time.sleep(.01)
 return trace
sys.settrace(trace);sys.orig_argv=[sys.executable,am];sys.argv=[am]+sys.argv[1:]
runpy.run_path(installer,run_name='__main__')
'''


class FiniteOutcomeFence(unittest.TestCase):
    def test_public_capture_cannot_interpose_before_verified_outcome(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            fence = test_omp_writer_fence.InstalledCanonicalFence()
            repo, receipt, proposal = fence.prepare_capture(world)
            control = world.inside('outcome-fence-control'); control.mkdir(mode=0o700)
            world.env['FENCE_TEST_DIR'] = str(control)
            observer_script = world.inside('outcome-observer.py'); observer_script.write_text(OBSERVER)
            writer_script = world.inside('outcome-writer.py'); writer_script.write_text(WRITER)
            am = Path(receipt['bin_dir'])/'am'
            installer = Path(receipt['source']['pack'])/'scripts/installer.py'
            session, lifecycle = str(uuid.uuid4()), str(uuid.uuid4())
            human = test_omp_boundary.NativeSources().entry('outcome-human', 'user', 'Controlled governed source.', attribution='user')
            request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'ingest',
                       'session': session, 'lifecycle': lifecycle,
                       'payload': test_omp_boundary.observer_payload(repo, session, [human])}
            observed, errors = [], []
            def observe():
                try:
                    observed.append(test_omp_boundary.observe_installed(world, am, repo, receipt, request,
                        launcher=[sys.executable, '-I', '-B', observer_script, installer, am, 'omp-boundary'], timeout=90))
                except BaseException as exc:
                    errors.append(exc)
            observer = threading.Thread(target=observe, daemon=True)
            writer = None
            try:
                observer.start(); fence.wait_path(control/'outcome-entering', 40)
                writer = subprocess.Popen([sys.executable, '-I', '-B', str(writer_script), str(installer), str(am),
                    '--json', 'admission', 'apply', '--proposal', str(proposal)], cwd=repo, env=world.env,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                fence.wait_path(control/'writer-entering')
                out, err = writer.communicate(timeout=60)
                observer.join(60)
                self.assertFalse(observer.is_alive(), 'Finite observer did not terminate')
                self.assertEqual(writer.returncode, 0, err.decode(errors='replace'))
                self.assertTrue(json.loads(out)['admission_verified'])
                self.assertTrue((control/'writer-locked').exists())
                self.assertTrue((control/'outcome-recorded').exists())
                self.assertEqual(errors, [])
                self.assertEqual(len(observed), 1)
                self.assertEqual(observed[0].returncode, 0, observed[0].stderr)
                self.assertTrue(json.loads(observed[0].stdout)['verified'])
            finally:
                (control/'outcome-recorded').touch()
                if writer is not None:
                    try: writer.communicate(timeout=40)
                    except subprocess.TimeoutExpired:
                        writer.kill(); writer.communicate()
                if observer.ident is not None: observer.join(40)
