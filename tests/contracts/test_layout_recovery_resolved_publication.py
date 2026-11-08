"""One controlled recovery, ordinary admission, publication, and fresh recall journey.

Review attestations bind this disposable fixture's sources; they are not an
independent semantic acceptance verdict.
"""
import json
from pathlib import Path
import sys
import unittest

from world import World
from identity_fixtures import bind
import layout_recovery_fixtures as f
from contracts.test_comparison import unpack
from contracts import test_layout_recovery_publication as publication


class ResolvedPublicationJourneyContract(unittest.TestCase):
    def test_eight_resolved_units_land_and_recall_in_an_empty_profile(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True, handoff=True)
            original_context = (bank / 'activeContext.md').read_bytes()
            original_handoff = (bank / 'handoff-prompt.md').read_bytes()
            self.assertEqual(len(original_context), 79054)
            self.assertEqual(len(original_handoff), 28478)

            # Establish a normal common ancestor and a disposable authority,
            # then introduce the invalid sources as the actual recovery input.
            (bank / 'handoff-prompt.md').unlink()
            for path, body in native.items():
                (bank / path).write_text(body)
            evidence = repo / 'synthetic-policy.md'
            evidence.write_text('# Independent synthetic repository source\n\n' + '\n\n'.join(f.POLICIES) + '\n')
            feature = repo / 'code.py'
            feature.write_text('VALUE = 1  # committed feature\n')
            w.git('add', 'memory-bank', 'synthetic-policy.md', 'code.py', cwd=repo)
            w.git('commit', '-m', 'Synthetic common ancestor', cwd=repo)
            remote = w.repository('authority.git', bare=True)
            w.git('remote', 'add', 'origin', str(remote), cwd=repo)
            w.git('config', '--local', 'agent-memory.authorityBranch', 'main', cwd=repo)
            w.git('push', 'origin', 'HEAD:main', cwd=repo)
            peer = w.clone(str(remote), 'authority-peer')
            fixture = publication.LayoutPublicationContract('runTest')
            fixture.pack = source
            fixture.github(w, repo, bank, peer)
            w.git('commit', '--only', '-m', 'Fixture existing authority identity', '--', 'memory-bank/MEMORY.md', cwd=repo)
            w.git('fetch', 'origin', 'main', cwd=repo)
            w.git('merge', '--no-edit', 'FETCH_HEAD', cwd=repo)
            native = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            (bank / 'activeContext.md').write_bytes(original_context)
            (bank / 'handoff-prompt.md').write_bytes(original_handoff)
            (peer / 'memory-bank/activeContext.md').write_bytes(original_context)
            w.git('add', 'memory-bank/activeContext.md', cwd=peer)
            w.git('commit', '-m', 'Synthetic invalid authority context', cwd=peer)
            w.git('push', 'origin', 'HEAD:main', cwd=peer)

            feature.write_text('VALUE = 2  # staged feature\n')
            w.git('add', 'code.py', cwd=repo)
            feature.write_text('VALUE = 3  # unstaged feature\n')
            untracked = repo / 'untracked-feature.txt'
            untracked.write_text('Untracked feature work\n')
            def feature_state():
                return (feature.read_bytes(), untracked.read_bytes(), evidence.read_bytes(),
                        (repo / '.git/index').read_bytes(),
                        w.git('rev-parse', 'HEAD', cwd=repo).stdout,
                        w.git('symbolic-ref', 'HEAD', cwd=repo).stdout,
                        w.git('status', '--porcelain=v1', '--untracked-files=all', '--',
                              'code.py', 'untracked-feature.txt', 'synthetic-policy.md', cwd=repo).stdout)
            before = feature_state()
            originals = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            safe, inspected = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, inspected, native)
            applied = f.call(w, command, 'apply', '--repo', repo, '--operation', safe['operation_id'], '--proposal', proposal)
            self.assertTrue(applied['canonical_eligibility'])
            self.assertFalse(applied['semantic_recovery_complete'])
            pending = set(applied['pending_unit_ids'])
            self.assertEqual(len(pending), 8)
            directory = f.private_directory(w, bank, safe['operation_id'])
            initial = json.loads((directory / 'ledger/00000000.json').read_text())
            units = {unit['unit_id']: unit for unit in initial['units']}
            self.assertEqual(set(units), pending)
            for original in inspected['manifest']['sources']:
                self.assertEqual((directory / original['blob']).read_bytes(), originals[original['path']])
            pending_bank = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            remote_before = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            refused = f.call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', safe['operation_id'],
                             '--output', w.inside('pending-publication.json'), expected=1)
            self.assertFalse(refused['publication_verified'])
            self.assertEqual(remote_before, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertEqual(pending_bank, {p.relative_to(bank).as_posix(): p.read_bytes()
                                            for p in bank.rglob('*') if p.is_file()})
            self.assertEqual(feature_state(), before)
            handoff_source = next(row for row in inspected['manifest']['sources'] if row['path'] == 'handoff-prompt.md')
            ranges = sorted((span['start'], span['end'])
                            for unit in units.values() for span in unit['source_ranges']
                            if span['source_id'] == handoff_source['source_id'])
            self.assertEqual(ranges, [(start, start + 3560) for start in range(0, 24920, 3560)] + [(24920, 28478)])
            pending_status = f.call(w, command, 'status', '--repo', repo, '--operation', safe['operation_id'])
            self.assertFalse(pending_status['semantic_recovery_complete'])
            self.assertEqual(set(pending_status['pending_unit_ids']), pending)

            admission_path, transaction, target = f.admission_proposal(
                w, command, source, repo, bank, inspected, full=True, policy_number=None)
            admitted = f.public(w, command, 'admission', 'apply', '--proposal', admission_path)
            self.assertTrue(admitted['admission_verified'])
            self.assertEqual(admitted['operation_id'], transaction['operation_id'])
            status = f.call(w, command, 'status', '--repo', repo, '--operation', safe['operation_id'])
            self.assertTrue(status['semantic_recovery_complete'])
            self.assertEqual(status['pending_unit_ids'], [])
            head = json.loads((directory / 'ledger-head.json').read_text())
            ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
            self.assertEqual({unit['unit_id'] for unit in ledger['units']}, pending)
            resolution_ids = set()
            for unit in ledger['units']:
                self.assertEqual(unit['state'], 'admitted')
                self.assertEqual(unit['source_ranges'], units[unit['unit_id']]['source_ranges'])
                self.assertEqual(unit['provenance'], units[unit['unit_id']]['provenance'])
                self.assertEqual(len(unit['terminal_receipts']), 1)
                link = unit['terminal_receipts'][0]
                resolution_ids.add(link['resolution_id'])
                receipt = json.loads((directory / 'resolutions' / (link['resolution_id'] + '.json')).read_text())
                self.assertEqual(f.sha(f.encoded(receipt)), link['sha256'])
                self.assertEqual(receipt['canonical_operation_id'], transaction['operation_id'])
                self.assertEqual(receipt['unit_id'], unit['unit_id'])
            self.assertEqual(len(resolution_ids), 8)
            metadata, body = unpack((bank / target).read_text())
            for policy in f.POLICIES:
                self.assertIn(policy, body)
            self.assertIn('synthetic-policy.md', metadata['evidence'])
            receipt = json.loads(Path(admitted['receipt_path']).read_text())

            observation_path = w.inside('resolved-publication-observation.json')
            f.call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', safe['operation_id'], '--output', observation_path)
            observation = json.loads(observation_path.read_text())['observation']
            files = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            resolved_bank = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            review = fixture.publication_review(observation, files)
            review['record_evidence'] = [{'operation_id': transaction['operation_id'],
                                          'receipt_sha256': f.sha(f.encoded(receipt)), 'proposal': transaction}]
            request = {'schema': 'agent-memory/layout-publication-prepare-v1',
                       'observation_id': observation['observation_id'], 'intended_files': files, 'review': review}
            prepared_path = w.inside('resolved-publication-prepared.json')
            f.call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', safe['operation_id'],
                   '--input', f.save(w, request), '--output', prepared_path)
            prepared = json.loads(prepared_path.read_text())['request']
            landed_path = w.inside('resolved-publication-landed.json')
            f.call(w, command, 'publish', 'land', '--repo', repo, '--operation', safe['operation_id'],
                   '--input', f.save(w, prepared), '--output', landed_path)
            self.assertTrue(json.loads(landed_path.read_text())['publication_verified'])
            self.assertEqual(resolved_bank, {p.relative_to(bank).as_posix(): p.read_bytes()
                                             for p in bank.rglob('*') if p.is_file()})
            self.assertEqual(feature_state(), before)
            for original in inspected['manifest']['sources']:
                self.assertEqual((directory / original['blob']).read_bytes(), originals[original['path']])
            changed = w.git('diff-tree', '--no-commit-id', '--name-only', '-r', 'refs/heads/main', cwd=remote).stdout.splitlines()
            self.assertTrue(all(path.startswith('memory-bank/') for path in changed), changed)
            self.assertIn('memory-bank/' + target, changed)

            clone = w.clone(str(remote), 'fresh-consumer')
            w.git('remote', 'set-url', 'origin', 'https://github.com/fixture/memory.git', cwd=clone)
            original_env = dict(w.env)
            try:
                profile = w.inside('empty consumer profile')
                profile.mkdir(mode=0o700)
                w.env.update(AGENT_MEMORY_CONFIG_HOME=str(profile / 'config'), AGENT_MEMORY_STATE_HOME=str(profile / 'state'))
                bind(w, clone)
                validation = w.run([command, 'bank', 'validate', '--repo', clone], check=False)
                self.assertEqual(validation.returncode, 0, (validation.stdout, validation.stderr))
                recall = {'project_id': observation['binding']['project_id'], 'query': 'synthetic offline delivery queue',
                          'ids': [metadata['id']], 'paths': [], 'context': {}, 'kinds': [metadata['kind']],
                          'minimum_trust': 'reviewed', 'limit': 8, 'include_context': False}
                process = w.run([sys.executable, '-I', '-B', source / 'scripts/retrieve.py', '--repo', clone,
                                 '--request', f.save(w, recall)], check=False)
                self.assertEqual(process.returncode, 0, (process.stdout, process.stderr))
                result = json.loads(process.stdout)
                self.assertEqual([item['path'] for item in result['results']], [target])
                recalled = result['results'][0]
                self.assertEqual(recalled['body'], body)
                for field in ('id', 'kind', 'evidence', 'conditions', 'origin', 'source'):
                    self.assertEqual(recalled['record'][field], metadata[field])
                for policy in f.POLICIES:
                    self.assertIn(policy, recalled['body'])
                self.assertFalse(result['source_assertions_verified'])
                self.assertFalse((profile / 'state' / 'writes').exists() and
                                 any((profile / 'state' / 'writes').rglob('layout-recovery')))
            finally:
                w.env.clear()
                w.env.update(original_env)


if __name__ == '__main__':
    unittest.main()
