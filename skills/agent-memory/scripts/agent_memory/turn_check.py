"""Combine two separately scoped observations without promoting either one."""
from __future__ import annotations

import hashlib
import json
import os
import sys
import uuid

from .frontmatter import ContractError

SCHEMA = 'agent-memory/turn-check-v1'


def requests(value):
    if isinstance(value, dict) and value.get('schema') == 'agent-memory/turn-check-request-v1':
        if set(value) != {'schema', 'assessments'} or not isinstance(value['assessments'], list) or not 1 <= len(value['assessments']) <= 128:
            raise ContractError('Use one to 128 exact source assessments within the bounded compact request')
        values = value['assessments']
        if any(not isinstance(v, dict) or not isinstance(v.get('observation_id'), str) for v in values):
            raise ContractError('A batch requires distinct local observation IDs')
        if len({v['observation_id'] for v in values}) != len(values):
            raise ContractError('Do not repeat an observation inside one compact request')
        if any((v.get('session'), v.get('prompt')) != (values[0].get('session'), values[0].get('prompt')) for v in values):
            raise ContractError('A compact batch must retain one actual session and lifecycle; recover earlier contexts separately')
    else:
        values = [value]
    if any(not isinstance(v, dict) or v.get('outcome') != 'no-learning' for v in values):
        raise ContractError('The compact check accepts explicit no-learning assessments only; use the governed publication route for durable learning')
    return values


def notice_required(repo, harness, session, refresh, assessment_failures=None):
    """Advisory failure deduplication; never a capture or completion receipt."""
    from . import turn_sources
    if not isinstance(session, str) or not session or len(session.encode()) > 512:
        return True
    try:
        _, _, _, directory, _ = turn_sources.namespace(repo, harness, session, 'refresh-notice', True)
        with turn_sources.locked(directory):
            path = directory / 'notice.json'
            try:
                old = turn_sources.read(path, 4096)
                if not isinstance(old, dict) or set(old) != {'schema', 'fingerprint'} or old['schema'] != 'agent-memory/refresh-notice-v1':
                    return True  # Preserve unknown/corrupt advisory state.
            except FileNotFoundError:
                old = None
            if refresh.get('sharing_state') == 'bank-synchronized' and refresh.get('remote_freshness_verified') is True and not assessment_failures:
                if old is not None:
                    path.unlink(); turn_sources.sync(directory)
                return True
            cause = {'refresh': {key:refresh.get(key) for key in ('sharing_state','failure_component','failure_code','failure_path','reason','repair')} if refresh.get('sharing_state') != 'bank-synchronized' or refresh.get('remote_freshness_verified') is not True else None,
                     'source_assessments': assessment_failures or []}
            value = {'schema': 'agent-memory/refresh-notice-v1',
                     'fingerprint': hashlib.sha256(turn_sources.encoded(cause)).hexdigest()}
            if old == value:
                return False
            temp = directory / (str(uuid.uuid4()) + '.tmp')
            try:
                turn_sources.exclusive(temp, value, 4096)
                os.replace(temp, path); turn_sources.sync(directory)
            finally:
                if os.path.lexists(temp): temp.unlink()
            return True
    except (OSError, ValueError, TypeError, KeyError):
        return True  # A diagnostic cache cannot hide or complete actual work.


def check(repo, value, complete, harness):
    """Refresh and retain explicit reviews, without authorship or promotion."""
    values = requests(value)
    from . import memory_refresh
    refresh = memory_refresh.current_result(repo)
    if not isinstance(refresh, dict):
        raise ContractError('The refresh observation is unavailable; preserve original sources')
    receipts = []
    for source in values:
        try:
            receipts.append(complete(repo, source))
        except (OSError, ValueError, TypeError, KeyError, UnicodeError) as exc:
            receipts.append({'verified': False, 'outcome': 'pending',
                             'failure_component': 'source-assessment', 'failure_code': 'assessment-unverified',
                             'reason': str(exc) if isinstance(exc, ContractError) else 'Original source or private assessment state is unavailable.',
                             'repair': 'Preserve the original observation and inspect the matching boundary pending inventory before retrying.',
                             'canonical_writes_performed': False})
    assessment = receipts[0] if len(receipts) == 1 else {'verified': all(r.get('verified') is True for r in receipts), 'results': receipts}
    result = combined(refresh, assessment)
    result['assessment_count'] = len(receipts)
    failures = [{'source_id':source.get('observation_id',source.get('source_uuid',source.get('prompt'))),
                 'request_sha256':hashlib.sha256(json.dumps(source,sort_keys=True,separators=(',',':')).encode()).hexdigest(),
                 'failure_code':receipt.get('failure_code','assessment-unverified'), 'reason':receipt.get('reason')}
                for source,receipt in zip(values,receipts) if receipt.get('verified') is not True]
    result['notice_required'] = notice_required(repo, harness, values[0].get('session'), refresh, failures)
    return result


def combined(refresh, assessment):
    refreshed = (refresh.get('sharing_state') == 'bank-synchronized'
                 and refresh.get('remote_freshness_verified') is True)
    assessed = assessment.get('verified') is True
    return {'schema': SCHEMA, 'operation': 'turn-check',
            'verification_scope': 'mechanical-refresh-and-source-assessment; not semantic completeness',
            'verified': refreshed and assessed,
            'outcome': 'finished' if refreshed and assessed else 'pending-refresh' if not refreshed else 'pending-assessment',
            'refresh_verified': refreshed, 'assessment_verified': assessed,
            'refresh': refresh, 'assessment': assessment,
            'admission_verified': False, 'publication_verified': False,
            'canonical_writes_performed': False}


def exit_code(result):
    if result.get('schema') == SCHEMA:
        return 0 if result.get('verified') is True else 1
    return 1 if result.get('verified') is False else 0


def failure_notice(result):
    """One actionable line retains every failed verification stage."""
    notices = []
    if result.get('schema') == SCHEMA and not result.get('refresh_verified'):
        refresh = result['refresh']
        component = refresh.get('failure_component', 'refresh')
        failure = refresh.get('failure_code', refresh.get('sharing_state', 'unverified'))
        reason = refresh.get('reason') or 'Shared memory needs source-bound review before it is current.'
        repair = refresh.get('repair') or 'Preserve work; inspect the refresh result and complete the required review.'
        notices.append('memory refresh pending [' + str(component) + '/' + str(failure) + ']: ' + str(reason) + ' ' + str(repair))
    if result.get('schema') != SCHEMA or not result.get('assessment_verified'):
        assessment = result.get('assessment', result)
        if 'results' in assessment:
            assessment = next((item for item in assessment['results'] if item.get('verified') is not True), assessment)
        notices.append('source assessment pending: ' + str(assessment.get('reason', 'Original evidence is unavailable.'))
                       + ' ' + str(assessment.get('repair', 'Preserve evidence and inspect the matching boundary operation.')))
    return 'am: ' + '; '.join(notices).replace('\n', ' ')


def emit(result, quiet=False):
    """Success is silent when requested; failures remain short and actionable."""
    code = exit_code(result)
    if not quiet:
        print(json.dumps(result))
    elif code and result.get('notice_required') is not False:
        print(failure_notice(result), file=sys.stderr)
    return code
