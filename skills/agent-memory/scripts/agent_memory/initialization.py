"""Evidence-led orientation, preserving existing memory and stronger authorities."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import sys
import uuid

from . import repository_config
from . import authority
from . import bank_contract as bank
from . import bank_transactions as transactions
from . import bank_writes as writes
from . import scope_binding as identity
from . import scopes
from . import security
from . import local_registry as storage
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from .identity_git import git

SCHEMA = 'agent-memory/initialize-proposal-v1'
EXCLUDED = {'.git', 'memory-bank', 'node_modules', 'vendor', 'dist', 'build', '__pycache__', '.venv', 'venv', '.tox'}
CODE = {'.py', '.js', '.ts', '.tsx', '.jsx', '.go', '.rs', '.c', '.h', '.cpp', '.java', '.rb', '.sh', '.swift', '.kt'}
MAX_SOURCES, MAX_FILE = 64, 65536


def result(operation, **values):
    return {'schema': 'agent-memory/initialization-result-v1', 'operation': operation,
            'source_assertions_verified': False, 'publication_verified': False, **values}


def root_at(repo):
    repo = Path(repo).resolve()
    if not repo.is_dir():
        raise ContractError('Initialization requires an existing repository directory')
    top = git(repo, 'rev-parse', '--show-toplevel', optional=True)
    return Path(top).resolve() if top else repo


def source_kind(path):
    parts = path.parts
    if path.suffix.lower() == '.md' or path.name.lower().startswith('readme'):
        return 'planning' if any(p.lower() in {'adr', 'adrs', 'design', 'prd', 'rfcs'} for p in parts[:-1]) else 'documentation'
    if path.suffix.lower() in CODE:
        return 'test' if any(p.lower() in {'test', 'tests', '__tests__', 'spec', 'specs'} for p in parts[:-1]) or path.stem.startswith('test_') else 'code'
    return None


def local_sources(root, selected=None):
    output, gaps, examined = [], [], 0
    canonical_bank = repository_config.bank_path(root)
    listed = git(root, 'ls-files', '--cached', '--others', '--exclude-standard', '-z', optional=True)
    allowed = set(listed.split('\0')) if listed is not None else None
    if selected is not None:
        if (not isinstance(selected, list) or not selected or len(selected) > MAX_SOURCES
                or len(set(selected)) != len(selected)):
            raise ContractError('Explicit source selection must contain 1-64 unique repository paths')
        for ref in selected:
            if allowed is not None and ref not in allowed:
                raise ContractError('Explicit source is ignored or outside the Git source inventory; inspect it without publishing private context')
            source = read_source(root, ref)
            if Path(ref).is_relative_to(repository_config.relative(root)):
                raise ContractError('Existing memory is inventoried separately, never promoted into source authority')
            output.append({'ref': ref, 'kind': source_kind(Path(ref)) or 'documentation',
                           'sha256': source['sha256'], 'bytes': len(source['content'].encode())})
        return output, gaps
    # This is deliberately a bounded orientation inventory, never an exhaustive
    # repo crawler or a substitute for inspecting the relevant implementation.
    for directory, names, files in os.walk(root, followlinks=False):
        names[:] = sorted(n for n in names if n not in EXCLUDED and not n.startswith('.')
                          and not (Path(directory) / n).is_symlink()
                          and (Path(directory) / n) != canonical_bank)
        for name in sorted(files):
            examined += 1
            if examined > 4096:
                return output, [*gaps, 'Local source discovery hit its 4096-file bound; inspect additional relevant sources explicitly']
            path = Path(directory) / name
            relative = path.relative_to(root)
            kind = source_kind(relative)
            if (not kind or name.startswith('.') or path.is_symlink()
                    or (allowed is not None and relative.as_posix() not in allowed)):
                continue
            if len(output) >= MAX_SOURCES:
                return output, [*gaps, 'Local source discovery hit its 64-source bound; inspect additional relevant sources explicitly']
            if path.stat().st_size > MAX_FILE:
                gaps.append('Source exceeds the bounded reader: ' + relative.as_posix())
                continue
            try:
                raw = path.read_bytes()
                raw.decode('utf-8')
            except UnicodeError:
                gaps.append('Non-UTF8 source requires explicit inspection: ' + relative.as_posix())
                continue
            ref = relative.as_posix()
            if any(ord(c) < 32 or c in '\x7f\x85\u2028\u2029' for c in ref):
                gaps.append('A source path cannot be represented safely; inspect it explicitly')
                continue
            output.append({'ref': ref, 'kind': kind, 'sha256': digest(raw), 'bytes': len(raw)})
    return output, gaps


def awa_evidence(path, project_id):
    if path is None:
        availability = 'not-collected' if shutil.which('awa') else 'unavailable'
        return {'availability': availability, 'sources': [], 'path': None, 'sha256': None}, ['AWA project/task/milestone evidence is ' + availability + '; use the AWA skill when available']
    path = Path(path).resolve()
    raw = path.read_bytes()
    value = load(raw)
    fields = {'schema', 'project_id', 'availability', 'reason', 'sources'}
    if (not isinstance(value, dict) or set(value) != fields or value['schema'] != 'agent-memory/awa-initialization-evidence-v1'
            or value['project_id'] != project_id or value['availability'] not in {'available', 'unavailable'}
            or not isinstance(value['reason'], str) or not isinstance(value['sources'], list) or len(value['sources']) > 64):
        raise ContractError('AWA evidence requires the same project, explicit availability and a bounded skill-collected source list')
    seen = set()
    for source in value['sources']:
        if (not isinstance(source, dict) or set(source) != {'kind', 'ref'} or source['kind'] not in {'project', 'task', 'milestone'}
                or not authority.text(source['ref']) or not source['ref'].startswith('https://') or source['ref'] in seen):
            raise ContractError('AWA sources require unique explicit project/task/milestone references')
        seen.add(source['ref'])
    if value['availability'] == 'unavailable' and (not value['reason'].strip() or value['sources']):
        raise ContractError('Unavailable AWA evidence needs the observed reason, without invented work records')
    gaps = [] if value['availability'] == 'available' else ['AWA evidence unavailable: ' + value['reason']]
    return {**value, 'path': str(path), 'sha256': digest(raw)}, gaps


def inspect(repo, awa_path=None, selected=None):
    root = root_at(repo)
    path = repository_config.bank_path(root)
    bank.bank_root(path)
    if path.is_symlink():
        raise ContractError('Existing bank is a symlink; preserve it for explicit repair')
    existing, bank_state, project_id = {}, 'absent', None
    error = None
    if path.exists():
        if not path.is_dir():
            raise ContractError('Existing bank location is not a directory; preserve it')
        if any(path.iterdir()):
            try:
                checked, existing = writes.snapshot(root)
                project_id, bank_state = checked['project_id'], 'usable'
            except (ContractError, OSError, ValueError) as exc:
                bank_state, error = 'needs-inspection', str(exc)
        else:
            bank_state = 'empty'
    if project_id is None:
        try:
            project_id = identity.project.status(root)['project_id']
        except (ContractError, OSError, ValueError):
            pass
    sources, gaps = local_sources(root, selected)
    for kind in ('documentation', 'planning', 'code', 'test'):
        if not any(source['kind'] == kind for source in sources):
            gaps.append('No ' + kind + ' sources selected; inspect relevant evidence explicitly if available')
    awa, missing = awa_evidence(awa_path, project_id)
    branch = git(root, 'symbolic-ref', '--quiet', '--short', 'HEAD', optional=True)
    head = git(root, 'rev-parse', '--verify', 'HEAD', optional=True)
    if error:
        gaps.append('Existing memory needs inspection and will not be overwritten: ' + error)
    return result('inspect', repo_root=str(root), project_id=project_id, bank_state=bank_state,
                  existing_hashes=existing, sources=sources, source_selection=selected, awa=awa, evidence_gaps=[*gaps, *missing],
                  local_git_context={'branch': branch, 'head': head, 'sources_verified_on_main': False},
                  bounds={'max_sources': MAX_SOURCES, 'max_file_bytes': MAX_FILE, 'max_discovered_files': 4096},
                  writes_performed=False)


def read_source(repo, path):
    root = root_at(repo)
    relative = Path(path)
    if (not isinstance(path, str) or relative.as_posix() != path or relative.is_absolute() or not relative.parts
            or any(ord(c) < 32 or c in '\x7f\x85\u2028\u2029' for c in path)
            or any(p in {'', '.', '..'} or p.startswith('.') for p in relative.parts)
            or any(p in EXCLUDED - {'memory-bank'} for p in relative.parts)):
        raise ContractError('Read an explicit repository-relative source or canonical memory file')
    target = root / relative
    if any(p.is_symlink() for p in [target, *list(target.parents)[:len(relative.parts) - 1]]):
        raise ContractError('Source links must not escape through symlinks')
    if not target.is_file() or target.stat().st_size > MAX_FILE:
        raise ContractError('Source is absent or exceeds the bounded reader')
    raw = target.read_bytes()
    return result('read', ref=relative.as_posix(), sha256=digest(raw), content=raw.decode('utf-8'),
                  content_is_untrusted_data=True, writes_performed=False)


def link(ref, bank_root='memory-bank'):
    # JSON strings and percent-encoded hrefs preserve data without giving source
    # names control over headings, Markdown structure or executable commands.
    from urllib.parse import quote
    label = ref.replace('\\', '\\\\').replace('[', '\\[').replace(']', '\\]')
    href = quote(ref, safe='/:#?=&%') if ref.startswith('https://') else quote('../' * len(Path(bank_root).parts) + ref, safe='/')
    return '[' + label + '](' + href + ')'


def review_value(review, observed, detecting_developer_id):
    if (not isinstance(review, dict) or set(review) != {'inventory', 'existing_memory_reviewed', 'claims', 'questions'}
            or review['inventory'] != observed or type(review['existing_memory_reviewed']) is not bool
            or not isinstance(review['claims'], list) or len(review['claims']) > 64
            or not isinstance(review['questions'], list) or len(review['questions']) > 16):
        raise ContractError('Initialization needs the exact inspected inventory and a bounded reviewed source interpretation')
    if observed['bank_state'] == 'usable' and not review['existing_memory_reviewed']:
        raise ContractError('Read existing applicable memory before creating orientation')
    known = {s['ref']: s for s in [*observed['sources'], *observed['awa']['sources']]}
    questions, claims, groups, gaps = [], [], {}, []
    for claim in review['claims']:
        fields = {'context', 'fact', 'fact_key', 'summary', 'value', 'scope', 'conditions', 'evidence', 'current', 'applicable', 'qualified', 'authority_kind'}
        if (not isinstance(claim, dict) or set(claim) != fields or claim['context'] not in bank.CONTEXTS
                or claim['fact'] not in authority.DOMAINS or not authority.text(claim['fact_key']) or not authority.text(claim['summary'])
                or len(claim['summary']) > 512 or not isinstance(claim['evidence'], list) or not claim['evidence']
                or len(set(claim['evidence'])) != len(claim['evidence']) or any(ref not in known for ref in claim['evidence'])
                or any(type(claim[k]) is not bool for k in ('current', 'applicable', 'qualified'))
                or claim['authority_kind'] not in authority.DOMAINS[claim['fact']]['source-kinds']):
            raise ContractError('Claims require explicit same-question authority, conditions and inspected source references')
        scopes.parse_scope(claim['scope'])
        scopes.conditions(claim['conditions'], allow_unknown=True)
        authority.comparable(claim['value'])
        if not all(claim[k] for k in ('current', 'applicable', 'qualified')) or any(value is None for value in claim['conditions'].values()):
            gaps.append('Unresolved source interpretation: ' + claim['fact_key'])
            continue
        scope_kind, scope_value = scopes.parse_scope(claim['scope']) or (None, None)
        if claim['fact'] == 'implementation':
            branch = observed['local_git_context']['branch']
            if not (scope_kind == 'branch' or any(key in claim['conditions'] for key in ('branch', 'revision'))):
                raise ContractError('Local implementation observations need explicit branch/revision conditions; initialization does not prove main publication')
            if ((scope_kind == 'branch' and (branch is None or scope_value != branch))
                    or ('branch' in claim['conditions'] and (branch is None or claim['conditions']['branch'] != branch))):
                raise ContractError('Implementation branch condition must match the known inspected source branch')
            revision = observed['local_git_context']['head']
            if 'revision' in claim['conditions'] and (revision is None or claim['conditions']['revision'] != revision):
                raise ContractError('Implementation revision condition must match the known inspected source revision')
        # Code or work records alone cannot qualify approved product intent.
        kinds = {known[ref]['kind'] for ref in claim['evidence']}
        if ((claim['fact'] in {'intent', 'requirement'} and not kinds & {'planning', 'documentation'})
                or (claim['fact'] == 'implementation' and not kinds & {'code', 'test'})
                or (claim['fact'] == 'delivery' and not kinds & {'project', 'task', 'milestone'})):
            raise ContractError('Source domain cannot establish this fact; distinguish intent, implementation and planned delivery')
        key = (claim['fact'], claim['fact_key'], claim['scope'], authority.comparable(claim['conditions']))
        groups.setdefault(key, []).append(claim)
    for key, group in groups.items():
        if len({authority.comparable(c['value']) for c in group}) > 1:
            refs = sorted({ref for c in group for ref in c['evidence']})
            questions.append({'question': 'Which applicable current source resolves ' + key[1] + '?',
                              'fact': key[0], 'fact_key': key[1], 'evidence': refs,
                              'detecting_developer_id': detecting_developer_id})
        else:
            # Equivalent reviewed assertions form one concise orientation item.
            c = dict(group[0]); c['evidence'] = sorted({ref for item in group for ref in item['evidence']})
            claims.append(c)
    for item in review['questions']:
        if (not isinstance(item, dict) or set(item) != {'question', 'evidence'} or not authority.text(item['question'])
                or len(item['question']) > 512 or not isinstance(item['evidence'], list)
                or any(ref not in known for ref in item['evidence'])):
            raise ContractError('Product questions must be focused and point to inspected evidence, without invented answers')
        questions.append({**item, 'detecting_developer_id': detecting_developer_id})
    return claims, questions, gaps


def claim_lines(claim, bank_root='memory-bank'):
    condition = json.dumps({'scope': claim['scope'], 'conditions': claim['conditions']}, ensure_ascii=False, sort_keys=True)
    value = authority.comparable(claim['value'])
    return [claim['summary'], '', 'Applicability: `' + condition.replace('`', '\\u0060') + '`.',
            'Reviewed value: `' + value.replace('`', '\\u0060') + '`.',
            'Evidence: ' + ', '.join(link(ref, bank_root) for ref in claim['evidence']) + '.']


def orientation(observed, claims, questions, gaps):
    sections = {}
    bank_root = repository_config.relative(observed['repo_root'])
    def add(path, heading, lines):
        if lines:
            sections.setdefault(path, []).append('## ' + heading + '\n\n' + '\n'.join(lines))
    for kind, path, heading in [('documentation', 'projectbrief.md', 'Documentation to inspect'),
            ('planning', 'systemPatterns.md', 'Planning sources; approval and current applicability require review'),
            ('code', 'techContext.md', 'Implementation sources; existence does not prove behavior'),
            ('test', 'techContext.md', 'Executable tests; existence does not prove they pass')]:
        add(path, heading, ['- ' + link(s['ref'], bank_root) for s in observed['sources'] if s['kind'] == kind][:8])
    add('progress.md', 'AWA projects, tasks and milestones; references do not prove delivery',
        ['- ' + s['kind'] + ': ' + link(s['ref'], bank_root) for s in observed['awa']['sources']][:16])
    for claim in claims:
        add(claim['context'], 'Reviewed ' + claim['fact'] + ': ' + claim['fact_key'], claim_lines(claim, bank_root))
    add('activeContext.md', 'Product questions for the detecting developer',
        ['- ' + q['question'] + (' Evidence: ' + ', '.join(link(ref, bank_root) for ref in q['evidence']) + '.' if q['evidence'] else '') for q in questions])
    add('activeContext.md', 'Evidence gaps; no requirements inferred', ['- ' + gap for gap in [*observed['evidence_gaps'], *gaps]])
    result = {path: '# ' + path.removesuffix('.md') + '\n\n' + '\n\n'.join(body) + '\n' for path, body in sections.items()}
    if any(len(body.encode()) > 16384 for body in result.values()):
        raise ContractError('Orientation exceeds its context bound; tighten reviewed summaries and source selection')
    return result


def new_claims(claims, files, bank_root):
    existing = '\n\n'.join(body for path, body in files.items() if path != 'MEMORY.md')
    def represented(claim):
        block = '## Reviewed ' + claim['fact'] + ': ' + claim['fact_key'] + '\n\n' + '\n'.join(claim_lines(claim, bank_root))
        return block in existing
    return [claim for claim in claims if not represented(claim)]


def preview(repo, review):
    if not isinstance(review, dict) or not isinstance(review.get('inventory'), dict):
        raise ContractError('Inspect and review evidence before initialization')
    inventory = review['inventory']
    observed = inspect(repo, inventory.get('awa', {}).get('path'), inventory.get('source_selection'))
    if observed['bank_state'] == 'needs-inspection':
        raise ContractError('Existing memory is invalid, busy or interrupted; inspect/recover it without overwriting')
    empty = observed['bank_state'] in {'empty', 'absent'}
    context = identity.require(observed['repo_root'], initialize=empty)
    claims, questions, gaps = review_value(review, observed, context['developer_id'])
    if not observed['sources'] and not observed['awa']['sources']:
        raise ContractError('No useful source evidence supplied; inspect documentation/code/tests/planning before creating a bank')
    root = Path(observed['repo_root'])
    base = {} if empty else transactions.files_at(repository_config.bank_path(root))
    claims = new_claims(claims, base, repository_config.relative(root))
    bodies = orientation(observed, claims, questions, gaps)
    files = dict(base)
    if empty:
        proposal = {'project-id': context['project_id'], 'router-updated': storage.now(), 'contexts': {}, 'omitted': {}}
        proof = identity.project.status(root)['evidence']
        if proof['canonical_remote'] is not None:
            proposal['canonical-remote'] = proof['canonical_remote']
        for path in bank.CONTEXTS:
            if path in bodies:
                proposal['contexts'][path] = {'body': bodies[path],
                    **{key: value for key, value in bank.context_route(path).items() if key != 'path'}}
            else:
                proposal['omitted'][path] = 'No distinct useful source context was established during initialization'
        files = bank.render(proposal)['files']
    else:
        added = []
        for path, body in bodies.items():
            if path in base:
                continue  # Existing prose, conditions, sources and provenance stay byte-identical.
            files[path] = body
            added.append(path)
        if added:
            files['MEMORY.md'] = bank.add_context_routes(base['MEMORY.md'], added, storage.now())
    tx = transactions.proposal_value({'schema': transactions.SCHEMA, 'operation_id': str(uuid.uuid4()), 'operation': 'initialize',
        'repo_root': str(root), 'project_id': context['project_id'], 'observed_hashes': observed['existing_hashes'],
        'files': files, 'scope_context': context, 'initialization_evidence': {
            'inventory': observed, 'existing_memory_reviewed': review['existing_memory_reviewed'],
            'claims': review['claims'], 'questions': review['questions'], 'base_router': base.get('MEMORY.md')}})
    security.ensure_safe({path: tx['files'][path] for path in transactions.changed(tx) if path in tx['files']})
    p = {'schema': SCHEMA, 'inventory': observed, 'review': review, 'transaction': tx, 'base_router': base.get('MEMORY.md')}
    return result('preview', proposal=p, questions=questions, evidence_gaps=[*observed['evidence_gaps'], *gaps],
                  changed_memory_paths=[repository_config.relative(root) + '/' + path for path in transactions.changed(tx)],
                  existing_memory_preserved=True, writes_performed=False, verified=False)


def check_sources(inventory):
    root = Path(inventory['repo_root'])
    sources, _ = local_sources(root, inventory['source_selection'])
    awa, _ = awa_evidence(inventory['awa']['path'], inventory['project_id'])
    context = {'branch': git(root, 'symbolic-ref', '--quiet', '--short', 'HEAD', optional=True),
               'head': git(root, 'rev-parse', '--verify', 'HEAD', optional=True), 'sources_verified_on_main': False}
    if sources != inventory['sources'] or awa != inventory['awa'] or context != inventory['local_git_context']:
        raise ContractError('Source evidence changed after initialization preview; inspect and review again')


def transaction_evidence(tx):
    proof = tx.get('initialization_evidence')
    fields = {'inventory', 'existing_memory_reviewed', 'claims', 'questions', 'base_router'}
    if not isinstance(proof, dict) or set(proof) != fields:
        raise ContractError('Initialization source proof is missing; preserve the journal and use rollback')
    review = {key: proof[key] for key in ('inventory', 'existing_memory_reviewed', 'claims', 'questions')}
    return {'schema': SCHEMA, 'inventory': proof['inventory'], 'review': review,
            'transaction': tx, 'base_router': proof['base_router']}


def validate_transaction(tx):
    p = transaction_evidence(tx)
    if (tx['operation'] != 'initialize' or tx['repo_root'] != p['inventory']['repo_root']
            or tx['project_id'] != p['inventory']['project_id'] or tx['observed_hashes'] != p['inventory']['existing_hashes']):
        raise ContractError('Initialization proposal has a different operation/repository')
    # Re-read source evidence before every attempt. Recovery preserves the exact
    # original transaction and never silently imports newer source interpretations.
    check_sources(p['inventory'])
    claims, questions, gaps = review_value(p['review'], p['inventory'], tx['scope_context']['developer_id'])
    # Reconstruct the intended files with the original timestamp and inventory,
    # preventing a substituted transaction from replacing existing bank content.
    original = p['inventory']['existing_hashes']
    prior_files = {path: tx['files'].get(path, '') for path in original}
    for path, sha in original.items():
        if path != 'MEMORY.md' and digest(prior_files[path].encode()) != sha:
            raise ContractError('Initialization must preserve every existing memory file')
    claims = new_claims(claims, prior_files, repository_config.relative(tx['repo_root']))
    bodies = orientation(p['inventory'], claims, questions, gaps)
    if original:
        if not isinstance(p['base_router'], str) or digest(p['base_router'].encode()) != original['MEMORY.md']:
            raise ContractError('Existing router differs from the inspected initialization base')
        added = [path for path in bodies if path not in original]
        expected_router = p['base_router']
        if added:
            updated = bank.parse_router(tx['files']['MEMORY.md'])[0]['router-updated']
            expected_router = bank.add_context_routes(p['base_router'], added, updated)
        if tx['files']['MEMORY.md'] != expected_router:
            raise ContractError('Initialization may add missing context routes, never rewrite existing routes or identity')
    elif p['base_router'] is not None:
        raise ContractError('Empty-bank initialization must not invent prior memory')
    else:
        metadata = {'schema': 'agent-memory/v1', 'project-id': tx['project_id'],
                    'router-updated': bank.parse_router(tx['files']['MEMORY.md'])[0]['router-updated']}
        data, _ = identity.project.observe()
        remote = data['projects'][tx['project_id']]['canonical_remote']
        if remote is not None:
            metadata['canonical-remote'] = remote
        routes = [bank.context_route(path) for path in bank.CONTEXTS if path in bodies]
        omitted = {path: 'No distinct useful source context was established during initialization'
                   for path in bank.CONTEXTS if path not in bodies}
        if tx['files']['MEMORY.md'] != bank.router_text(metadata, routes, omitted):
            raise ContractError('Initial router must match the resolved identity and useful source-linked contexts')
    if set(tx['files']) != {'MEMORY.md', *original, *bodies}:
        raise ContractError('Initialization file set differs from the preserved bank and useful reviewed contexts')
    for path, sha in original.items():
        if path != 'MEMORY.md' and digest(tx['files'].get(path, '').encode()) != sha:
            raise ContractError('Initialization must preserve every existing memory file')
    for path, body in tx['files'].items():
        if path != 'MEMORY.md' and path not in original and body != bodies.get(path):
            raise ContractError('New context differs from the reviewed source-linked orientation')
    security.ensure_safe({path: tx['files'][path] for path in transactions.changed(tx) if path in tx['files']})
    return claims, questions, gaps


def apply(prepared):
    p = prepared.get('proposal', prepared) if isinstance(prepared, dict) else prepared
    if (not isinstance(p, dict) or set(p) != {'schema', 'inventory', 'review', 'transaction', 'base_router'} or p['schema'] != SCHEMA):
        raise ContractError('Invalid initialization proposal')
    tx = transactions.proposal_value(p['transaction'])
    if authority.comparable(p) != authority.comparable(transaction_evidence(tx)):
        raise ContractError('Initialization wrapper differs from the original transaction evidence')
    claims, questions, gaps = validate_transaction(tx)
    root, bank_path = Path(tx['repo_root']), repository_config.bank_path(tx['repo_root'])
    bank.bank_root(bank_path)
    if bank_path.is_symlink() or (bank_path.exists() and not bank_path.is_dir()):
        raise ContractError('Bank location changed after preview; preserve it')
    bank_path.mkdir(parents=True, exist_ok=True)
    receipt = transactions.apply(tx)
    return result('apply', transaction=receipt, questions=questions, evidence_gaps=[*p['inventory']['evidence_gaps'], *gaps],
                  existing_memory_preserved=True, writes_performed=receipt['writes_performed'],
                  verified=receipt['verified'] and receipt['outcome'] == 'finished' and receipt['still_current'],
                  sharing='local-or-pending-sharing')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('inspect', 'read', 'preview'):
        sub = commands.add_parser(name)
        sub.add_argument('--repo', default='.')
        if name == 'inspect':
            sub.add_argument('--awa-evidence')
            sub.add_argument('--source', action='append')
        elif name == 'read':
            sub.add_argument('--path', required=True)
        else:
            sub.add_argument('--review', required=True)
    writer = commands.add_parser('apply')
    writer.add_argument('--proposal', required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == 'inspect':
            value = inspect(args.repo, args.awa_evidence, args.source)
        elif args.command == 'read':
            value = read_source(args.repo, args.path)
        elif args.command == 'preview':
            value = preview(args.repo, load(Path(args.review).read_bytes()))
        else:
            value = apply(load(Path(args.proposal).read_bytes()))
        print(json.dumps(value, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps(result(args.command, error=str(exc), verified=False, inspection_required=True)))
        return 1


if __name__ == '__main__':
    sys.exit(main())
