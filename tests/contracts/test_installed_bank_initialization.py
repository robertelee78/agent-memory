"""Actual installed public bank operations, sharing and readable manual fallback.

Fixture source setup may construct an existing bank. All operations under test
run the installed wrapper; missing routes are failures, never source fallback.
"""
import unittest
from unittest.mock import patch

from world import World
import installed_cli_fixtures as installed
from contracts import test_initialization as initialization
from contracts import test_checkpoint as checkpoint
from contracts import test_retrieval as retrieval
from contracts import test_governance as governance
from contracts import test_comparison as comparison
from contracts import test_admission as admission
from contracts import test_shared_learning_loop as loop


class InstalledBankRoundtrip(unittest.TestCase):
    def good(self, w, cli, *args):
        r, value = installed.command(w, cli, *args)
        self.assertEqual(r.returncode, 0, value or r.stderr)
        return value


    def test_cold_identity_initialization_and_checkpoint_have_no_bank_dependency(self):
        with World() as w:
            cli = installed.install(w)
            h = initialization.InitializationContract()
            h.command = lambda world, name, *args, **kw: installed.invoke(world, cli, name, *args, **kw)
            repo, project, developer = h.setup(w)
            self.assertFalse((repo/'memory-bank').exists())
            observation = h.inventory(w, repo)
            proposal = h.preview(w, repo, h.review(observation, [
                h.claim('implementation', 3, 'src/app.py', context='techContext.md')]))
            r, receipt = h.apply(w, proposal)
            self.assertEqual(r.returncode, 0, receipt); self.assertTrue(receipt['verified'])
            identity = self.good(w, cli, 'identity', 'status', '--repo', repo, '--json')
            self.assertEqual(identity['project_id'], project); self.assertEqual(identity['developer_id'], developer)
            memory = repo/'memory-bank'; before = h.files(memory)
            c = checkpoint.CheckpointContract()
            c.command = h.command
            inspected = c.inspect(w, repo)
            prepared = c.preview(w, repo, c.payload(inspected))
            r, written = c.apply(w, prepared)
            self.assertEqual(r.returncode, 0, written); self.assertTrue(written['verified'])
            after = h.files(memory)
            self.assertEqual({k:v for k,v in before.items() if k!='activeContext.md'},
                             {k:v for k,v in after.items() if k!='activeContext.md'})
            self.assertFalse(written['publication_verified'])



if __name__ == "__main__":
    unittest.main()
