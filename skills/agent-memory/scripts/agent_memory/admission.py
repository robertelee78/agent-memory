"""Small source-bound capture and governed admission, without publication."""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import uuid

from . import bank_contract, bank_transactions as tx, bank_writes as writes
from . import comparison, lifecycle, records, scope_binding, security
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError, write
from .identity_git import git
from . import sharing_limits

REQUEST = {'project_id', 'candidate', 'source_class', 'repository_evidence', 'assessment'}
ASSESSMENT = {'category', 'utility', 'durable', 'evidence_confidence', 'evidence_reason',
              'confirmed', 'source_redundant', 'work_state_copy', 'review'}
CATEGORIES = {'durable-discovery', 'verified-fix', 'failed-approach', 'user-correction', 'confirmed-decision'}
CAPTURE = {'schema', 'request', 'scope_context', 'source_observation', 'decision', 'reasons'}
EVIDENCE = {'capture', 'comparison_request', 'comparison_proof', 'recorded_at'}


def result(operation, **value):
    return {'schema': 'agent-memory/admission-result-v1', 'operation': operation,
            'semantic_truth_verified': False, 'source_assertions_verified': False,
            'main_freshness_verified': False, 'publication_verified': False,
            'identity_authenticated': False, **value}


def request_value(request):
    if (not isinstance(request, dict) or set(request) != REQUEST or len(encoded(request)) > 16384
            or not uuid_valid(request['project_id']) or request['source_class'] not in security.CLASSES
            or not isinstance(request['candidate'], dict) or set(request['candidate']) != {'metadata', 'body'}):
        raise ContractError('Capture requires one small explicit project learning and source classification')
    proposal = request['candidate']
    kind, metadata = records.validate(proposal['metadata'], proposal['body'])
    if (kind != 'standalone-record' or metadata['project-id'] != request['project_id']
            or metadata['status'] != 'candidate' or metadata['version'] != 1
            or metadata['corrects'] or metadata['supersedes'] or 'superseded-by' in metadata
            or metadata['source']['kind'] != request['source_class']
            or len(proposal['body'].encode()) > 4096):
        raise ContractError('Capture a small first-version candidate, preserving its true source; updates use consolidation')
    if metadata['origin']['code-state'] == 'unknown' and metadata['applicability'] != 'unknown':
        raise ContractError('Unknown code applicability must remain unknown, not become unconditional guidance')
    a = request['assessment']
    if (not isinstance(a, dict) or set(a) != ASSESSMENT or a['category'] not in CATEGORIES
            or a['evidence_confidence'] not in {'confirmed', 'inferred', 'unconfirmed'}
            or any(type(a[key]) is not bool for key in ('durable', 'confirmed', 'source_redundant', 'work_state_copy'))
            or any(not isinstance(a[key], str) or not a[key].strip() or len(a[key].encode()) > 1024
                   for key in ('utility', 'evidence_reason'))
            or (a['review'] is not None and (not isinstance(a['review'], str)
                or not a['review'].strip() or len(a['review'].encode()) > 1024))):
        raise ContractError('Assess utility, durability, evidence confidence, confirmation and redundant source/work state explicitly')
    if request['source_class'] == 'verified-repository' and request['repository_evidence'] is None:
        raise ContractError('Repository classification requires actual current local source evidence')
    return metadata


def classify(repo, request, *, owned_lock=None):
    metadata = request_value(request)
    root, _ = writes.bank_path(repo)
    context = scope_binding.require(root, request['project_id'], owned_lock=owned_lock)
    if metadata['origin']['developer-id'] != context['developer_id']:
        raise ContractError('A new capture must name the actual detecting developer; existing provenance belongs to consolidation')
    checked, observed = writes.snapshot(root, owned_lock=owned_lock)
    source = request['repository_evidence']
    if source is not None:
        security.repository_source(root, source)
        if source['path'] not in metadata['evidence']:
            raise ContractError('Retain the observed supporting repository path in record evidence')
    branch = git(root, 'symbolic-ref', '--quiet', '--short', 'HEAD', optional=True)
    head = git(root, 'rev-parse', '--verify', 'HEAD', optional=True)
    conditions = metadata['conditions']
    if metadata['origin']['code-state'] == 'unmerged' and not (
            (branch is not None and (conditions.get('branch') == branch or metadata['scope'] == 'branch:' + branch))
            or (head is not None and conditions.get('revision') == head)
            or metadata['applicability'] == 'unknown' or any(v is None for v in conditions.values())):
        raise ContractError('Retain observed branch/revision or unknown applicability for unfinished code')
    if metadata['origin']['code-state'] == 'default-branch':
        raise ContractError('Local capture cannot prove default-branch freshness; retain conditional unmerged or unknown code state')
    contents = [write(metadata) + request['candidate']['body'], encoded(request['assessment']).decode(),
                request['assessment']['utility'], request['assessment']['evidence_reason']]
    reasons = sorted({reason for content in contents for reason in security.scan(content)['reasons']})
    deleted = {e['id'] for e in checked['entries'] if e['format'] == 'tombstone'}
    if metadata['id'] in deleted or any(ref['id'] in deleted for ref in metadata['derived-from']):
        reasons.append('terminal-deletion')
    a = request['assessment']
    if reasons:
        decision = 'quarantine'
    elif not a['durable'] or a['source_redundant'] or a['work_state_copy']:
        decision, reasons = 'discard', ['not-distinct-durable-learning']
    elif metadata['kind'] == 'procedural-candidate':
        decision, reasons = 'review-required', ['explicit-procedure-promotion-required']
    elif not a['confirmed'] or a['evidence_confidence'] != 'confirmed':
        decision, reasons = 'review-required', ['unconfirmed-or-inferred']
    elif request['source_class'] in {'assistant', 'tool', 'external'} and (source is None or not a['review']):
        decision, reasons = 'review-required', ['stronger-source-review-required']
    else:
        decision = 'eligible'
    scope_binding.require(root, checked['project_id'], expected=context, owned_lock=owned_lock)
    if writes.snapshot(root, owned_lock=owned_lock)[1] != observed:
        raise ContractError('Memory changed during capture; reread before classification')
    if source is not None:
        security.repository_source(root, source)
    observation = {'head': head, 'branch': branch, 'repository_evidence': source}
    proof = {'schema': 'agent-memory/capture-v1', 'request': request, 'scope_context': context,
             'source_observation': observation, 'decision': decision, 'reasons': reasons}
    return proof


def capture(repo, request):
    proof = classify(repo, request)
    metadata = request['candidate']['metadata']
    event = security.audit(repo, str(uuid.uuid4()), 'admission', 'checked', proof['scope_context'],
        [request['source_class']], [r for r in proof['reasons'] if r in security.REASONS], [metadata['id']])
    # Unsafe/discarded content is never copied to a proposal, shared file or audit.
    return result('capture', decision=proof['decision'], reasons=proof['reasons'],
        capture=proof if proof['decision'] in {'eligible', 'review-required'} else None,
        source_bytes_observed=request['repository_evidence'] is not None,
        candidate_is_unconfirmed=proof['decision'] != 'eligible', admission_verified=False,
        canonical_writes_performed=False, audit=event)


def draft(repo, value):
    """Build mechanical record fields, retaining every agent judgment explicitly.

    This is not a capture, semantic comparison, admission, or publication.
    The caller must still run the existing governed operations on this request.
    """
    fields = {'body', 'kind', 'scope', 'applicability', 'conditions', 'source',
              'evidence', 'harness', 'session', 'code_state', 'assessment',
              'repository_evidence'}
    if not isinstance(value, dict) or set(value) != fields or not isinstance(value['source'], dict):
        raise ContractError('Draft requires explicit learning, source, applicability and assessment')
    context = scope_binding.require(repo)
    now = datetime.now(timezone.utc).isoformat()
    metadata = {
        'schema': 'agent-memory/record-v1', 'id': str(uuid.uuid4()), 'version': 1,
        'project-id': context['project_id'], 'kind': value['kind'], 'status': 'candidate',
        'scope': value['scope'], 'applicability': value['applicability'],
        'conditions': value['conditions'], 'source': value['source'], 'evidence': value['evidence'],
        'origin': {'developer-id': context['developer_id'], 'harness': value['harness'],
                   'session': value['session'], 'code-state': value['code_state']},
        'recorded-at': now, 'updated': now, 'valid-from': None, 'valid-until': None,
        'derived-from': [], 'corrects': [], 'supersedes': [],
    }
    request = {'project_id': context['project_id'], 'candidate': {'metadata': metadata, 'body': value['body']},
               'source_class': value['source'].get('kind'), 'repository_evidence': value['repository_evidence'],
               'assessment': value['assessment']}
    request_value(request)
    return result('draft', request=request, admission_verified=False, canonical_writes_performed=False,
                  note='Explicit agent assessment only; capture and semantic comparison are still required.')


def start(repo, value):
    """Combine mechanical drafting, capture and *incomplete* discovery.

    The caller supplies the semantic searches and still selects sources and
    judges all matches. This does not create an admission or a sharing proof.
    """
    if not isinstance(value, dict) or set(value) != {'candidate', 'task', 'searches'}:
        raise ContractError('Start requires explicit candidate, task and alternative searches')
    from . import discovery
    request = draft(repo, value['candidate'])['request']
    comparison_request = {'project_id': request['project_id'], 'candidate': request['candidate'],
                          'searches': value['searches'], 'read_paths': []}
    comparison.request_value(comparison_request)
    captured = capture(repo, request)
    if captured['decision'] != 'eligible':
        return result('start', decision=captured['decision'], reasons=captured['reasons'],
                      capture=captured['capture'], discovery=None, comparison_request=None,
                      admission_verified=False, canonical_writes_performed=False)
    observed = discovery.start(repo, value['task'], 'comparison')
    return result('start', decision='eligible', reasons=captured['reasons'],
                  capture=captured['capture'], discovery=observed,
                  comparison_request=comparison_request, admission_verified=False,
                  canonical_writes_performed=False,
                  note='Discovery is incomplete; select sources and perform semantic review before admission.')


def capture_value(repo, value, *, owned_lock=None):
    value = value.get('capture', value) if isinstance(value, dict) else value
    if not isinstance(value, dict) or set(value) != CAPTURE or value['schema'] != 'agent-memory/capture-v1':
        raise ContractError('Use the exact current capture artifact')
    current = classify(repo, value['request'], owned_lock=owned_lock)
    if encoded(value) != encoded(current) or current['decision'] != 'eligible':
        raise ContractError('Capture is changed, unconfirmed or ineligible; inspect its evidence again')
    return current


def desired(base, evidence, operation_id):
    capture = evidence['capture']
    candidate = capture['request']['candidate']
    metadata = copy.deepcopy(candidate['metadata'])
    assessment = capture['request']['assessment']
    reason = (assessment['category'] + ': ' + assessment['utility']
              + ' Evidence assessment: ' + assessment['evidence_reason']
              + (' Source review: ' + assessment['review'] if assessment['review'] else ''))
    metadata.update(status='active', trust='reviewed', visibility='visible',
                    updated=evidence['recorded_at'], **{'admission-reason': reason})
    for field in ('state-reason', 'redaction'):
        metadata.pop(field, None)
    source = capture['request']['repository_evidence']
    if source:
        ref = 'repo:' + source['path'] + '#sha256=' + source['sha256']
        metadata['evidence'] = list(dict.fromkeys([*metadata['evidence'], ref]))
    records.validate(metadata, candidate['body'])
    event = {'id': operation_id, 'evidence': metadata['evidence'], 'authority': capture['scope_context']['developer_id'],
             'reason': assessment['utility'], 'reviewed': True}
    if not lifecycle.transition(candidate['metadata'], 'admit', event)['legal']:
        raise ContractError('Candidate does not satisfy the shared admission transition')
    path = 'topics/' + metadata['id'] + '.md'
    history = 'history/events/' + operation_id + '.md'
    if path in base or history in base:
        raise ContractError('A new learning/event must not replace an existing source')
    history_metadata = {'schema': 'agent-memory/history-event-v1', 'id': operation_id,
        'project-id': metadata['project-id'], 'kind': 'memory-admitted',
        'record': {'id': metadata['id'], 'version': metadata['version']},
        'actor': capture['scope_context']['developer_id'], 'recorded-at': evidence['recorded_at'],
        'evidence': metadata['evidence']}
    records.event(history_metadata)
    if 'navigation' in evidence:
        from . import admission_navigation
        mapped=admission_navigation.apply(base,evidence['navigation'],path,history,metadata,evidence['recorded_at'])
        return {**mapped,path:write(metadata)+candidate['body'],history:write(history_metadata)}
    router, routes, omitted = bank_contract.parse_router(base['MEMORY.md'])
    router['router-updated'] = evidence['recorded_at']
    routes += [dict(path=path, purpose='Evidenced durable project learning', scope=metadata['scope'], lifecycle='active',
                    **{'read-when': 'When this learning and its conditions apply'}),
               dict(path=history, purpose='Admission provenance event', scope='repository', lifecycle='active',
                    **{'read-when': 'When inspecting the admission history'})]
    return {**base, path: write(metadata) + candidate['body'], history: write(history_metadata),
            'MEMORY.md': bank_contract.router_text(router, routes, omitted)}


def evidence_value(value):
    if not isinstance(value, dict) or not EVIDENCE.issubset(value) or set(value)-EVIDENCE-{'navigation'}:
        raise ContractError('Admission needs exact capture, comparison and event evidence')
    if 'navigation' in value:
        from . import admission_navigation
        admission_navigation.navigation_value(value['navigation'])
        if 'discovery' not in value['comparison_request']:
            raise ContractError('Supported semantic admission requires its completed source-bound discovery trace')
    lifecycle.time_value(value['recorded_at'])
    if lifecycle.time_value(value['recorded_at']) > lifecycle.time_value(datetime.now(timezone.utc).isoformat()):
        raise ContractError('Admission event time cannot be in the future')
    if value['comparison_request']['candidate'] != value['capture']['request']['candidate']:
        raise ContractError('Compare the exact captured candidate, not a substitute')
    return value


def preview(repo, captured, request, proof, navigation=None):
    captured = capture_value(repo, captured)
    proof = proof.get('proof', proof) if isinstance(proof, dict) else proof
    if encoded(request['candidate']) != encoded(captured['request']['candidate']):
        raise ContractError('Compare the exact captured candidate')
    compared = comparison.check(repo, request, proof)
    if compared['disposition'] != 'create-distinct':
        return result('preview', disposition=compared['disposition'], admission_verified=False,
            proposal=None, canonical_writes_performed=False,
            detecting_developer_id=compared['detecting_developer_id'],
            other_developer_ids=compared['other_developer_ids'], messages_sent=False,
            recommend_other_developer_clarification=compared['recommend_other_developer_clarification'])
    root, bank = writes.bank_path(repo)
    evidence = evidence_value({'capture': captured, 'comparison_request': request, 'comparison_proof': proof,
                              'recorded_at': datetime.now(timezone.utc).isoformat()})
    if navigation is not None:
        evidence['navigation']=navigation
        evidence_value(evidence)
    operation_id = str(uuid.uuid4())
    base = tx.files_at(bank)
    p = tx.preview(root, desired(base, evidence, operation_id), 'capture')['proposal']
    p.update(operation_id=operation_id, admission_evidence=evidence)
    validate_transaction(p)
    return result('preview', proposal=p, admission_verified=False, canonical_writes_performed=False,
                  disposition='admit-distinct', per_record_operator_approval_required=False)


def validate_transaction(p, *, owned_lock=None):
    evidence = evidence_value(p['admission_evidence'])
    if p['operation'] != 'capture' or p['project_id'] != evidence['capture']['request']['project_id']:
        raise ContractError('Admission evidence belongs only to its own project capture operation')
    capture_value(p['repo_root'], evidence['capture'], owned_lock=owned_lock)
    compared = comparison.check(p['repo_root'], evidence['comparison_request'], evidence['comparison_proof'], owned_lock=owned_lock)
    if compared['disposition'] != 'create-distinct':
        raise ContractError('Existing learning needs consolidation or clarification before a new admission')
    root, bank = writes.bank_path(p['repo_root'])
    base = tx.files_at(bank)
    if tx.file_hashes(base) != p['observed_hashes'] or encoded(desired(base, evidence, p['operation_id'])) != encoded(p['files']):
        raise ContractError('Admission proposal changed canonical content, provenance or the observed bank')
    scope_binding.require(root, p['project_id'], expected=p['scope_context'], owned_lock=owned_lock)


def check_sources(p, *, owner=None, marker=None):
    """Also run during final transactional readback, while memory has its marker."""
    evidence = evidence_value(p['admission_evidence'])
    captured = evidence['capture']
    root = Path(p['repo_root'])
    scope_binding.require(root, p['project_id'], expected=p['scope_context'],
                          owned_lock=owner, owned_transaction=marker)
    current = {'head': git(root, 'rev-parse', '--verify', 'HEAD', optional=True),
               'branch': git(root, 'symbolic-ref', '--quiet', '--short', 'HEAD', optional=True),
               'repository_evidence': captured['request']['repository_evidence']}
    if encoded(current) != encoded(captured['source_observation']):
        raise ContractError('Originating source/Git context changed; preserve the interrupted admission')
    if current['repository_evidence'] is not None:
        security.repository_source(root, current['repository_evidence'])


def apply(prepared):
    p = tx.proposal_value(prepared)
    if 'admission_evidence' not in p:
        raise ContractError('Use a complete governed admission proposal')
    receipt = tx.apply(p)
    return result('apply', **{k: v for k, v in receipt.items() if k not in {'schema', 'operation'}},
        admission_verified=receipt['verified'] and receipt['outcome'] == 'finished',
        canonical_writes_performed=receipt['writes_performed'])


def read_input(path, maximum=65536):
    with Path(path).open('rb') as stream:
        raw = stream.read(maximum + 1)
    if len(raw) > maximum:
        raise ContractError('Admission input exceeds its bounded reader')
    return load(raw)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    d = commands.add_parser('draft');d.add_argument('--repo', required=True);d.add_argument('--input', required=True)
    s = commands.add_parser('start');s.add_argument('--repo', required=True);s.add_argument('--input', required=True)
    c = commands.add_parser('capture');c.add_argument('--repo', required=True);c.add_argument('--request', required=True)
    p = commands.add_parser('preview');p.add_argument('--repo', required=True)
    for flag in ('capture', 'request', 'proof'):p.add_argument('--' + flag, required=True)
    p.add_argument('--navigation')
    a = commands.add_parser('apply');a.add_argument('--proposal', required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == 'draft':value = draft(args.repo, read_input(args.input, 16384))
        elif args.command == 'start':value = start(args.repo, read_input(args.input, 16384))
        elif args.command == 'capture':value = capture(args.repo, read_input(args.request, 16384))
        elif args.command == 'preview':value = preview(args.repo, read_input(args.capture), read_input(args.request), read_input(args.proof),read_input(args.navigation,16384) if args.navigation else None)
        else:value = apply(read_input(args.proposal, sharing_limits.MAX_PRIVATE))
        print(json.dumps(value, indent=2, ensure_ascii=False));return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError):
        print(json.dumps(result(args.command, admission_verified=False, canonical_writes_performed=False,
            error='Unsafe, ineligible, unavailable or changed admission; preserve sources and repeat evidence/comparison review')))
        return 1


if __name__ == '__main__':
    sys.exit(main())
