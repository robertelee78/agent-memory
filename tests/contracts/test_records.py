"""Public typed Markdown format, conditional recall and optional history loss."""
import copy
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / 'scripts/record_contract.py'
EXAMPLE = ROOT / 'skills/agent-memory/templates/record-proposal.example.json'


class RecordContract(unittest.TestCase):
    def proposal(self):
        value = json.loads(EXAMPLE.read_bytes())
        value['metadata']['id'] = str(uuid.uuid4())
        value['metadata']['project-id'] = str(uuid.uuid4())
        value['metadata']['origin']['developer-id'] = str(uuid.uuid4())
        value['metadata']['conditions']['revision'] = '1' * 40
        return value

    def command(self, w, operation, *, value=None, file=None, require=(), helper=HELPER):
        args = [sys.executable, '-I', helper, operation]
        if value is not None:
            source = w.inside('request.json')
            source.write_text(json.dumps(value, allow_nan=False))
            args += ['--input', source]
        if file is not None:
            args += ['--file', file]
        for field in require:
            args += ['--require', field]
        p = w.run(args, check=False)
        return p, json.loads(p.stdout)

    def roundtrip(self, w, proposal, require=(), helper=HELPER):
        p, output = self.command(w, 'render', value=proposal, require=require, helper=helper)
        self.assertEqual(p.returncode, 0, output)
        target = w.inside('record.md')
        target.write_text(output['content'])
        before = target.read_bytes()
        p, checked = self.command(w, 'validate', file=target, require=require, helper=helper)
        self.assertEqual(p.returncode, 0, checked)
        self.assertEqual(checked['metadata'], proposal['metadata'])
        self.assertEqual(checked['body'], proposal['body'])
        self.assertFalse(checked['writes_performed'])
        self.assertFalse(checked['sources_verified'])
        self.assertFalse(checked['admission_verified'])
        self.assertEqual(target.read_bytes(), before)
        return target, output

    def test_all_seven_kinds_roundtrip_with_origin_conditions_evidence_and_exact_times(self):
        with World() as w:
            p, definition = self.command(w, 'model')
            self.assertEqual(p.returncode, 0)
            kinds = definition['record']['properties']['kind']['enum']
            self.assertEqual(set(kinds), {'semantic-fact', 'user-feedback', 'project-decision', 'episodic-attempt',
                                         'result', 'reference', 'procedural-candidate'})
            for kind in kinds:
                value = self.proposal()
                value['metadata']['kind'] = kind
                value['metadata']['conditions'].update({'version': 'v0.1.0', 'session': 'origin-session'})
                value['metadata']['recorded-at'] = '2026-10-22T21:00:00.000000000000000001Z'
                value['metadata']['updated'] = '2026-10-22T14:00:00.000000000000000002-07:00'
                value['metadata']['derived-from'] = [{'id': str(uuid.uuid4()), 'version': 3}]
                self.roundtrip(w, value)
            self.assertFalse(Path(w.env['AGENT_MEMORY_STATE_HOME']).exists())

    def test_active_policy_and_stale_versions_remain_explicit_without_history_replay(self):
        with World() as w:
            value = self.proposal()
            meta = value['metadata']
            meta.update({'status': 'active', 'trust': 'reviewed', 'visibility': 'visible',
                         'admission-reason': 'Inspected the linked isolated probe', 'confidence': None,
                         'redaction': 'repository-reviewed'})
            self.roundtrip(w, value, require=('confidence', 'redaction'))
            for field in ('trust', 'visibility', 'admission-reason', 'confidence', 'redaction'):
                changed = copy.deepcopy(value)
                del changed['metadata'][field]
                p, output = self.command(w, 'render', value=changed, require=('confidence', 'redaction'))
                self.assertEqual(p.returncode, 1, field)
                self.assertFalse(output['writes_performed'])
            corrected = copy.deepcopy(value)
            corrected['metadata']['version'] = 2
            corrected['metadata']['corrects'] = [{'id': meta['id'], 'version': 1}]
            corrected['metadata']['evidence'].append('../evidence/new-probe.txt')
            corrected['body'] = '# Corrected conditional observation\n\nThe new probe returned four; see the retained original and new evidence.\n'
            self.roundtrip(w, corrected)
            self.assertEqual(corrected['metadata']['origin'], meta['origin'])
            for state in ('quarantined', 'superseded', 'revoked', 'expired'):
                old = copy.deepcopy(value)
                old['metadata'].update({'status': state, 'state-reason': 'Historical claim; do not use as current guidance'})
                if state == 'superseded':
                    old['metadata']['superseded-by'] = [{'id': meta['id'], 'version': 2}]
                target, _ = self.roundtrip(w, old)
                self.assertIn('Historical claim; do not use as current guidance', target.read_text())
                self.assertIn('status: "' + state + '"', target.read_text())
                del old['metadata']['state-reason']
                p, _ = self.command(w, 'render', value=old)
                self.assertEqual(p.returncode, 1)

    def test_malformed_identity_source_time_links_and_metadata_are_rejected_without_echo(self):
        with World() as w:
            value = self.proposal()
            mutations = [('id', 'not-a-uuid'), ('schema', 'agent-memory/record-v999'), ('version', True),
                         ('project-id', str(uuid.uuid1())), ('scope', 'personal:me'), ('kind', 'transcript'),
                         ('status', 'deleted'), ('source', {'actor': '', 'kind': 'test'}), ('evidence', []),
                         ('origin', {'developer-id': 'display-name'}), ('recorded-at', '2026-10-22'),
                         ('updated', '2026-10-22T20:59:59.999999999999999999Z'),
                         ('valid-from', 'not-a-time'), ('confidence', True), ('confidence', 1.1),
                         ('derived-from', [{'id': value['metadata']['id'], 'version': 1}]),
                         ('corrects', [{'id': str(uuid.uuid4()), 'version': 1}]), ('unexpected', 'SECRET_RECORD_CANARY')]
            for key, replacement in mutations:
                changed = copy.deepcopy(value)
                changed['metadata'][key] = replacement
                p, output = self.command(w, 'render', value=changed)
                self.assertEqual(p.returncode, 1, (key, replacement))
                self.assertFalse(output['valid'])
                self.assertNotIn('SECRET_RECORD_CANARY', p.stdout + p.stderr)
            for changes in ({'valid-from': '2026-10-22T21:00:00.000000000000000002Z',
                             'valid-until': '2026-10-22T21:00:00.000000000000000001Z'},
                            {'conditions': {}, 'scope': 'repository'}):
                changed = copy.deepcopy(value)
                changed['metadata'].update(changes)
                p, _ = self.command(w, 'render', value=changed)
                self.assertEqual(p.returncode, 1)
            target = w.inside('bad.md')
            for content in ('\ufeff---\nSECRET_RECORD_CANARY\n', '---\nid: SECRET_RECORD_CANARY\nid: duplicate\n---\nBody\n'):
                target.write_text(content)
                p, _ = self.command(w, 'validate', file=target)
                self.assertEqual(p.returncode, 1)
                self.assertNotIn('SECRET_RECORD_CANARY', p.stdout + p.stderr)

    def test_memory_landing_does_not_widen_unmerged_or_unknown_conditions(self):
        with World() as w:
            value = self.proposal()
            value['metadata']['conditions'].update({'version': 'v0.1.0', 'environment': 'test-machine', 'prerequisite': 'fixture-ready'})
            target, _ = self.roundtrip(w, value)
            context = {**value['metadata']['conditions'], 'component': 'src/retry/limit.py'}
            request = {'bank_project_id': value['metadata']['project-id'], 'context': context}
            p, output = self.command(w, 'applicability', file=target, value=request)
            self.assertEqual(p.returncode, 0, output)
            self.assertTrue(output['applicable'])
            for key in context:
                changed = copy.deepcopy(request)
                changed['context'].pop(key)
                p, output = self.command(w, 'applicability', file=target, value=changed)
                self.assertEqual(p.returncode, 0, output)
                self.assertFalse(output['applicable'], key)
            for key, observed in (('branch', 'main'), ('revision', '2' * 40), ('component', 'src/retry2/limit.py'), ('environment', 'other-machine')):
                changed = copy.deepcopy(request)
                changed['context'][key] = observed
                p, output = self.command(w, 'applicability', file=target, value=changed)
                self.assertEqual(p.returncode, 0, output)
                self.assertFalse(output['applicable'])
            unknown = copy.deepcopy(value)
            unknown['metadata']['applicability'] = 'unknown'
            target, _ = self.roundtrip(w, unknown)
            p, output = self.command(w, 'applicability', file=target, value=request)
            self.assertEqual(p.returncode, 0, output)
            self.assertFalse(output['applicable'])
            self.assertIn('unknown-applicability', output['reasons'])
            unknown['metadata']['applicability'] = 'known'
            unknown['metadata']['conditions']['prerequisite'] = None
            target, _ = self.roundtrip(w, unknown)
            p, output = self.command(w, 'applicability', file=target, value=request)
            self.assertEqual(p.returncode, 0, output)
            self.assertFalse(output['applicable'])
            self.assertIn('unknown-condition:prerequisite', output['reasons'])
            for route in ('revision:' + '1' * 40, 'version:v0.1.0', 'environment:test', 'prerequisite:ready'):
                changed = copy.deepcopy(value)
                changed['metadata']['scope'] = route
                p, _ = self.command(w, 'render', value=changed)
                self.assertEqual(p.returncode, 1, route)

    def test_optional_event_loss_preserves_readable_current_record_and_stale_state(self):
        with World() as w:
            repo = w.repository()
            value = self.proposal()
            value['metadata'].update({'status': 'revoked', 'state-reason': 'Probe disproved; excluded from current guidance'})
            target, rendered = self.roundtrip(w, value)
            bank = repo / 'memory-bank'
            (bank / 'topics').mkdir(parents=True)
            current = bank / 'topics/retry.md'
            current.write_text(rendered['content'])
            proposal = json.loads((ROOT / 'skills/agent-memory/templates/router-proposal.example.json').read_text())
            proposal['project-id'] = value['metadata']['project-id']
            request = w.inside('router.json')
            request.write_text(json.dumps(proposal))
            p = w.run([sys.executable, '-I', ROOT / 'scripts/bank_contract.py', 'render', '--input', request])
            files = json.loads(p.stdout)['files']
            self.assertIn('\n## Omitted contexts', files['MEMORY.md'])
            files['MEMORY.md'] = files['MEMORY.md'].replace('\n## Omitted contexts', '\n| [topics/retry.md](<topics/retry.md>) | Retry lesson and explicit revoked state | component:src/retry | revoked | When retry work needs historical context |\n\n## Omitted contexts')
            for name, body in files.items():
                (bank / name).write_text(body)
            event = {'metadata': {'schema': 'agent-memory/history-event-v1', 'id': str(uuid.uuid4()),
                     'project-id': value['metadata']['project-id'], 'kind': 'memory-revoked',
                     'record': {'id': value['metadata']['id'], 'version': 1}, 'actor': 'detector',
                     'recorded-at': '2026-10-22T21:01:00Z', 'evidence': value['metadata']['evidence']}, 'body': ''}
            _, historical = self.roundtrip(w, event)
            auxiliary = w.inside('optional-history')
            auxiliary.mkdir()
            (auxiliary / 'event.md').write_text(historical['content'])
            w.git('add', '.', cwd=repo)
            w.git('commit', '-m', 'Complete current Markdown with explicit revoked lesson', cwd=repo)
            index = (repo / '.git/index').read_bytes()
            for path in (auxiliary, w.paths['state'], w.paths['cache'], w.paths['data']):
                shutil.rmtree(path)
            p, output = self.command(w, 'validate', file=current)
            self.assertEqual(p.returncode, 0, output)
            self.assertEqual(output['metadata'], value['metadata'])
            self.assertEqual(output['body'], value['body'])
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            p = w.run([sys.executable, '-I', ROOT / 'scripts/bank_state.py', 'inspect', '--repo', repo], check=False)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
            self.assertEqual(json.loads(p.stdout)['canonical_source'], 'Markdown')
            clone = w.clone(repo, 'fresh-record-clone')
            p, output = self.command(w, 'validate', file=clone / 'memory-bank/topics/retry.md')
            self.assertEqual(p.returncode, 0, output)
            self.assertEqual(output['metadata']['status'], 'revoked')
            event['metadata']['kind'] = 'memory-deleted'
            p, _ = self.command(w, 'render', value=event)
            self.assertEqual(p.returncode, 1)

    def test_copied_artifact_and_five_field_tombstone_exception_require_no_harness_or_state(self):
        with World() as w:
            pack = w.inside('copied pack')
            shutil.copytree(ROOT / 'skills/agent-memory', pack)
            helper = pack / 'scripts/record_contract.py'
            self.roundtrip(w, self.proposal(), helper=helper)
            deletion = {'metadata': {'id': str(uuid.uuid4()), 'scope': str(uuid.uuid4()),
                        'deleted-at': '2026-10-22T21:02:00Z', 'deletion-authority': str(uuid.uuid4()),
                        'reason-class': 'sensitive'}, 'body': ''}
            _, output = self.roundtrip(w, deletion, helper=helper)
            self.assertEqual(output['format'], 'tombstone')
            for extra in ({'evidence': ['DELETED_SECRET_CANARY']}, {'schema': 'agent-memory/record-v1'}, {'origin': {'session': 'DELETED_SECRET_CANARY'}}):
                changed = copy.deepcopy(deletion)
                changed['metadata'].update(extra)
                p, _ = self.command(w, 'render', value=changed, helper=helper)
                self.assertEqual(p.returncode, 1)
                self.assertNotIn('DELETED_SECRET_CANARY', p.stdout + p.stderr)
            deletion['body'] = 'DELETED_SECRET_CANARY'
            p, _ = self.command(w, 'render', value=deletion, helper=helper)
            self.assertEqual(p.returncode, 1)
            self.assertNotIn('DELETED_SECRET_CANARY', p.stdout + p.stderr)


if __name__ == '__main__':
    unittest.main()
