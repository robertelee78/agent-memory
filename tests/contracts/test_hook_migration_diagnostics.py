"""Receipt-level hook migration warnings, without client or repository writes."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import diagnostics


class HookMigrationDiagnostics(unittest.TestCase):
    def test_missing_and_intentionally_disabled_hook_targets(self):
        report = diagnostics.Report('status')
        receipt = {'no_guidance': False, 'enabled_adapters': ['codex', 'claude', 'opencode'], 'targets': [], 'source': {'pack': '/private/tmp/pack with spaces'}}
        with patch.object(diagnostics.installation, 'hook_paths', lambda name: Path('/private/tmp') / name):
            diagnostics.hook_migration(report, receipt)
            warnings = report.finish()['warnings']
            self.assertEqual(len(warnings), 3)
            self.assertTrue(all('am install' in warning['repair'] for warning in warnings))
            report = diagnostics.Report('status')
            diagnostics.hook_migration(report, {**receipt, 'no_guidance': True})
            self.assertFalse(report.finish()['warnings'])
            codex_prompt, codex_stop = diagnostics.installation.client_hook_entries('codex', Path(receipt['source']['pack']))
            claude_prompt, claude_stop = diagnostics.installation.client_hook_entries('claude', Path(receipt['source']['pack']))
            receipt['targets'] = [
                {'path': '/private/tmp/codex', 'kind': 'hook-config', 'hook_entry': codex_prompt},
                {'path': '/private/tmp/claude', 'kind': 'hook-config', 'hook_entry': claude_prompt, 'stop_entry': claude_stop},
                {'path': '/private/tmp/opencode', 'kind': 'hook-link'},
            ]
            report = diagnostics.Report('status')
            diagnostics.hook_migration(report, receipt)
            self.assertEqual([v['component'] for v in report.finish()['warnings']], ['runtime.hook_migration.codex'])
            receipt['targets'][0]['stop_entry'] = codex_stop
            report = diagnostics.Report('status')
            diagnostics.hook_migration(report, receipt)
            self.assertFalse(report.finish()['warnings'])
            receipt['targets'][0]['hook_entry'] = {'hooks': [{'type': 'command', 'command': 'python3 old-hook.py'}]}
            report = diagnostics.Report('status')
            diagnostics.hook_migration(report, receipt)
            self.assertEqual([v['component'] for v in report.finish()['warnings']], ['runtime.hook_migration.codex'])
