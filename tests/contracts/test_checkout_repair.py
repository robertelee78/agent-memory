"""A previously adopted checkout survives device churn without changing memory."""
import json
import sys
import unittest
from pathlib import Path
from world import World
from contracts import test_admission


class CheckoutRepairContract(unittest.TestCase):
    def test_doctor_recognizes_and_persists_old_device_binding(self):
        with World() as w:
            original_repository = w.repository
            def repository(*args, **kwargs):
                repo = original_repository(*args, **kwargs)
                w.git('remote', 'add', 'origin', 'https://github.com/example/memory-fixture.git', cwd=repo)
                return repo
            w.repository = repository
            helper = test_admission.AdmissionContract()
            repo, bank = helper.fixture(w)
            registry = w.paths['config'] / 'agent-memory' / 'projects.json'
            # World overrides AGENT_MEMORY_CONFIG_HOME in some fixture versions.
            import os
            registry = Path(w.env.get('AGENT_MEMORY_CONFIG_HOME', str(registry.parent))) / 'projects.json'
            data = json.loads(registry.read_text())
            key = next(iter(data['checkouts']))
            device, inode = key.split(':')
            old = str(int(device) + 100) + ':' + inode
            data['checkouts'][old] = data['checkouts'].pop(key)
            registry.write_text(json.dumps(data))
            before = {str(p.relative_to(bank)): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            cli = helper.pack / 'scripts/installer.py'
            def call(*args):
                result = w.run([sys.executable, '-I', cli, *args, '--repo', repo, '--json'], check=False)
                return json.loads(result.stdout)
            observed = call('doctor')
            self.assertEqual(observed['project']['binding_source'], 'historical-device-change')
            self.assertNotIn(key, json.loads(registry.read_text())['checkouts'])
            repaired = call('doctor', '--fix')
            self.assertTrue(repaired['repairs_performed'], repaired.get('repairs'))
            self.assertIn(key, json.loads(registry.read_text())['checkouts'])
            self.assertIn(old, json.loads(registry.read_text())['checkouts'])
            self.assertFalse(call('doctor', '--fix')['repairs_performed'])
            self.assertEqual(before, {str(p.relative_to(bank)): p.read_bytes() for p in bank.rglob('*') if p.is_file()})

    def test_doctor_reports_prompt_and_stop_failures_independently(self):
        with World() as w:
            helper = test_admission.AdmissionContract()
            repo, _ = helper.fixture(w)
            script = "import sys;sys.path.insert(0,sys.argv[1]);from agent_memory import hook_failure as h;h.record(sys.argv[2],'codex','prompt','identity','identity.unresolved');h.record(sys.argv[2],'codex','stop','boundary','boundary.missing')"
            w.run([sys.executable, '-I', '-c', script, helper.pack / 'scripts', repo])
            result = w.run([sys.executable, '-I', helper.pack / 'scripts/installer.py', 'doctor', '--repo', repo, '--json'], check=False)
            value = json.loads(result.stdout)
            self.assertEqual(len(value['hook_failures']), 2)
            components = {x['component'] for x in value['warnings']}
            self.assertIn('hook.codex.prompt', components)
            self.assertIn('hook.codex.stop', components)
