"""Installed task filtering and historical evidence across independent clones."""
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import unittest
from urllib.parse import quote

from world import World
import installed_cli_fixtures as installed
from contracts import test_checkpoint as checkpoint
from contracts import test_retrieval as retrieval
from contracts import test_assembly as assembly
from contracts import test_shared_learning_loop as shared


class PortableHandoff(unittest.TestCase):
    def good(self, w, cli, *args):
        r, value = installed.command(w, cli, *args)
        self.assertEqual(r.returncode, 0, value or r.stderr)
        return value

    def helper(self, cli):
        h = checkpoint.CheckpointContract()
        h.command = lambda world, name, *args, **kw: installed.invoke(world, cli, name, *args, **kw)
        return h

    def request(self, project, **context):
        return retrieval.RetrievalContract().request(project, query='', paths=['activeContext.md'],
            kinds=[], include_context=True, context=context)

    def test_published_handoff_filters_other_task_and_branch_but_remains_explicitly_inspectable(self):
        with World() as w:
            cli = installed.install(w)
            f = shared.SharedLearningLoopContract()
            f.call = lambda world, name, *args: installed.invoke(world, cli, name, *args)
            repo, bank, seed, _ = f.fixture(w)
            policy = f.github(w, repo, bank, seed)
            remote = Path(json.loads(policy.read_text())['remote'])
            receiver = w.clone(remote, 'independent receiver')
            w.git('remote', 'set-url', 'origin', 'https://github.com/fixture/memory.git', cwd=receiver)
            w.git('checkout', '-b', 'receiver-task', cwd=receiver)
            with f.actor_profile(w, 'receiver machine'):
                # Stable developer identity is created explicitly by the common fixture.
                from identity_fixtures import bind
                actor_b = bind(w, receiver)['developer_id']
            (bank / 'activeContext.md').write_text('# Session notes\n\nPreserve the original context.\n')
            local = repo / 'local-result.txt'
            local.write_text('Locally reviewed artifact, not published source.\n')
            outside = w.inside('outside-private-evidence.txt')
            outside.write_text('PRIVATE_OUTSIDE_BYTES_MUST_NOT_BE_HASHED\n')
            h = self.helper(cli)
            current = h.inspect(w, repo)
            payload = h.payload(current)
            payload.update(focus='Investigate task A against these qualified references.', conditions={
                'task': 'task-A', 'branch': 'feature-session'},
                evidence=['../code.py', '../local-result.txt', str(outside), 'https://example.invalid/evidence'])
            self.assertTrue(h.apply(w, h.preview(w, repo, payload))[1]['verified'])
            teacher = self.good(w, cli, 'identity', 'status', '--repo', repo)
            publication = f.prepared(w, repo)['request']
            self.assertTrue(f.land(w, repo, publication)['publication_verified'])
            before = f.state(w, receiver, receiver / 'memory-bank')
            with f.actor_profile(w, 'receiver machine'):
                request = self.request(teacher['project_id'], task='task-B')
                received = f.apply_refresh(w, f.prepared_refresh(w, receiver, recall=request))
                self.assertTrue(received['remote_freshness_verified'])
                self.assertFalse(received['feature_code_freshness_verified'])
                self.assertEqual(received['recall']['results'], [])
                self.assertEqual(received['recall']['scope_context']['developer_id'], actor_b)
                history = self.good(w, cli, 'inspect', '--repo', receiver, '--path', 'activeContext.md')
                self.assertEqual(history['inspection_class'], 'historical-orientation')
                self.assertFalse(history['assignment']); self.assertFalse(history['usable_as_current_guidance'])
                self.assertIn(teacher['developer_id'], history['body'])
                self.assertEqual(history['context_assessment']['conditions']['task'], 'task-A')
                private = next(row for row in history['context_assessment']['evidence'] if row['ref'] == str(outside))
                self.assertIsNone(private['recorded']['sha256']); self.assertIsNone(private['current']['sha256'])
                self.assertNotIn(hashlib.sha256(outside.read_bytes()).hexdigest(), json.dumps(history))
                missing = next(row for row in history['context_assessment']['evidence'] if row['ref'] == '../local-result.txt')
                self.assertEqual(missing['qualification'], 'unavailable-here')
                self.assertFalse((receiver / 'local-result.txt').exists())
                assembled = self.good(w, cli, 'context', 'preview', '--repo', receiver, '--request', h.save(w, {
                    'recall': request, 'max_bytes': 16384, 'token_budget': 16384, 'conflicts': []}))
                self.assertNotIn('Investigate task A', assembled['context_text'])
                self.assertEqual(before[1:], f.state(w, receiver, receiver / 'memory-bank')[1:])
                w.git('checkout', '-b', 'feature-session', cwd=receiver)
                request = self.request(teacher['project_id'], task='task-A')
                matching = self.good(w, cli, 'get', '--repo', receiver, '--request', h.save(w, request))
                row = matching['results'][0]
                self.assertEqual(row['selection_class'], 'orientation-only'); self.assertIsNone(row['trust'])
                self.assertFalse(row['assignment']); self.assertFalse(row['context_assessment']['evidence_bytes_match'])
                self.assertFalse(row['source_assertions_verified'])
            self.assertEqual(before[1], f.state(w, receiver, receiver / 'memory-bank')[1])

    def test_source_pins_refuse_preapply_and_locked_changes_and_outside_reference_never_opens(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(cli)
            repo, bank = h.fixture(w)
            source = repo / 'result.txt'; source.write_text('Observed result A\n')
            payload = h.payload(h.inspect(w, repo)); payload['evidence'] = ['../result.txt']
            proposal = h.preview(w, repo, payload); before = h.files(bank)
            source.write_text('Changed result B\n')
            self.assertNotEqual(h.apply(w, proposal)[0].returncode, 0)
            self.assertEqual(h.files(bank), before)
            proposal = h.preview(w, repo, payload)
            directory = w.inside('source rendezvous'); directory.mkdir()
            w.env.update(AGENT_MEMORY_FAULT_STAGE='before-recheck', AGENT_MEMORY_FAULT_MODE='pause',
                         AGENT_MEMORY_FAULT_DIR=str(directory))
            child = w.spawn([cli, '--json', 'checkpoint', 'apply', '--proposal', h.save(w, proposal)])
            ready, release = directory / 'before-recheck.ready', directory / 'before-recheck.release'
            deadline = time.monotonic() + 15
            while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(.01)
            self.assertTrue(ready.exists(), child.poll())
            source.write_text('Changed result C while bank locked\n'); release.touch()
            out, err = child.communicate(timeout=30)
            self.assertNotEqual(child.returncode, 0, out + err)
            self.assertEqual(h.files(bank), before)
            for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):
                w.env.pop(key)
            outside = w.inside('never-open-private.txt'); outside.write_text('PRIVATE_NEVER_OPEN_CANARY\n')
            payload['evidence'] = [str(outside), quote(str(outside), safe='')]
            input_path = h.save(w, payload)
            pack = Path(json.loads((w.paths['state'] / 'agent-memory/install/receipt.json').read_text())['source']['pack'])
            # Audit the actual installed Python entrypoint. No production-only
            # hook or monkeypatch grants the reference permission to read outside.
            program = ('import sys,runpy\n'
                'def audit(event,args):\n'
                ' if event=="open" and str(args[0]) in ' + repr([str(outside), outside.name]) + ': raise RuntimeError("outside reference opened")\n'
                'sys.addaudithook(audit)\nsys.argv=["am","--json","checkpoint","preview","--repo",' + repr(str(repo)) + ',"--input",' + repr(str(input_path)) + ']\n'
                'runpy.run_path(' + repr(str(pack / 'scripts/installer.py')) + ',run_name="__main__")\n')
            r = w.run([sys.executable, '-I', '-c', program], check=False)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertNotIn(hashlib.sha256(outside.read_bytes()).hexdigest(), r.stdout)
            self.assertIn('outside-uninspected', r.stdout)

    def test_preparation_recovery_retains_original_source_pins_and_can_rollback(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(cli)
            repo, bank = h.fixture(w, False)
            source = repo / 'result.txt'; source.write_text('Original source\n')
            payload = h.payload(h.inspect(w, repo)); payload['evidence'] = ['../result.txt']
            before = h.files(bank)
            prepared = h.good(w, 'checkpoint', 'prepare', '--repo', repo, '--input', h.save(w, payload))
            r, _ = h.command(w, 'bank_transactions', 'apply', '--proposal', h.save(w, prepared['transaction']),
                env={'AGENT_MEMORY_FAULT_STAGE': 'transaction-after-write:activeContext.md', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(r.returncode, 86)
            unlock = h.good(w, 'bank_writes', 'recover', '--repo', repo)
            h.good(w, 'bank_writes', 'recover', '--repo', repo, '--proposal', h.save(w, unlock))
            source.write_text('Changed source after partial preparation\n')
            self.assertNotEqual(h.command(w, 'bank_transactions', 'recover', '--repo', repo, '--mode', 'finish')[0].returncode, 0)
            rollback = h.good(w, 'bank_transactions', 'recover', '--repo', repo, '--mode', 'rollback')
            self.assertTrue(h.good(w, 'bank_transactions', 'recover', '--repo', repo, '--proposal', h.save(w, rollback))['verified'])
            self.assertEqual(h.files(bank), before)

    def test_unknown_conditions_malformed_framing_and_stale_evidence_stay_qualified(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(cli)
            repo, bank = h.fixture(w)
            source = repo / 'result.txt'; source.write_text('Actual source\n')
            value = h.payload(h.inspect(w, repo)); value.update(conditions={'task': None}, evidence=['../result.txt'])
            self.assertTrue(h.apply(w, h.preview(w, repo, value))[1]['verified'])
            project = self.good(w, cli, 'identity', 'status', '--repo', repo)['project_id']
            request = self.request(project, task='task-A')
            self.assertEqual(self.good(w, cli, 'get', '--repo', repo, '--request', h.save(w, request))['results'], [])
            history = self.good(w, cli, 'inspect', '--repo', repo, '--path', 'activeContext.md')
            self.assertFalse(history['usable_as_current_guidance'])
            self.assertIn('unknown-condition:task', history['context_assessment']['reasons'])
            value = h.payload(h.inspect(w, repo)); value.update(conditions={'task': 'task-A'}, evidence=['../result.txt'])
            self.assertTrue(h.apply(w, h.preview(w, repo, value))[1]['verified'])
            target = bank / 'activeContext.md'; original = target.read_text()
            target.write_text(original.replace('- ../result.txt\n', '- ../different-result.txt\n', 1))
            row = self.good(w, cli, 'get', '--repo', repo, '--request', h.save(w, request))['results'][0]
            self.assertEqual(row['context_assessment']['evidence_status'], 'historical-or-unqualified-evidence')
            target.write_text(original + '\n## Checkpoint conditions\n\n```json\n{}\n```\n')
            self.assertEqual(self.good(w, cli, 'get', '--repo', repo, '--request', h.save(w, request))['results'], [])
            self.assertEqual(self.good(w, cli, 'inspect', '--repo', repo, '--path', 'activeContext.md')['body'], target.read_text())
            target.write_text('# Manual project orientation\n\nAn unresolved question; no task is assigned.\n')
            row = self.good(w, cli, 'get', '--repo', repo, '--request', h.save(w, request))['results'][0]
            self.assertFalse(row['context_assessment']['managed_checkpoint']); self.assertFalse(row['assignment'])


if __name__ == '__main__':
    unittest.main()
