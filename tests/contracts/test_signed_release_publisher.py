"""Actual publisher commands, fixture signing and bounded offline service controls."""
import json
import os
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from signed_release_fixtures import SignedRelease
from signed_publisher_fixtures import SignedPublisher


class SignedPublisherContract(unittest.TestCase):
    def test_one_command_first_publish_exact_resume_and_harmless_descendant(self):
        for mode in ('normal','partial-upload','harmless-descendant'):
            with self.subTest(mode=mode), World() as w, SignedPublisher(w) as p:
                initial=p.prepare_publisher(mode)
                result=p.publish()
                if mode=='partial-upload':
                    self.assertEqual(result.returncode,1,result.stdout+result.stderr)
                    state=p.authority();self.assertTrue(state['release']['draft']);self.assertEqual(len(state['assets']),1)
                    result=p.publish()
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                value=json.loads(result.stdout);self.assertTrue(value['published']);self.assertTrue(value['default_activation_performed'])
                state=p.authority();self.assertEqual(state['created'],1);self.assertEqual(state['published'],1)
                self.assertEqual(len(state['assets']),4)
                expected=initial['foreign'] if mode=='harmless-descendant' else initial['source']
                self.assertEqual(value['verified_main_commit'],expected)
                repeated=p.publish();self.assertEqual(repeated.returncode,0,repeated.stdout+repeated.stderr)
                state=p.authority();self.assertEqual(state['created'],1);self.assertEqual(state['published'],1)

    def test_wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation(self):
        for mode in ('preexisting-tag','tag-before','tag-after','bootstrap-revert','runtime-revert','push-race'):
            with self.subTest(mode=mode), World() as w, SignedPublisher(w) as p:
                initial=p.prepare_publisher(mode);result=p.publish()
                self.assertEqual(result.returncode,1,result.stdout+result.stderr)
                self.assertNotIn('APPARATUS:',result.stderr)
                state=p.authority()
                main=w.run([state['git'],'--git-dir',state['bare'],'rev-parse','main']).stdout.strip()
                expected=initial['foreign'] if mode in ('bootstrap-revert','runtime-revert','push-race') else initial['base']
                self.assertEqual(main,expected)
                self.assertEqual(state['pushes'],1 if mode=='push-race' else 0)
                if mode=='preexisting-tag': self.assertIsNone(state['release'])
                elif mode=='tag-before': self.assertTrue(state['release']['draft'])
                else: self.assertFalse(state['release']['draft'])

    def test_unsafe_signing_key_paths_refuse_before_output_or_key_use(self):
        with World() as w, SignedRelease(w) as p:
            p.prepare()
            private=p.root/'skills/agent-memory/fixture-private';private.mkdir(mode=0o700)
            inside=private/'key';shutil.copyfile(p.key,inside);inside.chmod(0o600)
            # Fixture-only private material deliberately tracked in a clean
            # portable tree: cleanliness must not hide this custody violation.
            w.git('add','.',cwd=p.root);w.git('commit','-m','Fixture-only unsafe key location',cwd=p.root)
            symlink=w.inside('symlink-key');symlink.symlink_to(p.key)
            linked=w.inside('linked-key');os.link(p.key,linked)
            unsafe=w.inside('unsafe-key-parent');unsafe.mkdir(mode=0o755)
            unsafe_key=unsafe/'key';shutil.copyfile(p.key,unsafe_key);unsafe_key.chmod(0o600)
            for index,key in enumerate((inside,symlink,linked,unsafe_key)):
                with self.subTest(key=key.name):
                    output=w.inside('refused-output-'+str(index))
                    result=w.run([sys.executable,'-B',p.root/'scripts/distribution/release.py','build',
                                  '--private-key',key,'--output',output],check=False)
                    self.assertEqual(result.returncode,1,result.stdout+result.stderr)
                    self.assertNotIn('Release source must be clean',result.stderr)
                    self.assertFalse(output.exists())
