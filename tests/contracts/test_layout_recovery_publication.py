"""Real Git raw-source/candidate controls; hosted policy acceptance is separate."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from publication_fixtures import PublicationFixture

ROOT = Path(__file__).resolve().parents[2]


class LayoutPublicationContract(PublicationFixture):
    def module(self, w, code, *args, check=True):
        program = ('import sys,json;from pathlib import Path;'
                   'sys.path.insert(0,sys.argv.pop(1));'
                   'from agent_memory import layout_recovery_publication as p;'
                   'from agent_memory.frontmatter import ContractError;'+code)
        return w.run([sys.executable, '-I', '-B', '-c', program,
                      str(self.pack/'scripts'), *map(str, args)], check=check)

    def test_reported_size_sources_are_raw_not_a_normal_snapshot(self):
        with World() as w:
            repo, bank, peer, _ = self.fixture(w)
            body = '# Qualified context\n\n' + 'Context condition and provenance remain review inputs.\n' * 2000
            raw = body.encode()[:79054]
            raw += b' ' * (79054-len(raw))
            handoff = b'# Pending handoff\n\n' + b'x' * (28478-len(b'# Pending handoff\n\n'))
            (peer/'memory-bank/activeContext.md').write_bytes(raw)
            (peer/'memory-bank/handoff-prompt.md').write_bytes(handoff)
            w.git('add', 'memory-bank', cwd=peer)
            w.git('commit', '-m', 'Synthetic invalid authority layout', cwd=peer)
            revision = w.git('rev-parse', 'HEAD', cwd=peer).stdout.strip()
            w.git('fetch', str(peer), revision, cwd=repo)
            before = self.state(w, repo, bank)
            private = w.inside('raw publication evidence')
            observed = self.module(w, 'm=p.observe_tree(Path(sys.argv[1]),sys.argv[2],"memory-bank",Path(sys.argv[3]));'
                                   'print(json.dumps(m))', repo, revision, private)
            manifest = json.loads(observed.stdout)
            sources = {s['path']: s for s in manifest['sources']}
            self.assertTrue(manifest['observation_complete'])
            self.assertEqual(sources['activeContext.md']['byte_count'], 79054)
            self.assertEqual(sources['handoff-prompt.md']['byte_count'], 28478)
            self.assertEqual((private/sources['activeContext.md']['blob']).read_bytes(), raw)
            self.assertEqual((private/sources['handoff-prompt.md']['blob']).read_bytes(), handoff)
            normal = self.module(w, 'p.git.tree(Path(sys.argv[1]),sys.argv[2],"memory-bank")',
                                 repo, revision, check=False)
            self.assertNotEqual(normal.returncode, 0)
            self.assertEqual(before, self.state(w, repo, bank))

    def test_raw_source_bound_alias_and_committed_hold_refuse(self):
        for variant in ('overflow', 'alias', 'hold', 'symlink'):
            with self.subTest(variant=variant), World() as w:
                repo, bank, _, _ = self.fixture(w)
                if variant == 'overflow':
                    (bank/'activeContext.md').write_bytes(b'x' * 1048577)
                elif variant == 'alias':
                    (bank/'ProjectBrief.md').write_text('Portable alias must not be hidden.\n')
                elif variant == 'hold':
                    (bank/'.agent-memory-transaction.md').write_text('{}\n')
                else:
                    (bank/'redirect.md').symlink_to('projectbrief.md')
                w.git('add', 'memory-bank', cwd=repo)
                w.git('commit', '-m', 'Unsafe raw source control', cwd=repo)
                revision = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
                before = self.state(w, repo, bank)
                process = self.module(w, 'p.observe_tree(Path(sys.argv[1]),sys.argv[2],"memory-bank")',
                                      repo, revision, check=False)
                self.assertNotEqual(process.returncode, 0)
                self.assertEqual(before, self.state(w, repo, bank))

    def test_complete_candidate_delta_excludes_original_feature_index(self):
        with World() as w:
            repo, bank, _, base = self.fixture(w)
            # The authority base is deliberately invalid and cannot pass the
            # normal snapshot used by ordinary publication.candidate.
            (bank/'handoff-prompt.md').write_text('# Explicitly reviewed non-guidance framing\n')
            w.git('add', 'memory-bank/handoff-prompt.md', cwd=repo)
            w.git('commit', '-m', 'Synthetic unsupported authority occupant', cwd=repo)
            invalid = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            (bank/'handoff-prompt.md').unlink()
            files = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            files['projectbrief.md'] += '\nSource-preserving independently reviewed context.\n'
            request = w.inside('candidate files.json'); request.write_text(json.dumps(files))
            before = self.state(w, repo, bank)
            built = self.module(w, 'root=Path(sys.argv[1]);revision=sys.argv[2];'
                                'files=json.loads(Path(sys.argv[3]).read_text());'
                                'raw=p.observe_tree(root,revision,"memory-bank");'
                                'binding={"authority_head":revision,"bank_root":"memory-bank"};'
                                'candidate=p._candidate(root,binding,raw,files);'
                                'delta=p._delta(root,revision,candidate,"memory-bank",'
                                '{s["path"]:s["sha256"] for s in raw["sources"]},files);'
                                'print(json.dumps({"candidate":candidate,"delta":delta}))', repo, invalid, request)
            value = json.loads(built.stdout)
            self.assertIn('handoff-prompt.md', value['delta'])
            self.assertEqual(w.git('rev-list', '--parents', '-n', '1', value['candidate'], cwd=repo).stdout.split()[1:], [invalid])
            self.assertEqual(before, self.state(w, repo, bank))
            # A real code-only candidate must fail the full-tree delta gate,
            # even when its native bank remains otherwise valid.
            w.git('checkout', '-b', 'code-control', value['candidate'], cwd=repo)
            (repo/'code.py').write_text('VALUE = 999\n')
            w.git('add', 'code.py', cwd=repo); w.git('commit', '-m', 'Unreviewed code delta', cwd=repo)
            bad = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            rejected = self.module(w, 'root=Path(sys.argv[1]);files=json.loads(Path(sys.argv[4]).read_text());'
                                   'raw=p.observe_tree(root,sys.argv[2],"memory-bank");'
                                   'p._delta(root,sys.argv[2],sys.argv[3],"memory-bank",'
                                   '{s["path"]:s["sha256"] for s in raw["sources"]},files)',
                                   repo, value['candidate'], bad, request, check=False)
            self.assertNotEqual(rejected.returncode, 0)

    def public_repair(self, w):
        from layout_recovery_fixtures import installed, inspect, prepare, call
        command, source = installed(w)
        repo, bank, peer, _ = self.fixture(w)
        remote = Path(w.git('remote', 'get-url', 'origin', cwd=repo).stdout.strip())
        self.github(w, repo, bank, peer)
        original = (bank/'activeContext.md').read_text()
        sentence = 'Under the synthetic condition preserve the original source before acting.\n'
        raw = original + sentence * ((79054-len(original.encode()))//len(sentence.encode()))
        raw += ' ' * (79054-len(raw.encode()))
        self.assertEqual(len(raw.encode()), 79054)
        (bank/'activeContext.md').write_text(raw)
        (peer/'memory-bank/activeContext.md').write_text(raw)
        w.git('add', 'memory-bank/activeContext.md', cwd=peer)
        w.git('commit', '-m', 'Synthetic invalid authority context', cwd=peer)
        w.git('push', 'origin', 'HEAD:main', cwd=peer)
        intended = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
        intended['activeContext.md'] = original + '\n' + sentence
        _, observed = inspect(w, command, repo)
        proposal_path, proposal = prepare(w, command, source, repo, observed, intended)
        call(w, command, 'apply', '--repo', repo, '--proposal', proposal_path)
        return command, source, repo, bank, remote, observed['operation_id'], intended

    def publication_review(self, observation, files):
        from datetime import datetime, timezone
        from layout_recovery_fixtures import encoded, sha
        import uuid
        author = {'developer_id': observation['binding']['scope_context']['developer_id'],
                  'harness': 'controlled-author', 'session': str(uuid.uuid4()),
                  'evidence': 'Actual disposable fixture source review; not product acceptance'}
        units = []
        for side, variant in observation['variants'].items():
            for source in variant['sources']:
                units.append({'unit_id': str(uuid.uuid4()), 'side': side, 'source_id': source['source_id'],
                              'start': 0, 'end': source['byte_count'], 'disposition': 'preserve',
                              'targets': [{'path': source['path'], 'sha256': sha(files[source['path']].encode())}],
                              'rationale': 'Complete controlled source meaning and conditions retained in the native result',
                              'terminal_evidence': None})
        return {'schema': 'agent-memory/layout-publication-review-v1', 'review_id': str(uuid.uuid4()),
                'observation_sha256': sha(encoded(observation)), 'intended_files_sha256': sha(encoded(files)),
                'author': author, 'reviewer': {**author, 'harness': 'controlled-reviewer', 'session': str(uuid.uuid4())},
                'reviewed_at': datetime.now(timezone.utc).isoformat(), 'source_units': units, 'added_paths': [],
                'identity_decisions': {side: 'Checked original same-project router and authority' for side in ('base', 'local', 'landed')},
                'semantic_preservation': True, 'security': True, 'decision': 'approve'}

    def test_public_raw_authority_publication_and_empty_profile_second_clone(self):
        from layout_recovery_fixtures import call, save
        from identity_fixtures import bind
        with World() as w:
            command, source, repo, bank, remote, operation_id, intended = self.public_repair(w)
            before = self.state(w, repo, bank)
            observation_path = w.inside('publish observation.json')
            call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', operation_id, '--output', observation_path)
            observation = json.loads(observation_path.read_text())['observation']
            review = self.publication_review(observation, intended)
            # Omitted all-side evidence refuses before any candidate can land.
            omitted = {**review, 'source_units': review['source_units'][:-1]}
            bad = {'schema': 'agent-memory/layout-publication-prepare-v1', 'observation_id': observation['observation_id'],
                   'intended_files': intended, 'review': omitted}
            call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation_id,
                 '--input', save(w, bad), '--output', w.inside('refused publication.json'), expected=1)
            request_input = {**bad, 'review': review}
            prepared_path = w.inside('prepared publication.json')
            call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation_id,
                 '--input', save(w, request_input), '--output', prepared_path)
            request = json.loads(prepared_path.read_text())['request']
            call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation_id,
                 '--input', save(w, request), '--output', w.inside('landed publication.json'))
            landed = json.loads(w.inside('landed publication.json').read_text())
            self.assertTrue(landed['publication_verified'])
            self.assertEqual(before, self.state(w, repo, bank))
            again_path = w.inside('historical publication.json')
            call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation_id,
                 '--input', save(w, request), '--output', again_path)
            self.assertEqual(json.loads(again_path.read_text())['state'], 'already-landed')
            clone = w.clone(str(remote), 'second-clone')
            w.git('remote', 'set-url', 'origin', 'https://github.com/fixture/memory.git', cwd=clone)
            original_env = dict(w.env)
            try:
                profile = w.inside('empty second profile'); profile.mkdir(mode=0o700)
                w.env.update(AGENT_MEMORY_CONFIG_HOME=str(profile/'config'), AGENT_MEMORY_STATE_HOME=str(profile/'state'))
                bind(w, clone)
                validation = w.run([command, 'bank', 'validate', '--repo', clone], check=False)
                self.assertEqual(validation.returncode, 0, (validation.stdout, validation.stderr))
                recall = {'project_id': observation['binding']['project_id'], 'query': 'synthetic condition',
                          'ids': [], 'paths': ['activeContext.md'], 'context': {}, 'kinds': [],
                          'minimum_trust': 'reviewed', 'limit': 8, 'include_context': True}
                process = w.run([sys.executable, '-I', '-B', source/'scripts/retrieve.py', '--repo', clone,
                                 '--request', save(w, recall)], check=False)
                self.assertEqual(process.returncode, 0, (process.stdout, process.stderr))
                self.assertIn('synthetic condition', process.stdout)
                self.assertFalse((profile/'state'/'writes').exists() and any((profile/'state'/'writes').rglob('layout-recovery')))
            finally:
                w.env.clear(); w.env.update(original_env)

    def test_publication_refuses_pending_frontier_before_network(self):
        from layout_recovery_fixtures import installed, memory, inspect, prepare, call
        with World() as w:
            command, source = installed(w)
            repo, bank, intended = memory(w, handoff=True)
            _, observed = inspect(w, command, repo)
            proposal, _ = prepare(w, command, source, repo, observed, intended)
            applied = call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            refused = call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', observed['operation_id'],
                           '--output', w.inside('pending publish.json'), expected=1)
            self.assertFalse(refused['publication_verified'])
            self.assertEqual(before, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})

    def test_offline_and_changed_sources_preserve_local_result_and_evidence(self):
        from layout_recovery_fixtures import call, save
        with World() as w:
            command, _, repo, bank, remote, operation_id, intended = self.public_repair(w)
            before = self.state(w, repo, bank)
            away = remote.with_name('offline.git'); remote.rename(away)
            call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', operation_id,
                 '--output', w.inside('offline publish.json'), expected=1)
            self.assertEqual(before, self.state(w, repo, bank))
            away.rename(remote)
            output = w.inside('fresh observation.json')
            call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', operation_id, '--output', output)
            observation = json.loads(output.read_text())['observation']
            request = {'schema': 'agent-memory/layout-publication-prepare-v1', 'observation_id': observation['observation_id'],
                       'intended_files': intended, 'review': self.publication_review(observation, intended)}
            (bank/'projectbrief.md').write_text(intended['projectbrief.md']+'\nA later independent local change.\n')
            changed = self.state(w, repo, bank)
            call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation_id,
                 '--input', save(w, request), '--output', w.inside('stale publication.json'), expected=1)
            self.assertEqual(changed, self.state(w, repo, bank))


if __name__ == '__main__':
    unittest.main()
