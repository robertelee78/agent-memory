"""Private workspace keeps explicit inputs outside the bank and refuses overwrite."""
import json
import os
import stat
import subprocess
import sys
import unittest
from pathlib import Path
from world import World
from contracts import test_admission


class PrivateWorkspaceContract(unittest.TestCase):
    def test_explicit_private_input_and_refusals(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            command=[sys.executable,'-I',helper.pack/'scripts/installer.py','private']
            before=helper.state(w,repo,bank)
            response=w.run([*command,'create','--repo',repo]);created=json.loads(response.stdout)
            path=Path(created['path']);self.assertEqual(stat.S_IMODE(path.stat().st_mode),0o700)
            self.assertFalse(path.is_relative_to(repo))
            raw=b'{"explicit":"authored"}'
            args=[*command,'put','--repo',repo,'--workspace',created['workspace'],'--name','request.json']
            response=subprocess.run([str(x) for x in args],input=raw,cwd=repo,env=w.env,capture_output=True)
            self.assertEqual(response.returncode,0,str(response.stderr+response.stdout))
            target=path/'request.json';self.assertEqual(target.read_bytes(),raw)
            self.assertEqual(stat.S_IMODE(target.stat().st_mode),0o600)
            self.assertNotEqual(subprocess.run([str(x) for x in args],input=b'{"changed":true}',cwd=repo,env=w.env,capture_output=True).returncode,0)
            self.assertEqual(target.read_bytes(),raw)
            self.assertNotEqual(subprocess.run([str(x) for x in [*command,'put','--repo',repo,'--workspace',created['workspace'],'--name','../bad.json']],input=raw,cwd=repo,env=w.env,capture_output=True).returncode,0)
            self.assertNotEqual(subprocess.run([str(x) for x in [*command,'put','--repo',repo,'--workspace',created['workspace'],'--name','dupe.json']],input=b'{"x":1,"x":2}',cwd=repo,env=w.env,capture_output=True).returncode,0)
            outside=w.inside('redirect-target.json');outside.write_text('untouched')
            (path/'link.json').symlink_to(outside)
            self.assertNotEqual(subprocess.run([str(x) for x in [*command,'put','--repo',repo,'--workspace',created['workspace'],'--name','link.json']],input=raw,cwd=repo,env=w.env,capture_output=True).returncode,0)
            self.assertEqual(outside.read_text(),'untouched')
            self.assertEqual(before,helper.state(w,repo,bank))
