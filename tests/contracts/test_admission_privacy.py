"""Admission rejects publicly readable or redirected inputs without writes."""
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
from world import World

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'skills/agent-memory/scripts'))
from agent_memory.admission import PrivateInputError, read_input

class AdmissionPrivacyContract(unittest.TestCase):
    def test_private_input_and_refuse_readable_or_redirected(self):
        with World() as world:
            directory=world.inside('private-input');directory.mkdir(mode=0o700)
            path=directory/'input.json';path.write_text('{"value":1}');path.chmod(0o600)
            self.assertEqual(read_input(path), {'value':1})
            path.chmod(0o644)
            self.assertEqual(read_input(path), {'value':1})
            path.chmod(0o666)
            with self.assertRaises(PrivateInputError):read_input(path)
            path.chmod(0o600);link=directory/'link.json';link.symlink_to(path)
            with self.assertRaises(PrivateInputError):read_input(link)
            directory.chmod(0o755)
            with self.assertRaises(PrivateInputError):read_input(path)

    def test_replacement_during_read_is_refused(self):
        with World() as world:
            directory=world.inside('private-input');directory.mkdir(mode=0o700)
            path=directory/'input.json';path.write_text('{"value":1}');path.chmod(0o600)
            original_stat=os.stat; swapped=[False]
            def racing_stat(name, *args, **kwargs):
                if name == 'input.json' and not swapped[0]:
                    swapped[0]=True
                    path.unlink();path.write_text('{"value":2}');path.chmod(0o600)
                return original_stat(name, *args, **kwargs)
            with patch('agent_memory.admission.os.stat', side_effect=racing_stat):
                with self.assertRaises(PrivateInputError):read_input(path)
            self.assertTrue(swapped[0])

    def test_intermediate_symlink_is_refused(self):
        with World() as world:
            real=world.inside('real');real.mkdir(mode=0o700)
            private=real/'private';private.mkdir(mode=0o700)
            path=private/'input.json';path.write_text('{}');path.chmod(0o600)
            alias=world.inside('alias');alias.symlink_to(real, target_is_directory=True)
            with self.assertRaises(PrivateInputError):read_input(alias/'private/input.json')

    def test_public_cli_explains_private_workspace(self):
        with World() as world:
            repo=world.repository();directory=world.inside('public-input');directory.mkdir(mode=0o755);path=directory/'input.json';path.write_text('{}');path.chmod(0o644)
            proc=world.run([sys.executable,'-I',ROOT/'skills/agent-memory/scripts/installer.py',
                            'admission','draft','--repo',repo,'--input',path],check=False)
            value=json.loads(proc.stdout)
            self.assertNotEqual(proc.returncode,0)
            self.assertFalse(value['canonical_writes_performed'])
            self.assertIn('am private create',value['error'])
