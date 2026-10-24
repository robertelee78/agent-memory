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

    def test_qualified_recall_aliases_preserve_ids_conditions_validity_and_provenance(self):
        with World() as w:
            cli = installed.install(w, 'link')
            h = retrieval.RetrievalContract()
            h.command = lambda world, name, *args: installed.invoke(world, cli, name, *args)
            repo, memory, project = h.fixture(w)
            metadata, path = h.add(memory, project, 'qualified', conditions={'component':'app.py'})
            request = h.save(w, h.request(project, ids=[metadata['id']], query='', context={'component':'app.py'}))
            for group in ('search', 'get', 'explain'):
                result = self.good(w, cli, group, '--repo', repo, '--request', request, '--json')
                record = result['results'][0]['record']
                for key in ('id', 'version', 'conditions', 'origin', 'valid-from', 'valid-until', 'status'):
                    self.assertEqual(record[key], metadata[key])
                self.assertEqual(result['scope_context']['developer_id'], h.actor)
            mismatch = h.save(w, h.request(project, ids=[metadata['id']], query='', context={'component':'other.py'}))
            self.assertEqual(self.good(w, cli, 'get', '--repo', repo, '--request', mismatch)['results'], [])
            cli.unlink()
            # Plain canonical Markdown remains usable with the command removed.
            self.assertIn(path, (memory/'MEMORY.md').read_text())
            self.assertIn('verified retry probe', (memory/path).read_text())
            self.assertFalse(cli.exists())

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


class InstalledSharedLearningLoop(loop.SharedLearningLoopContract):
    def fixture(self, w):
        result = super().fixture(w)
        self.cli = installed.install(w)
        return result

    def call(self, w, helper, *args):
        return installed.invoke(w, self.cli, helper, *args)

    def setUp(self):
        # The loop's independent admission fixture must also use the installed
        # public surface rather than silently calling its copied source pack.
        outer = self
        def admission_command(_fixture, w, helper, *args):
            return outer.call(w, helper, *args)
        self.capture_patch = patch.object(admission.AdmissionContract, 'call', admission_command)
        self.capture_patch.start()
        self.addCleanup(self.capture_patch.stop)


if __name__ == '__main__':
    unittest.main()
