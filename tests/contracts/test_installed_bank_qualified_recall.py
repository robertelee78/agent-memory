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



if __name__ == "__main__":
    unittest.main()
