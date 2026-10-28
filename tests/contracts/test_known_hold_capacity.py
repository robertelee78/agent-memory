"""Actual dispute successors preserve evidence at unchanged capacity boundaries.

Bulk private state exists only in disposable World profiles. Every template comes
from a real public source review; complete public manifests author the judgments.
The fixture reads full source rows/known descriptors rather than rebuilding the
same manifest once per disclosure page. This is mechanical, not model-quality QA.
"""
import copy
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys
import unittest

from world import World
from contracts import test_semantic_sharing as fixtures
from contracts import test_known_holds_v2 as hold_fixtures
from contracts.test_comparison import render

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'skills/agent-memory/scripts'))
from agent_memory import sharing_holds as holds
from agent_memory.frontmatter import ContractError


class KnownHoldCapacityContract(unittest.TestCase):
    inspect = hold_fixtures.KnownHoldsVersionTwoContract.inspect
    select = hold_fixtures.KnownHoldsVersionTwoContract.select

    def invoke(self, w, *args, expected=0):
        result = w.run([sys.executable, '-I', '-B', self.h.pack/'scripts/reconcile_memory.py', *args],
                       check=False, timeout=180)
        value = json.loads(result.stdout)
        if expected is not None:
            self.assertEqual(result.returncode, expected, (value, result.stderr))
        return value

    def judgment(self, view, resolutions=None):
        rows = {row['key']: row for row in view['sources']}
        value = {'schema': 'agent-memory/semantic-sharing-judgment-v2',
                 'request': view['request'], 'binding': view['binding'],
                 'manifest_sha256': view['manifest_sha256'], 'pages': [p['sha256'] for p in view['pages']],
                 'pairs': [{'left': a, 'right': b,
                            'classification': 'contradiction' if resolutions is None and {rows[a]['path'], rows[b]['path']} == self.claims else 'related',
                            'reason': 'This authored fixture retains the same unresolved room color claims.'}
                           for a, b in view['required_pairs']],
                 'dependencies': {}, 'consolidations': [],
                 'rationale': 'Inspect all manifest source rows and fixture-derived hold descriptors; retain unresolved claims.',
                 'recorded_at': datetime.now(timezone.utc).isoformat()}
        if resolutions is not None:
            value['resolutions'] = resolutions
        return value

    def review(self, w, repo, judgment, label, *, expected=0):
        request = w.inside(label+'-judgment.json'); request.write_text(json.dumps(judgment))
        output = w.inside(label+'-proof.json')
        value = self.invoke(w, 'review-v2', '--repo', repo, '--input', request, '--output', output, expected=expected)
        return json.loads(output.read_text()) if output.exists() else value

    def fixture(self, w, *, resolved=False):
        self.h = fixtures.SemanticSharingContract(); repo, bank, peer, _ = self.h.fixture(w)
        _, local = self.h.add(bank, 'wall-local', '# Atrium wall\n\nUse yellow.\n')
        _, landed = self.h.add(peer/'memory-bank', 'wall-peer', '# Atrium wall\n\nUse blue.\n')
        self.claims = {local, landed}; self.h.publish(w, peer)
        _, initial = self.inspect(w, repo, 'initial')
        _, view = self.inspect(w, repo, 'selected', self.select(initial))
        self.review(w, repo, self.judgment(view), 'initial')
        self.feedback = None
        if resolved:
            metadata, path = self.h.add(bank, 'clarification', '# Clarification\n\nThe detecting developer confirmed blue.\n')
            metadata['kind'] = 'user-feedback'; metadata['evidence'] = ['user:synthetic-blue-clarification']
            self.feedback = metadata, path
            (bank/path).write_text(render(metadata, '# Clarification\n\nThe detecting developer confirmed blue.\n'))
            view, resolutions = self.renewed_view(w, repo, 'clarified')
            self.review(w, repo, self.judgment(view, resolutions), 'clarified')
        self.registry = next(w.paths['state'].rglob('semantic-holds/*/registry.json'))
        self.backup = self.registry.with_name('recovery.json')
        self.baseline = json.loads(self.registry.read_text())
        self.project = self.baseline['project_id']; self.repo = repo
        return repo, bank

    def renewed_view(self, w, repo, label):
        _, initial = self.inspect(w, repo, label)
        request = self.select(initial)
        for selection in request['selections'].values():
            selection['paths'].append(self.feedback[1])
        _, view = self.inspect(w, repo, label+'-selected', request)
        evidence = next(row['key'] for row in view['sources'] if row['path'] == self.feedback[1])
        resolutions = {cause['id']: {'basis': 'operator-clarification', 'evidence': [evidence],
                                    'reason': 'Actual attributed fixture clarification resolves retained wall colors.'}
                       for cause in view['known_causes']}
        return view, resolutions

    def reseal(self, value):
        value.pop('checksum', None)
        return holds.seal(value)

    def bulk(self, count):
        value = copy.deepcopy(self.baseline); template = next(iter(value['records'].values()))
        value['records'] = {}
        for i in range(count):
            record = copy.deepcopy(template); old = record['cause']['sources']
            keys = [hashlib.sha256((key+':disposable-capacity-variant:'+str(i)).encode()).hexdigest() for key in old]
            record['cause']['sources'] = keys
            record['references'] = {b: record['references'][a] for a, b in zip(old, keys)}
            value['records'][holds.cause_id(record['cause'])] = record
        return self.reseal(value)

    def sized(self, value, size):
        value = copy.deepcopy(value); remaining = size-len(holds.encoded(value))
        self.assertGreaterEqual(remaining, 0)
        for record in value['records'].values():
            amount = min(remaining, 4096-len(record['cause']['reason'].encode()))
            record['cause']['reason'] += 'x'*amount; remaining -= amount
            if not remaining:
                break
        self.assertEqual(remaining, 0)
        value = self.reseal(value); self.assertEqual(len(holds.encoded(value)), size)
        return value

    def seed(self, value):
        holds.registry_value(value, self.repo, self.project)
        raw = holds.encoded(value); self.registry.write_bytes(raw); self.backup.write_bytes(raw)
        return raw

    def retained(self):
        directory = self.registry.parent.parent
        return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in directory.rglob('*') if p.is_file()}

    def frontier_judgment(self, w, repo, label):
        _, initial = self.inspect(w, repo, label)
        _, view = self.inspect(w, repo, label+'-selected', self.select(initial))
        return self.judgment(view)

    def refused_twice(self, w, repo, bank, judgment, label, code, amount, limit):
        before = self.h.state(w, repo, bank); retained = self.retained()
        for attempt in range(2):
            result = self.review(w, repo, judgment, label+'-'+str(attempt), expected=1)
            self.assertEqual(result['failure_code'], code, result)
            self.assertEqual(result['failure_component'], 'known-dispute-state', result)
            text = result.get('error', result.get('reason', ''))
            self.assertIn(str(amount), text); self.assertIn(str(limit), text)
            self.assertIn('cannot retain', result['repair'])
            self.assertIn('recovery and identical retries do not increase capacity', result['repair'])
            self.assertFalse(result['canonical_writes_performed']); self.assertFalse(result['publication_verified'])
            self.assertEqual(self.retained(), retained)
            self.assertEqual(self.h.state(w, repo, bank), before)

    def same_proof(self, w, repo, bank, reviewed, label):
        path = w.inside(label+'-checked-proof.json'); path.write_text(json.dumps(reviewed['proof']))
        before = self.h.state(w, repo, bank); retained = self.retained()
        result = self.invoke(w, 'check', '--v2', '--repo', repo, '--input', path)
        self.assertTrue(result['source_observation_current'])
        self.assertEqual(self.retained(), retained); self.assertEqual(self.h.state(w, repo, bank), before)

    def test_record_boundary_excess_and_retry(self):
        with World() as w:
            repo, bank = self.fixture(w)
            self.seed(self.bulk(4095)); before = self.h.state(w, repo, bank)
            reviewed = self.review(w, repo, self.frontier_judgment(w, repo, 'record-boundary'), 'record-boundary')
            self.assertEqual(len(json.loads(self.registry.read_text())['records']), 4096)
            self.assertEqual(self.h.state(w, repo, bank), before)
            self.same_proof(w, repo, bank, reviewed, 'record-boundary')
            self.seed(self.bulk(4096))
            judgment = self.frontier_judgment(w, repo, 'record-excess')
            self.refused_twice(w, repo, bank, judgment, 'record-excess', 'known-holds-record-capacity', 4097, 4096)

    def test_history_boundary_excess_and_retry(self):
        with World() as w:
            repo, bank = self.fixture(w, resolved=True)
            resolution = copy.deepcopy(next(iter(self.baseline['records'].values()))['resolution'])
            for count in (4095, 4096):
                seeded = copy.deepcopy(self.baseline)
                next(iter(seeded['records'].values()))['resolution_history'] = [copy.deepcopy(resolution) for _ in range(count)]
                self.seed(self.reseal(seeded)); metadata, path = self.feedback
                (bank/path).write_text(render(metadata, '# Clarification\n\nThe detecting developer specified blue; renewal '+str(count)+'.\n'))
                view, resolutions = self.renewed_view(w, repo, 'history-'+str(count))
                judgment = self.judgment(view, resolutions)
                if count == 4095:
                    before = self.h.state(w, repo, bank)
                    reviewed = self.review(w, repo, judgment, 'history-boundary')
                    record = next(iter(json.loads(self.registry.read_text())['records'].values()))
                    self.assertEqual(len(record['resolution_history']), 4096)
                    self.assertEqual(self.h.state(w, repo, bank), before)
                    self.same_proof(w, repo, bank, reviewed, 'history-boundary')
                else:
                    self.refused_twice(w, repo, bank, judgment, 'history-excess', 'known-holds-history-capacity', 4097, 4096)

    def test_sealed_byte_boundary_excess_and_retry(self):
        with World() as w:
            repo, bank = self.fixture(w)
            original = self.seed(self.bulk(1))
            self.review(w, repo, self.frontier_judgment(w, repo, 'measure'), 'measure')
            delta = len(self.registry.read_bytes())-len(original)
            self.assertGreater(delta, 0)
            for excess in (0, 1):
                self.seed(self.sized(self.bulk(4095), holds.MAX_STORE-delta+excess))
                judgment = self.frontier_judgment(w, repo, 'bytes-'+str(excess))
                if not excess:
                    before = self.h.state(w, repo, bank)
                    reviewed = self.review(w, repo, judgment, 'byte-boundary')
                    self.assertEqual(len(self.registry.read_bytes()), 16777216)
                    self.assertEqual(self.h.state(w, repo, bank), before)
                    self.same_proof(w, repo, bank, reviewed, 'byte-boundary')
                else:
                    self.refused_twice(w, repo, bank, judgment, 'byte-excess', 'known-holds-byte-capacity', 16777217, 16777216)

    def test_structure_precedes_capacity_and_combined_dimension_precedence(self):
        with World() as w:
            repo, bank = self.fixture(w, resolved=True)
            original = copy.deepcopy(next(iter(self.baseline['records'].values()))['resolution'])
            value = self.bulk(4097)
            record = next(iter(value['records'].values()))
            record['resolution_history'] = [copy.deepcopy(original) for _ in range(4097)]
            for row in value['records'].values():
                row['cause']['reason'] += 'x'*(4096-len(row['cause']['reason'].encode()))
            value = self.reseal(value)
            self.assertGreater(len(holds.encoded(value)), 16777216)
            retained = self.retained(); before = self.h.state(w, repo, bank)
            with self.assertRaises(holds.HoldFailure) as failure:
                holds.registry_value(value, repo, self.project, successor=True)
            self.assertEqual(failure.exception.code, 'known-holds-record-capacity')
            value['records'] = {next(iter(value['records'])): record}
            for entry in record['resolution_history']:
                entry['reason'] = 'x'*4096
            value = self.reseal(value)
            self.assertGreater(len(holds.encoded(value)), 16777216)
            with self.assertRaises(holds.HoldFailure) as failure:
                holds.registry_value(value, repo, self.project, successor=True)
            self.assertEqual(failure.exception.code, 'known-holds-history-capacity')
            for malformed in ('checksum', 'last-history-entry'):
                invalid = copy.deepcopy(value)
                if malformed == 'checksum': invalid['checksum'] = '0'*64
                else:
                    next(iter(invalid['records'].values()))['resolution_history'][-1]['unexpected'] = True
                    invalid = self.reseal(invalid)
                with self.assertRaises(ContractError) as failure:
                    holds.registry_value(invalid, repo, self.project, successor=True)
                self.assertNotIsInstance(failure.exception, holds.HoldFailure)
            with self.assertRaises(ContractError) as failure:
                holds.registry_value(value, repo, self.project)
            self.assertNotIsInstance(failure.exception, holds.HoldFailure)
            self.assertEqual(self.retained(), retained); self.assertEqual(self.h.state(w, repo, bank), before)

    def test_persisted_corruption_and_interruption_remain_noncapacity(self):
        with World() as w:
            repo, bank = self.fixture(w)
            valid = self.registry.read_bytes()
            for mode in ('checksum', 'oversized-persisted', 'interrupted'):
                self.registry.write_bytes(valid); self.backup.write_bytes(valid)
                journal = self.registry.with_name('journal.json')
                if mode == 'checksum':
                    value = json.loads(valid); value['checksum'] = '0'*64
                    raw = holds.encoded(value); self.registry.write_bytes(raw); self.backup.write_bytes(raw)
                elif mode == 'oversized-persisted':
                    raw = holds.encoded(self.bulk(4097)); self.registry.write_bytes(raw); self.backup.write_bytes(raw)
                else:
                    journal.write_text('{"retained-unknown-journal":true}'); journal.chmod(0o600)
                before = self.h.state(w, repo, bank); retained = self.retained()
                result = self.invoke(w, 'inspect-v2', '--repo', repo, '--output', w.inside(mode+'-inspection.json'), expected=1)
                self.assertNotIn(result['failure_code'], {'known-holds-record-capacity', 'known-holds-history-capacity', 'known-holds-byte-capacity'})
                if mode == 'interrupted': self.assertEqual(result['failure_code'], 'known-holds-interrupted')
                self.assertEqual(self.retained(), retained); self.assertEqual(self.h.state(w, repo, bank), before)
