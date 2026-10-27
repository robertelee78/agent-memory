"""Private workspace keeps explicit inputs outside the bank and refuses overwrite."""
import json
import os
import stat
import subprocess
import sys
import unittest
from unittest import mock
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
            binding=path/'binding.json';original=binding.read_bytes()
            binding.unlink();binding.symlink_to(outside)
            self.assertNotEqual(subprocess.run([str(x) for x in [*command,'put','--repo',repo,'--workspace',created['workspace'],'--name','badbinding.json']],input=raw,cwd=repo,env=w.env,capture_output=True).returncode,0)
            binding.unlink();binding.write_bytes(b'x'*4097);binding.chmod(0o600)
            self.assertNotEqual(subprocess.run([str(x) for x in [*command,'put','--repo',repo,'--workspace',created['workspace'],'--name','oversize.json']],input=raw,cwd=repo,env=w.env,capture_output=True).returncode,0)
            binding.write_bytes(original);binding.chmod(0o600)
            sys.path.insert(0,str(helper.pack/'scripts'))
            try:
                from agent_memory import private_workspace as private
                real_write=private.write_fd
                def replace_directory(fd,name,data):
                    real_write(fd,name,data)
                    path.rename(path.with_name(path.name+'-moved'))
                    path.mkdir(mode=0o700)
                with mock.patch.dict(os.environ,w.env,clear=True), mock.patch.object(private,'write_fd',side_effect=replace_directory):
                    with self.assertRaises(Exception):private.put(repo,created['workspace'],'race.json',raw)
                self.assertFalse((path/'race.json').exists())
                self.assertEqual((path.with_name(path.name+'-moved')/'race.json').read_bytes(),raw)
            finally:
                sys.path.pop(0)
            self.assertEqual(before,helper.state(w,repo,bank))
