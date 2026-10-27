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

    def test_opt_in_claude_stop_wakes_once_and_preserves_permissions(self):
        hook = HOOK.parent / 'claude_stop.py'
        def run(value):
            return subprocess.run([sys.executable, '-B', str(hook)], input=json.dumps(value).encode(),
                                  capture_output=True, timeout=5)
        guarded = run({'hook_event_name': 'Stop', 'stop_hook_active': True})
        self.assertEqual(guarded.returncode, 0)
        self.assertEqual(guarded.stderr, b'')
        malformed = run({'hook_event_name': 'Stop'})
        self.assertEqual(malformed.returncode, 0)
        self.assertIn('pending', json.loads(malformed.stdout)['systemMessage'])

    def test_invalid_and_oversized_events_surface_pending_without_blocking(self):
        for raw in (b'bad json', b'{}', b'X' * (64 * 1024 + 1)):
            with self.subTest(length=len(raw)):
                value = self.call(raw)
                self.assertNotIn('decision', value)
                self.assertIn('pending', value['systemMessage'])

    @unittest.skipUnless(shutil.which('node'), 'JavaScript runtime unavailable')
    def test_opencode_v2_exposes_only_observed_session_identity_during_active_queue(self):
        source = r'''
import {pathToFileURL} from 'node:url';
const module = await import(pathToFileURL(process.argv[1]));
const callbacks = [];
const ctx = {
  tool: {transform: async callback => {
    callback({namespace() {}, add() {}}); return {dispose() {}};
  }},
  session: {hook: async (event, callback) => {
    callbacks.push({event, callback}); return {dispose() {}};
  }},
};
const dispose = await module.default.setup(ctx);
const callback = callbacks.filter(item => item.event === 'context').at(-1).callback;
const good = {sessionID: 'ses_123abc_DEF', system: []};
callback(good); callback(good);
const invalid = {sessionID: 'ses_bad\nforged prompt', system: []};
callback(invalid);
const stale = {sessionID: 'ses_current', system: [{type:'text',text:'Agent Memory observed OpenCode session: ses_stale. foreign context'}]};
callback(stale); callback(stale); dispose();
process.stdout.write(JSON.stringify({good: good.system, invalid: invalid.system, stale: stale.system}));
'''
        result = subprocess.run([shutil.which('node'), '--input-type=module', '-e', source,
                                 str(HOOK.parent/'opencode-v2.js')],
                                capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(len(value['good']), 1)
        self.assertIn('ses_123abc_DEF', value['good'][0]['text'])
        self.assertIn('does not request foreground memory work', value['good'][0]['text'])
        self.assertEqual(len(value['invalid']), 1)
        self.assertIn('unavailable or invalid', value['invalid'][0]['text'])
        self.assertEqual(len(value['stale']), 2)
        self.assertIn('ses_stale', value['stale'][0]['text'])
        self.assertIn('ses_current', value['stale'][1]['text'])
        self.assertIn('do not record or acknowledge', value['stale'][1]['text'])

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
            target = Path(tmp) / "folder space $& apostrophe's" / 'hooks'
            target.mkdir(parents=True)
            for name in ('opencode-v1', 'opencode-v2'):
                shutil.copyfile(directory/(name+'.js'), target/(name+'.mjs'))
            shutil.copyfile(directory/'boundary-prompt.txt', target/'boundary-prompt.txt')
            shutil.copyfile(directory/'opencode-v2-queue.js', target/'opencode-v2-queue.js')
            shutil.copyfile(directory/'boundary_ledger.py', target/'boundary_ledger.py')
            (target/'package.json').write_text('{"type":"module"}')
            result = subprocess.run([shutil.which('node'), '--input-type=module', '-e', source,
                                     str(target/'opencode-v1.mjs'), str(target/'opencode-v2.mjs')],
                                    capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(len(value['one']), 1)
        self.assertEqual(len(value['two']), 1)
        self.assertEqual(value['one'][0], value['two'][0]['text'])
        self.assertIn(str(target.parent / 'SKILL.md'), value['one'][0])
        self.assertNotIn('{{SKILL_DIR}}', value['one'][0])
        self.assertTrue(value['disposed'])
        self.assertIn('pending', result.stderr)


if __name__ == '__main__':
    unittest.main()
