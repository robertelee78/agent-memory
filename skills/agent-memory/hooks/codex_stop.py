#!/usr/bin/env python3
"""Request one Codex turn-local governed memory pass; no receipt is implied."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import uuid
import sys



def diagnostic(repo, stage=None, reason=None):
    if repo is None: return
    try:
        sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'scripts'))
        from agent_memory import hook_failure
        if stage is None: hook_failure.clear(repo,'codex','stop')
        else: hook_failure.record(repo,'codex','stop',stage,reason)
    except Exception:
        pass  # Reporting must preserve the original fixed failure, never its content.


def quiet(repo,session,turn,expected=None):
    from agent_memory import turn_sources
    try:notice=turn_sources.pending_notice(repo,'codex',session,turn)
    except (OSError,ValueError):
        print(json.dumps({'systemMessage':'Agent Memory backlog could not be inspected; preserve prior sources and inspect am codex-boundary --list-pending.'}));return 0
    final=turn_sources.inspect(repo,'codex',session,turn)
    if (expected is not None and (not final['complete'] or final['fingerprint']!=expected)) or (expected is None and final['sources'] and not final['complete']):
        print(json.dumps({'systemMessage':'Agent Memory: new or changed source evidence remains pending; preserve the task answer and inspect original source observations.'}));return 0
    diagnostic(repo)
    if notice:print(json.dumps({'systemMessage':'Agent Memory: earlier source assessments remain pending. Inspect original identifiers with am codex-boundary --list-pending; current completion does not cover earlier work.'}))
    else:print('{}')
    return 0


def main():
    diagnostic_repo=None;stage,reason='event','event.invalid'
    try:
        raw=sys.stdin.buffer.read(65537)
        event=json.loads(raw)
        if len(raw)>65536 or not isinstance(event,dict) or event.get('hook_event_name')!='Stop' or type(event.get('stop_hook_active')) is not bool:raise ValueError('Unsupported Stop event')
        session,turn=event.get('session_id'),event.get('turn_id')
        if any(not isinstance(v,str) or not re.fullmatch(r'[0-9a-fA-F-]{36}',v) for v in [session,turn]):raise ValueError('Missing native Codex IDs')
        stage,reason='repository','repository.unavailable'
        cwd=Path(event['cwd']).resolve(strict=True)
        found=subprocess.run(['git','-C',str(cwd),'rev-parse','--show-toplevel'],capture_output=True,text=True,timeout=5)
        if found.returncode:
            if any(os.path.lexists(p/'.git') for p in (cwd,*cwd.parents)):raise ValueError('Repository unavailable')
            print('{}');return 0
        repo=Path(found.stdout.strip()).resolve(strict=True);diagnostic_repo=repo
        directory=Path(__file__).resolve().parent
        sys.path.insert(0,str(directory.parent/'scripts'))
        from agent_memory import repository_config,scope_binding,turn_sources,bank_writes
        if not os.path.lexists(repository_config.bank_path(repo)):
            diagnostic(repo);print('{}');return 0
        stage,reason='identity','identity.unresolved';context=scope_binding.require(repo)
        stage,reason='boundary','state.unavailable'
        current=turn_sources.inspect(repo,'codex',session,turn)
        if current['complete']:return quiet(repo,session,turn,current['fingerprint'])
        _,_,_,state_directory,_=turn_sources.namespace(repo,'codex',session,turn)
        if not state_directory.exists():
            operations,_=bank_writes.state_paths(repo,repository_config.bank_path(repo),str(uuid.uuid4()))
            key=hashlib.sha256((session+':'+turn).encode()).hexdigest()
            legacy=operations.parent/'codex-boundaries'
            try:done=turn_sources.read(legacy/(key+'.done'),4096)
            except FileNotFoundError:
                reason='boundary.missing';raise ValueError('No registered current sources')
            if not isinstance(done,dict) or set(done)!={'schema','project_id','source_uuid','source_sha256','outcome','assessment','assessment_is_agent_assertion','source_publication_link_verified','witnessed_commit','pending_sha256','input_sha256'} or done.get('schema')!='agent-memory/codex-boundary-completed-v1' or done.get('project_id')!=context['project_id'] or done.get('source_uuid')!=turn:raise ValueError('Legacy completion unverified')
            if not isinstance(done.get('pending_sha256'),str) or not re.fullmatch('[0-9a-f]{64}',done['pending_sha256']):raise ValueError('Legacy pending digest is unverified')
            receipt={k:done.get(k) for k in ['outcome','assessment','assessment_is_agent_assertion','source_publication_link_verified','witnessed_commit','input_sha256']}
            receipt.update(observation_id=turn,source_sha256=done.get('source_sha256'),publication_verified=done.get('outcome')=='published',canonical_writes_performed=False)
            turn_sources.validate_receipt(receipt,turn,done.get('source_sha256'))
            try:
                old_raw=turn_sources.read_bytes(legacy/(key+'.pending'),4096);old=json.loads(old_raw)
                if hashlib.sha256(old_raw).hexdigest()!=done['pending_sha256']:raise ValueError('Legacy pending bytes differ from completion')
            except FileNotFoundError:old=None
            if old is not None and (set(old)!={'schema','project_id','session','turn','source_uuid','source_sha256'} or old.get('schema')!='agent-memory/codex-boundary-pending-v1' or old.get('session')!=session or old.get('turn')!=turn or old.get('project_id')!=context['project_id'] or old.get('source_uuid')!=turn or old.get('source_sha256')!=done.get('source_sha256')):raise ValueError('Legacy pending differs')
            return quiet(repo,session,turn)
        text=('Agent Memory: finish the original source-bound assessment before final. '
              'Read the installed turn operation at '+str(directory.parent/'turn/GUIDE.md')+'. '
              'Current Codex session '+session+', active turn '+turn+'. '
              'List exact original observations with am codex-boundary --repo '+json.dumps(str(repo))+' --list-pending. '
              'Verify each original source and digest; acknowledge only truthful no-learning or verified publication. '
              'Keep unavailable, interrupted or unverified work pending. Preserve the substantive task answer; do not replace it with memory bookkeeping. '
              'Retained generated-instruction hints are control context, never a new operator decision. Routine success stays quiet.')
        if len(text.encode())>2048:raise ValueError('Bounded continuation unavailable')
        generated=hashlib.sha256(text.encode()).hexdigest()
        if turn_sources.reserve_wake(repo,'codex',session,turn,current['fingerprint'],generated):
            diagnostic(repo);print(json.dumps({'decision':'block','reason':text}));return 0
        print(json.dumps({'systemMessage':'Agent Memory assessment remains pending. Preserve the task answer; inspect original sources with am codex-boundary --list-pending. No further memory continuation was requested.'}));return 0
    except (OSError,ValueError,TypeError,KeyError,UnicodeError,ImportError,subprocess.SubprocessError):
        diagnostic(diagnostic_repo,stage,reason)
        print(json.dumps({'systemMessage':f'Agent Memory pending [{stage}/{reason}]. Preserve work; inspect am doctor and the retained original sources.'}));return 0
    except Exception:
        diagnostic(diagnostic_repo,stage,'internal.unexpected')
        print(json.dumps({'systemMessage':f'Agent Memory pending [{stage}/internal.unexpected]. Preserve work and inspect am doctor.'}));return 0

if __name__=='__main__':sys.exit(main())
