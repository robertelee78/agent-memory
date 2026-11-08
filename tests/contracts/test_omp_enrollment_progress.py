"""Real transport/CLI regressions; not native loading or provider qualification."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

from world import World
from contracts import test_omp_boundary as fixtures

transport = fixtures.transport
turn_sources = fixtures.turn_sources
ROOT = Path(__file__).resolve().parents[2]


class EnrollmentProgress(unittest.TestCase):
    def rows(self, repo, expected_faults=()):
        rows, faults = transport.pending_inventory(repo)
        self.assertEqual(faults, list(expected_faults))
        self.assertTrue(all(row['source_reference'].startswith(('omp:capture:', 'omp:seal:')) for row in rows))
        return {row['source_reference']: row for row in rows}

    def test_bounded_later_sweeps_enroll_oldest_and_keep_every_new_arrival(self):
        with World() as world:
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('human-' + str(i), 'user', 'Ephemeral request ' + str(i),
                                    attribution='user') for i in range(5)]
            with patch.dict(os.environ, world.env, clear=True):
                first = helper.observe(repo, session, lifecycle, entries)
                initial = self.rows(repo)
                self.assertEqual(len(initial), 10)
                self.assertEqual(sum('observation_id' in row for row in initial.values()), 8)
                waiting = {ref for ref, row in initial.items() if 'observation_id' not in row}
                retained = {ref: row['source_sha256'] for ref, row in initial.items()}
                entries += [helper.entry('human-' + str(i), 'user', 'Ephemeral request ' + str(i),
                                         attribution='user') for i in range(5, 9)]
                second = helper.observe(repo, session, first['lifecycle'], entries)
                arrived = self.rows(repo)
                self.assertEqual(len(arrived), 18)
                self.assertEqual(sum('observation_id' in row for row in arrived.values()), 16)
                self.assertTrue(all('observation_id' in arrived[ref] for ref in waiting))
                self.assertEqual(retained, {ref: arrived[ref]['source_sha256'] for ref in initial})
                self.assertTrue(all(arrived[ref].get('observation_id') == row['observation_id']
                                    for ref, row in initial.items() if 'observation_id' in row))
                helper.observe(repo, session, second['lifecycle'], entries)
                finished = self.rows(repo)
                self.assertEqual(set(finished), set(arrived))
                self.assertTrue(all('observation_id' in row for row in finished.values()))
                self.assertEqual({ref: row['source_sha256'] for ref, row in arrived.items()},
                                 {ref: row['source_sha256'] for ref, row in finished.items()})

    def test_lost_success_reuses_observation_and_unknown_attempt_is_not_retried(self):
        # Construct only the interrupted-write boundary. Recovery itself must go
        # through the ordinary observer; this setup is not native-source proof.
        for successful in (False, True):
            with self.subTest(successful=successful), World() as world:
                helper = fixtures.NativeSources()
                repo, session, lifecycle = helper.fixture(world)
                with patch.dict(os.environ, world.env, clear=True):
                    directory, cursor = transport.new_segment(repo, session, lifecycle, lifecycle, 'human', 1)
                    captured = transport.capture(directory, cursor, helper.entry(
                        'human', 'user', 'Ephemeral interrupted source', attribution='user'))
                    transport.enroll_admission(directory, cursor, captured)
                    transport.seal(directory, cursor)
                    enrollment = next(transport.read(path, 4096) for path in
                                      (directory / 'enrollments').glob('*.json')
                                      if transport.read(path, 4096)['kind'] == 'admission')
                    attempt_path = directory / 'registration-attempts' / (transport.digest(enrollment['source_reference']) + '.json')
                    transport.retain(attempt_path, {
                        'schema': 'agent-memory/omp-registration-attempt-v1', 'session': session,
                        'lifecycle': lifecycle, 'source_reference': enrollment['source_reference'],
                        'source_sha256': enrollment['source_sha256']}, 4096)
                    original_attempt = attempt_path.read_bytes()
                    observation = None
                    if successful:
                        observation = turn_sources.register(repo, 'omp', session, lifecycle,
                            enrollment['source_sha256'], native_source_id=enrollment['native_source_id'],
                            source_reference=enrollment['source_reference'])
                    helper.observe(repo, session, lifecycle, [], seal=False)
                    recovered = self.rows(repo)[enrollment['source_reference']]
                    if successful:
                        self.assertEqual(recovered['observation_id'], observation['observation_id'])
                    else:
                        self.assertNotIn('observation_id', recovered)
                        self.assertEqual(recovered['faults'], ['registration-fault-retained'])
                    self.assertEqual(attempt_path.read_bytes(), original_attempt)
                    state = turn_sources.inspect(repo, 'omp', session, lifecycle)
                    matching = [source for source in state['sources']
                                if source['source_reference'] == enrollment['source_reference']]
                    self.assertEqual(len(matching), int(successful))
                    self.assertEqual(recovered['source_sha256'], enrollment['source_sha256'])

    def test_shared_fault_retained_across_later_observer_progress(self):
        with World() as world:
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('human', 'user', 'Ephemeral capacity fault', attribution='user')]
            with patch.dict(os.environ, world.env, clear=True):
                # Exercise the real shared source-capacity failure, not a mocked
                # registration result. Restoring capacity must not erase/retry it.
                with patch.object(turn_sources, 'MAX_SOURCES', 1):
                    result = helper.observe(repo, session, lifecycle, entries)
                directory, cursor = transport.segments(repo, session)[0]
                _, _, _, shared, _ = transport._shared(repo, cursor)
                fault_path = shared / 'fault.json'
                original_fault = fault_path.read_bytes()
                attempts = {path.name: path.read_bytes() for path in (directory / 'registration-attempts').glob('*.json')}
                retained = self.rows(repo, ['registration-fault-retained'])
                helper.observe(repo, session, result['lifecycle'], entries)
                retried = self.rows(repo, ['registration-fault-retained'])
                self.assertEqual(retried, retained)
                self.assertEqual(fault_path.read_bytes(), original_fault)
                self.assertEqual({path.name: path.read_bytes() for path in
                                  (directory / 'registration-attempts').glob('*.json')}, attempts)
                self.assertTrue(all(row['status'] == 'registration-fault' for row in retried.values()))
                self.assertEqual(len(turn_sources.inspect(repo, 'omp', session, cursor['lifecycle'])['sources']), 1)

    def test_public_observer_seal_and_no_learning_long_segment(self):
        with World() as world:
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            agent_dir = world.inside('explicit native root')
            agent_dir.mkdir()
            installed = json.loads(world.run([
                sys.executable, '-I', '-B', ROOT / 'skills/agent-memory/scripts/installer.py',
                'install', '--development-source', ROOT, '--presets', 'omp',
                '--omp-agent-dir', agent_dir, '--omp-peers-upgraded',
                '--bin-dir', world.inside('bin')], timeout=60).stdout)
            receipt = installed['receipt']
            am = Path(receipt['bin_dir']) / 'am'

            def public(*arguments, value=None):
                command = [str(am), '--json', 'omp-boundary']
                result = subprocess.run([*command, '--repo', str(repo), *map(str, arguments)],
                                        input=json.dumps(value) if value is not None else None,
                                        text=True, capture_output=True, env=world.env, cwd=repo, timeout=60)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return json.loads(result.stdout)

            bank = repo / 'memory-bank'
            before = {path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*') if path.is_file()}
            index = (repo / '.git/index').read_bytes()
            entries = [helper.entry('human', 'user', 'Compute only ephemeral arithmetic; no project learning.', attribution='user')]
            entries += [helper.entry('assistant-' + str(i), 'assistant', 'Ephemeral computation ' + str(i)) for i in range(150)]
            payload = fixtures.observer_payload(repo, session, entries, terminal=True)
            payload['request_wake'] = False
            start = time.monotonic()
            pack = Path(receipt['source']['pack'])
            completed = fixtures.observe_installed(world, am, repo, receipt, {
                'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal', 'session': session,
                'lifecycle': lifecycle, 'payload': payload}, launcher=[
                    '/bin/sh', pack / 'hooks/python3.sh', pack / 'scripts/omp_boundary.py'])
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            observed = json.loads(completed.stdout)
            elapsed = time.monotonic() - start
            self.assertTrue(observed['verified'])
            pending = public('--list-pending')
            self.assertFalse(pending['faults'])
            self.assertEqual(len(pending['pending']), 2)
            source_proofs = []
            for row in pending['pending']:
                self.assertIn('observation_id', row)
                source = public('--source', row['observation_id'], '--session', row['session'], '--lifecycle', row['lifecycle'])
                self.assertTrue(source['verified'])
                self.assertEqual(source['enrollment']['source_sha256'], row['source_sha256'])
                self.assertEqual(hashlib.sha256(turn_sources.encoded(source['source'])).hexdigest(), row['source_sha256'])
                source_proofs.append({'reference': row['source_reference'], 'observation_id': row['observation_id'],
                                      'source_sha256': row['source_sha256']})
                if row['source_reference'].startswith('omp:seal:'):
                    self.assertEqual(len(source['source']['captures']), len(entries))
                assessment = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
                assessment.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
                    'source_review': 'Read the public retained arithmetic-only source and its exact capture manifest; no durable project fact or instruction.',
                    'candidates': []})
                self.assertTrue(public('--stdin', value=assessment)['verified'])
            self.assertEqual(public('--list-pending')['pending'], [])
            self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*') if path.is_file()}, before)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            print(json.dumps({'proof': 'public-observer-and-no-learning', 'elapsed_seconds': elapsed,
                              'retained_entries': len(entries), 'sources': source_proofs,
                              'native_loading': 'not-exercised', 'model_provider': 'not-exercised'}, sort_keys=True))


if __name__ == '__main__':
    unittest.main()
