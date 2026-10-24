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


    def test_governance_alias_checks_intent_at_preview_and_apply_before_writes(self):
        with World() as w:
            cli = installed.install(w)
            h = governance.GovernanceContract()
            h.call = lambda world, name, *args: installed.invoke(world, cli, name, *args)
            repo, memory = h.fixture(w)
            old, path = h.add(memory, 'wrong')
            request = h.request(memory, old, path, 'revoke')
            before = h.state(w, repo, memory)
            saved = h.save(w, request)
            for group in ('correct', 'supersede', 'forget'):
                r, refused = installed.command(w, cli, group, 'preview', '--repo', repo, '--request', saved)
                self.assertEqual(r.returncode, 1, refused); self.assertEqual(h.state(w, repo, memory), before)
            proposal = self.good(w, cli, 'revoke', 'preview', '--repo', repo, '--request', saved)
            prepared = h.save(w, proposal['proposal'], 'revoke-proposal.json')
            r, refused = installed.command(w, cli, 'forget', 'apply', '--proposal', prepared)
            self.assertEqual(r.returncode, 1, refused); self.assertEqual(h.state(w, repo, memory), before)
            done = self.good(w, cli, 'revoke', 'apply', '--proposal', prepared)
            self.assertTrue(done['governance_verified'])
            current, _ = comparison.unpack((memory/path).read_text())
            self.assertEqual(current['status'], 'revoked')
            self.assertEqual(before[1:], h.state(w, repo, memory)[1:])



if __name__ == "__main__":
    unittest.main()
