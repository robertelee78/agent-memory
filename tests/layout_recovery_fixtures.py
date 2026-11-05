"""Disposable public layout-recovery fixtures; review labels are test evidence only.

These controlled attestations exercise schema/source binding. They are not an
independent product acceptance verdict or an assertion about incident content.
"""
import hashlib
import json
import os
from pathlib import Path
import sys
import uuid

from identity_fixtures import bind
from migration_fixtures import installed as install_fixture
from runtime_update_fixtures import bank
from record_fixtures import record_text, project_id
from contracts.test_comparison import unpack, render

POLICIES = (
    'In the synthetic offline-queue environment, attempt delivery at most three times, retain every queued item after failure, and resume delivery only after the operator changes the environment to online.',
    'For synthetic read-only database snapshots, pin the snapshot revision before querying, reject any write request, and discard the query result if the revision changes before readback.',
    'For the synthetic local cache, a missing checksum is a cache miss rather than an empty value; preserve the backing file and rebuild only after reading the authoritative fixture input.',
    'For synthetic batch imports, process records in original sequence, keep the first invalid record pending with its original ordinal, and never report the unread remainder as imported.',
    'For synthetic dry-run deployments, generate the exact target plan without activating it; activation requires a separate operator decision tied to that same plan and target revision.',
    'For synthetic expired credentials, stop the requested network operation without retrying another account; retain the pending request and ask the detecting developer to renew the intended account.',
    'For synthetic concurrent edits, preserve both source versions and their conditions; a later timestamp alone does not choose the correct meaning or authorize overwriting either version.',
    'For synthetic terminal deletion, retain only the content-free stable-identity marker in current memory and reject any later stale snapshot that tries to resurrect the removed body.',
)


def policy_body(number):
    return '# Synthetic policy ' + str(number + 1) + '\n\n' + POLICIES[number] + '\n'


def add_witness_pair(directory, developer_id):
    """Real serialized mandatory rows, not sizing-only placeholder metadata."""
    pid = project_id((directory / 'MEMORY.md').read_text())
    owner = str(uuid.uuid4()); capture = str(uuid.uuid4())
    old, _ = unpack(record_text('# Earlier synthetic queue policy\n\nRetain queued items in the legacy synthetic environment.\n', pid, status='superseded'))
    old.update({'id': owner, 'version': 1, 'derived-from': [{'id': capture, 'version': 1}],
                'superseded-by': [{'id': owner, 'version': 2}]})
    old['origin'].update({'developer-id': developer_id, 'harness': 'fixture-prior-author', 'session': 'prior-source-observation'})
    current, _ = unpack(record_text('# Current synthetic queue policy\n\nUnder the current synthetic environment, retain queued items and preserve failure evidence.\n', pid))
    current.update({'id': owner, 'version': 2, 'derived-from': [{'id': owner, 'version': 1}],
                    'supersedes': [{'id': owner, 'version': 1}]})
    current['origin'].update({'developer-id': developer_id, 'harness': 'fixture-prior-author', 'session': 'prior-reviewed-update'})
    paths = [('history/sources/' + capture + '/' + owner + '.md', old, '# Earlier synthetic queue policy\n\nRetain queued items in the legacy synthetic environment.\n'),
             ('topics/' + owner + '.md', current, '# Current synthetic queue policy\n\nUnder the current synthetic environment, retain queued items and preserve failure evidence.\n')]
    router = directory / 'MEMORY.md'
    for path, metadata, body in paths:
        target = directory / path; target.parent.mkdir(parents=True, exist_ok=True); target.write_text(render(metadata, body))
        row = '| [' + path + '](<' + path + '>) | Controlled prior-guidance fixture | repository | ' + metadata['status'] + ' | When reviewing synthetic queue conditions |\n'
        router.write_text(router.read_text().replace('\n## Omitted contexts', row + '\n## Omitted contexts'))


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode()


def sha(value):
    return hashlib.sha256(value).hexdigest()


def save(world, value, name=None):
    path = world.inside(name or ('recovery-' + str(uuid.uuid4()) + '.json'))
    path.write_bytes(encoded(value)); path.chmod(0o600)
    return path


def installed(world):
    return install_fixture(world, codecs=False)


def memory(world, *, oversized=False, handoff=False):
    repo = world.repository(); directory = bank(world, repo); scope = bind(world, repo)
    if oversized or handoff:
        add_witness_pair(directory, scope['developer_id'])
    native = {path.relative_to(directory).as_posix(): path.read_text() for path in directory.rglob('*.md')
              if not path.name.startswith('.agent-memory')}
    if oversized:
        sentence = 'Under the synthetic review condition, preserve existing guidance and check the original source before acting.\n'
        intro = '# Active context\n\n'
        body = intro + sentence * ((79054 - len(intro.encode())) // len(sentence.encode()))
        body += ' ' * (79054 - len(body.encode()))
        (directory / 'activeContext.md').write_bytes(body.encode())
        native['activeContext.md'] = intro + sentence
        prefix, tail = native['MEMORY.md'].split('\n## Omitted contexts\n', 1)
        route = '| [activeContext.md](<activeContext.md>) | Synthetic source-preserving orientation | repository | active | When reviewing the synthetic recovery condition |'
        native['MEMORY.md'] = prefix.rstrip() + '\n' + route + '\n\n## Omitted contexts\n' + tail.replace('- `activeContext.md`: No applicable evidence yet\n', '')
        assert (directory / 'activeContext.md').stat().st_size == 79054
    if handoff:
        parts = []
        for number, size in enumerate([3560] * 7 + [3558]):
            text = policy_body(number) + '\n'
            sentence = POLICIES[number] + '\n'
            text += sentence * ((size - len(text.encode())) // len(sentence.encode()))
            text += ' ' * (size - len(text.encode()))
            parts.append(text.encode())
        (directory / 'handoff-prompt.md').write_bytes(b''.join(parts))
        (repo / 'synthetic-policy.md').write_text('# Independently readable synthetic repository source\n\n' + POLICIES[0] + '\n')
        assert (directory / 'handoff-prompt.md').stat().st_size == 28478
    return repo, directory, native


def call(world, command, *args, expected=0, env=None):
    completed = world.run([command, 'bank', 'recover-layout', *args], check=False, env=env, timeout=60)
    value = json.loads(completed.stdout) if completed.stdout else None
    if completed.returncode != expected:
        raise AssertionError((completed.returncode, value, completed.stderr))
    return value


def inspect(world, command, repo):
    output = world.inside('inspect-' + str(uuid.uuid4()) + '.json')
    safe = call(world, command, 'inspect', '--repo', repo, '--output', output)
    return safe, json.loads(output.read_text())


def inherited_rows(world, manifest):
    base = (Path(world.env['AGENT_MEMORY_STATE_HOME']) / 'writes'
            / sha(manifest['bank_path'].encode()) / 'layout-recovery')
    index = base / 'index.json'
    result = []
    for operation, indexed in sorted((json.loads(index.read_text())['operations'] if index.exists() else {}).items()):
        if indexed['state'] != 'rolled-back':
            continue
        directory = base / operation
        original = json.loads((directory / 'manifest.json').read_text())
        head = json.loads((directory / 'ledger-head.json').read_text())
        ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
        sources = {row['source_id']: row for row in original['sources']}
        for unit in ledger['units']:
            if unit['state'] != 'pending-admission':
                continue
            body = b''.join((directory / sources[span['source_id']]['blob']).read_bytes()[span['start']:span['end']]
                            for span in unit['source_ranges'])
            result.append({'unit_id': unit['unit_id'], 'operation_id': operation, 'body': body.decode(),
                           'source_ranges': unit['source_ranges'], 'provenance': unit['provenance'], 'reason': unit['reason'],
                           'selection_class': 'comparison-only-not-current-guidance', 'source_assertions_verified': False})
    return result


def review_request(world, command, source, repo, inspected, intended):
    manifest = inspected['manifest']; operation = manifest['operation_id']
    author = {'developer_id': manifest['identity']['scope_context']['developer_id'], 'harness': 'fixture-author',
              'session': 'controlled-author-' + operation, 'evidence': 'Controlled test author observation; not acceptance evidence'}
    reviewer = {**author, 'harness': 'fixture-independent-reviewer', 'session': 'controlled-reviewer-' + operation,
                'evidence': 'Controlled distinct test review attestation covering all original bytes and result'}
    units = []
    page_cursor = 0
    for original in manifest['sources']:
        lengths = [3560] * 7 + [3558] if original['path'] == 'handoff-prompt.md' else [original['byte_count']]
        offset = 0
        source_pages = list(range(page_cursor, page_cursor + max(1, (original['byte_count'] + 24575) // 24576)))
        page_cursor += len(source_pages)
        existing = None
        if original['path'] in intended and intended[original['path']].startswith('---\n'):
            metadata, _ = unpack(intended[original['path']])
            if 'id' in metadata:
                existing = {'id': metadata['id'], 'version': metadata.get('version')}
        for size in lengths:
            pending = original['path'] == 'handoff-prompt.md'
            units.append({'unit_id': str(uuid.uuid4()), 'source_ranges': [{'source_id': original['source_id'], 'start': offset, 'end': offset + size}],
                          'semantic_kind': 'unadmitted-source' if pending else 'canonical-framing' if original['path'] == 'MEMORY.md' else 'canonical-guidance',
                          'existing_record': existing, 'provenance': {'origin': 'unknown' if pending else 'existing canonical fixture', 'uncertainty': pending},
                          'disposition': 'pending-admission' if pending else 'preserve',
                          'targets': [] if pending else [{'path': original['path'], 'sha256': sha(intended[original['path']].encode())}],
                          'reason': 'Unconfirmed synthetic candidate needs separate ordinary admission' if pending else 'Preserve complete synthetic meaning and conditions in native concise form',
                          'required_actions': ['Detecting developer confirms independent repository evidence, then ordinary reviewed admission'] if pending else [],
                          'context_pages': source_pages})
            offset += size
    unit_map = {'schema': 'agent-memory/layout-unit-map-v1', 'operation_id': operation,
                'manifest_sha256': sha(encoded(manifest)), 'units': units}
    stamp = manifest['inspected_at']; review_id = str(uuid.uuid4())
    inherited = inherited_rows(world, manifest)
    review = {'schema': 'agent-memory/layout-review-v1', 'review_id': review_id, 'operation_id': operation,
              'manifest_sha256': sha(encoded(manifest)), 'unit_map_sha256': sha(encoded(unit_map)),
              'intended_files_sha256': sha(encoded(intended)), 'authority_sha256': sha(encoded(manifest['identity']['project_evidence'])),
              'author': author, 'reviewer': reviewer, 'reviewed_at': stamp,
              'unit_decisions': [{'unit_id': unit['unit_id'], 'disposition': unit['disposition'], 'rationale': unit['reason']} for unit in units],
              'inherited_frontier_sha256': sha(encoded(inherited)),
              'inherited_unit_decisions': [{'unit_id': row['unit_id'], 'disposition': 'retain-pending',
                  'rationale': 'Retain original reviewed obligation and stable identity; no admission by this repair'} for row in inherited],
              'semantic_preservation': True, 'security': True, 'identity': True, 'decision': 'approve',
              'evidence': 'Controlled fixture review of full original/context pages and complete native intended map'}
    physical = {row['path']: row for row in manifest['physical_entries']}
    metadata = {'files': {path: {'uid': physical[path]['uid'] if path in physical else os.geteuid(),
                                'gid': physical[path]['gid'] if path in physical else os.getegid(),
                                'mode': physical[path]['mode'] & 0o777 if path in physical else 0o644} for path in intended},
                'directories': {}}
    attestation = None
    if inherited or any(unit['disposition'] == 'pending-admission' for unit in units):
        completed = world.run([sys.executable, '-I', source / 'scripts/artifact.py', 'inspect', '--pack', source], check=False)
        runtime = json.loads(completed.stdout)
        if completed.returncode:
            raise AssertionError(runtime)
        state_root = world.env['AGENT_MEMORY_STATE_HOME']
        attestation = {'schema': 'agent-memory/layout-quiescence-attestation-v1', 'attestation_id': str(uuid.uuid4()),
                       'operation_id': operation, 'project_id': manifest['project_id'], 'repo_root': str(repo), 'bank_path': manifest['bank_path'],
                       'checkout_key': manifest['identity']['scope_context']['checkout_key'], 'state_root': state_root,
                       'manifest_sha256': sha(encoded(manifest)), 'runtime_artifact_digest': runtime['artifact_digest'],
                       'operator_developer_id': author['developer_id'], 'issued_at': stamp,
                       'declared_principals': [author['developer_id']], 'declared_invocation_routes': [str(command)],
                       'stopped_sessions': [], 'statement': 'All authorized legacy invocation routes and in-flight sessions are disabled or quiesced until terminal recovery retirement, or use the independently verified capable runtime and bound private state.',
                       'review_id': review_id}
    return {'schema': 'agent-memory/layout-prepare-v1', 'operation_id': operation, 'author': author,
            'unit_map': unit_map, 'intended_files': intended, 'metadata_plan': metadata, 'review': review,
            'quiescence_attestation': attestation, 'retained_source_ids': []}


def prepare(world, command, source, repo, inspected, intended):
    request = review_request(world, command, source, repo, inspected, intended)
    output = world.inside('proposal-' + str(uuid.uuid4()) + '.json')
    call(world, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
         '--input', save(world, request), '--output', output)
    return output, json.loads(output.read_text())
