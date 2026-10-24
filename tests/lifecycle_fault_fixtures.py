"""Enumerate real lifecycle writes, kill public commands, recover and retry.

Payload snapshots are logical writes: their unpublished file copies and fsyncs
are one bounded stage. Comparison preserves canonical/owned semantics, while
transaction names, timestamps and retained rollback archives may differ.
"""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from runtime_update_fixtures import ROOT, PACK, version, bank, snapshot, receipt_path


def invoke(world, argv, *, expected=0, env=None):
    value = world.run(argv, check=False, env=env, timeout=120)
    if value.returncode != expected:
        raise AssertionError((value.returncode, value.stdout, value.stderr))
    return json.loads(value.stdout) if value.stdout else None


def helper(operation, *args):
    name = 'uninstall_runtime.py' if operation in {'uninstall', 'purge-data'} else 'installer.py'
    return [sys.executable, '-I', ROOT / 'scripts' / name, operation, *args]


def setup(world, mode):
    channel = world.repository('fault channel with spaces')
    pack = channel / 'skills/agent-memory'; shutil.copytree(PACK, pack); version(pack, '1.0.0')
    world.git('add', '.', cwd=channel); world.git('commit', '-m', 'Actual disposable runtime', cwd=channel)
    (world.paths['codex'] / 'AGENTS.md').write_text('Keep original developer instructions.\n')
    memory = bank(world, world.paths['repo'])
    argv = helper('install', '--remote', channel, '--ref', 'main', '--mode', mode, '--presets', 'codex')
    return argv, memory


def read_trace(path):
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    assert rows and len(rows) % 2 == 0
    for step in range(len(rows) // 2):
        left, right = rows[step * 2:step * 2 + 2]
        assert left['step'] == right['step'] == step
        assert left['kind'] == right['kind']
        assert left['phase'] == 'before' and right['phase'] == 'after'
    return rows


def semantics(receipt):
    """Keep real target identities, excluding private Git-clone metadata only."""
    return {key: receipt[key] for key in ('schema', 'mode', 'presets', 'custom_targets',
        'enabled_adapters', 'bin_dir', 'data_roots', 'memory_data_location', 'runtime_state')} | {
        'uninstalled': receipt.get('uninstalled', False),
        'source': {k: receipt['source'][k] for k in ('kind', 'path', 'pack', 'remote', 'ref',
                                                   'revision', 'dirty', 'version', 'artifact_digest')},
        'targets': [{k: v for k, v in target.items() if k in {'kind', 'path', 'artifact_digest', 'block_hash'}
                    or (k == 'identity' and target['kind'] != 'source')}
                    for target in receipt['targets']]}


def save_baseline(world):
    base = world.inside('baseline'); base.mkdir()
    parts = {name: world.paths[name] for name in ('data', 'state', 'home', 'codex')}
    for name, path in parts.items():
        shutil.copytree(path, base / name, symlinks=True)
    return base, parts


def restore(world, base, parts):
    for name, path in parts.items():
        shutil.rmtree(path); shutil.copytree(base / name, path, symlinks=True)
    # This is solely the public operation's disposable candidate input area.
    shutil.rmtree(world.paths['tmp']); world.paths['tmp'].mkdir(mode=0o700)


class LifecycleWriteCases:
    def replay(self, operation, mode, phase):
        with World() as world:
            install_argv, memory = setup(world, mode)
            memory_before = snapshot(memory)
            if operation == 'install':
                argv = install_argv
            else:
                installed = invoke(world, install_argv)
                self.assertTrue(installed['verified'])
                if operation == 'uninstall':
                    argv = helper('uninstall')
                elif operation == 'recover-rollback':
                    # A real incomplete removal requires actual target rollback.
                    invoke(world, helper('uninstall'), expected=86, env={
                        'AGENT_MEMORY_FAULT_STAGE': 'uninstall-after-activate-0',
                        'AGENT_MEMORY_FAULT_MODE': 'exit'})
                    argv = helper('recover')
                elif operation == 'recover-commit':
                    invoke(world, helper('uninstall'), expected=86, env={
                        'AGENT_MEMORY_FAULT_STAGE': 'uninstall-after-receipt',
                        'AGENT_MEMORY_FAULT_MODE': 'exit'})
                    argv = helper('recover')
                else:
                    raise AssertionError(operation)
            baseline_receipt = (json.loads(receipt_path(world).read_text())
                                if receipt_path(world).exists() else None)
            base, parts = save_baseline(world)
            clean_path = world.inside('clean-trace.jsonl')
            clean = invoke(world, argv, env={'AGENT_MEMORY_FAULT_TRACE': str(clean_path)})
            self.assertTrue(clean['verified'])
            expected = json.loads(receipt_path(world).read_text())
            expected_semantics = semantics(expected)
            if expected.get('uninstalled'):
                self.assertFalse(Path(expected['source']['path']).exists())
            else:
                self.assertTrue(Path(expected['source']['pack']).is_dir())
            expected_nodes = {t['path']: snapshot(Path(t['path'])) for t in expected['targets']
                              if t['kind'] != 'source' or expected.get('uninstalled')}
            clean_rows = read_trace(clean_path)
            selected = [row for row in clean_rows if row['phase'] == phase]
            kinds = {row['kind'] for row in clean_rows}
            self.assertTrue({'file-stage', 'record-publication'} <= kinds)
            if operation in {'install', 'uninstall', 'recover-rollback'}:
                self.assertIn('atomic-node-switch', kinds)
            if operation == 'install':
                self.assertTrue({'state-parent-directory', 'target-parent-directory', 'payload-snapshot'} <= kinds)
            executed = []
            for event in selected:
                with self.subTest(point=event['point'], kind=event['kind']):
                    restore(world, base, parts)
                    trace = world.inside('actual-trace.jsonl'); trace.unlink(missing_ok=True)
                    invoke(world, argv, expected=86, env={
                        'AGENT_MEMORY_FAULT_STAGE': event['point'], 'AGENT_MEMORY_FAULT_MODE': 'exit',
                        'AGENT_MEMORY_FAULT_TRACE': str(trace)})
                    observed = [json.loads(line) for line in trace.read_text().splitlines()]
                    self.assertEqual(observed[-1], event)
                    recovered = invoke(world, helper('recover'))
                    self.assertTrue(recovered['verified'])
                    retried = invoke(world, argv); self.assertTrue(retried['verified'])
                    current = json.loads(receipt_path(world).read_text())
                    self.assertEqual(semantics(current), expected_semantics)
                    self.assertEqual({p: snapshot(Path(p)) for p in expected_nodes}, expected_nodes)
                    if baseline_receipt:
                        self.assertEqual(current['installation_id'], baseline_receipt['installation_id'])
                    self.assertEqual(snapshot(memory), memory_before)
                    # Verify actual ownership, source and targets with the public
                    # helper, even when removal deleted the installed command.
                    healthy = invoke(world, [sys.executable, '-I', ROOT / 'scripts/diagnose.py', 'doctor',
                                             '--repo', world.paths['repo']])
                    self.assertTrue(healthy['ok'])
                    stable = receipt_path(world).read_bytes()
                    self.assertTrue(invoke(world, argv)['verified'])
                    self.assertEqual(receipt_path(world).read_bytes(), stable)
                    self.assertEqual(snapshot(memory), memory_before)
                    row = {'operation': operation, 'mode': mode, 'point': event['point'],
                           'kind': event['kind'], 'interrupted_exit': 86, 'recovery_verified': True,
                           'retry_matches_clean_semantics': True, 'repeat_receipt_identical': True,
                           'physical_targets_match_clean': True,
                           'bank_unchanged': True}
                    sys.stderr.write(json.dumps(row) + '\n'); executed.append(event['point'])
            self.assertEqual(executed, [event['point'] for event in selected])
            self.assertGreater(len(executed), 0)
            sys.stderr.write(json.dumps({'operation': operation, 'mode': mode, 'phase': phase,
                'observed_write_points': len(selected), 'actual_interruptions_and_retries': len(executed),
                'kinds': sorted(kinds)}) + '\n')


class PurgeWriteCases:
    def fixture(self, world):
        memory = bank(world, world.paths['repo'])
        (memory / 'alias.md').hardlink_to(memory / 'projectbrief.md')
        generated = memory / '.agent-memory-cache'; generated.mkdir()
        (generated / 'references.json').write_text('{"derived":true}\n')
        check = helper('purge-data', '--repo', world.paths['repo'], '--path', memory, '--check')
        preview = invoke(world, check)['preview']
        argv = helper('purge-data', '--repo', world.paths['repo'], '--path', memory,
                      '--confirm', preview['confirmation'])
        return memory, preview, argv

    def replay_purge(self, phase):
        with World() as world:
            memory, preview, argv = self.fixture(world)
            trace = world.inside('clean-trace.jsonl')
            clean = invoke(world, argv, env={'AGENT_MEMORY_FAULT_TRACE': str(trace)})
            self.assertTrue(clean['verified']); self.assertFalse(memory.exists())
            rows = read_trace(trace)
            selected = [row for row in rows if row['phase'] == phase]
            kinds = {row['kind'] for row in rows}
            self.assertTrue({'purge-entry-remove', 'purge-lock-remove', 'purge-root-remove',
                             'purge-bank-lock', 'record-publication', 'atomic-node-switch'} <= kinds)
        executed = []
        for event in selected:
            with self.subTest(point=event['point'], kind=event['kind']), World() as world:
                memory, preview, argv = self.fixture(world)
                retained = {str(p): snapshot(p) for p in (world.paths['repo'] / '.git', world.paths['config'],
                    world.paths['data'], world.paths['home'], world.paths['backups'])}
                trace = world.inside('actual-trace.jsonl')
                invoke(world, argv, expected=86, env={'AGENT_MEMORY_FAULT_STAGE': event['point'],
                    'AGENT_MEMORY_FAULT_MODE': 'exit', 'AGENT_MEMORY_FAULT_TRACE': str(trace)})
                observed = [json.loads(line) for line in trace.read_text().splitlines()]
                self.assertEqual(observed[-1], event)
                # Data deletion resumes only this exact already-approved World
                # preview, never a fresh authorization from runtime recovery.
                completed = invoke(world, argv); self.assertTrue(completed['verified'])
                receipt = completed['receipt']
                self.assertEqual(receipt['preview'], preview)
                self.assertEqual(receipt['status'], clean['receipt']['status'])
                self.assertTrue(receipt['deletion_readback_verified'])
                self.assertFalse(memory.exists()); self.assertFalse(Path(receipt['stage']).exists())
                self.assertFalse(completed['replacement_bank_present'])
                self.assertFalse(invoke(world, argv)['changed'])
                self.assertEqual({p: snapshot(Path(p)) for p in retained}, retained)
                sys.stderr.write(json.dumps({'operation': 'purge', 'point': event['point'],
                    'kind': event['kind'], 'interrupted_exit': 86, 'same_confirmation_retry_verified': True,
                    'clean_deletion_outcome': True, 'retained_paths_unchanged': True}) + '\n')
                executed.append(event['point'])
        self.assertEqual(executed, [event['point'] for event in selected])
        self.assertGreater(len(executed), 0)
        sys.stderr.write(json.dumps({'operation': 'purge', 'phase': phase,
            'observed_write_points': len(selected), 'actual_interruptions_and_retries': len(executed),
            'kinds': sorted(kinds)}) + '\n')
