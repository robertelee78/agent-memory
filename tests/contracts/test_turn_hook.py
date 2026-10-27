"""Contract for non-blocking harness-boundary invocation guidance."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HOOK = ROOT / 'skills/agent-memory/hooks/turn_hook.py'


class TurnHookContract(unittest.TestCase):
    def call(self, data):
        result = subprocess.run([sys.executable, '-B', str(HOOK)], input=data,
                                capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_prompt_injects_shared_guidance_without_blocking(self):
        value = self.call(json.dumps({'hook_event_name': 'UserPromptSubmit',
                                      'session_id': 'fixture-session', 'cwd': '/tmp'}).encode())
        self.assertNotIn('decision', value)
        output = value['hookSpecificOutput']
        self.assertEqual(output['hookEventName'], 'UserPromptSubmit')
        prompt = output['additionalContext']
        self.assertIn('compare it with existing memory by meaning', prompt)
        self.assertIn('do not invent', prompt.lower())
        self.assertIn('pending', prompt)
        self.assertIn(str(HOOK.resolve().parent.parent / 'SKILL.md'), prompt)
        self.assertIn('Never directly edit or stage', prompt)
        self.assertIn('do not bypass permissions', prompt)

    def test_invalid_and_oversized_events_surface_pending_without_blocking(self):
        for raw in (b'bad json', b'{}', b'X' * (64 * 1024 + 1)):
            with self.subTest(length=len(raw)):
                value = self.call(raw)
                self.assertNotIn('decision', value)
                self.assertIn('pending', value['systemMessage'])

    @unittest.skipUnless(shutil.which('node'), 'JavaScript runtime unavailable')
    def test_opencode_adapters_inject_identical_context_once(self):
        directory = HOOK.parent
        source = r'''
import {pathToFileURL} from 'node:url';
const v1 = await import(pathToFileURL(process.argv[1]));
const v2 = await import(pathToFileURL(process.argv[2]));
const a = await v1.AgentMemoryBoundary();
const one = {system: []};
await a['experimental.chat.system.transform']({}, one);
await a['experimental.chat.system.transform']({}, one);
let callback, disposed = false;
const dispose = await v2.default.setup({session: {hook: async (_event, cb) => {
  callback = cb; return {dispose: () => { disposed = true }};
}}});
const two = {system: []}; callback(two); callback(two); dispose();
const failureDispose = await v2.default.setup({session: {hook: async () => { throw new Error('fixture'); }}});
failureDispose();
process.stdout.write(JSON.stringify({one: one.system, two: two.system, disposed}));
'''
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            for name in ('opencode-v1', 'opencode-v2'):
                shutil.copyfile(directory/(name+'.js'), target/(name+'.mjs'))
            shutil.copyfile(directory/'boundary-prompt.txt', target/'boundary-prompt.txt')
            result = subprocess.run([shutil.which('node'), '--input-type=module', '-e', source,
                                     str(target/'opencode-v1.mjs'), str(target/'opencode-v2.mjs')],
                                    capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(len(value['one']), 1)
        self.assertEqual(len(value['two']), 1)
        self.assertEqual(value['one'][0], value['two'][0]['text'])
        self.assertTrue(value['disposed'])
        self.assertIn('pending', result.stderr)


if __name__ == '__main__':
    unittest.main()
