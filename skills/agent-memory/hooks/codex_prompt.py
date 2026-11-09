#!/usr/bin/env python3
"""Bind a Codex user prompt to its turn without retaining its content."""
import hashlib,json,os,re,subprocess,sys,uuid
sys.dont_write_bytecode = True
from pathlib import Path


def diagnostic(repo, stage=None, reason=None):
    if repo is None: return
    try:
        sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'scripts'))
        from agent_memory import hook_failure
        if stage is None: hook_failure.clear(repo,'codex','prompt')
        else: hook_failure.record(repo,'codex','prompt',stage,reason)
    except Exception:
        pass  # Reporting must preserve the original fixed failure, never its content.


def main():
    diagnostic_repo = None
    scope_binding = None
    retained_input=None
    stage, reason = "event", "event.invalid"
    try:
        raw=sys.stdin.buffer.read(65537)
        event=json.loads(raw)
        session,turn,text=event['session_id'],event['turn_id'],event['prompt']
        if len(raw)>65536 or event.get('hook_event_name')!='UserPromptSubmit' or any(not isinstance(v,str) or not re.fullmatch(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}',v) for v in (session,turn)) or not isinstance(text,str) or not text or len(text.encode())>65536:
            raise ValueError('invalid event')
        stage,reason='repository','repository.unavailable'
        cwd=Path(event['cwd']).resolve(strict=True)
        result=subprocess.run(['git','-C',str(cwd),'rev-parse','--show-toplevel'],capture_output=True,text=True,timeout=5)
        if result.returncode:
            if any(os.path.lexists(p/'.git') for p in (cwd,*cwd.parents)):raise ValueError('repository unavailable')
            print('{}');return 0
        root=Path(result.stdout.strip()).resolve(strict=True);diagnostic_repo=root;pack=Path(__file__).resolve().parent.parent
        sys.path.insert(0,str(pack/'scripts'))
        from agent_memory import repository_config,scope_binding
        if not os.path.lexists(repository_config.bank_path(root)):
            diagnostic(root);print('{}');return 0
        from agent_memory import turn_sources
        stage,reason='boundary','state.unavailable'
        enrollment=scope_binding.private_enrollment_context(root)
        native_ids=[event[key] for key in ('event_id','prompt_id') if event.get(key) is not None]
        native=(native_ids[0] if native_ids and all(isinstance(value,str) and value and len(value.encode())<=512
                and not any(ord(char)<32 for char in value) for value in native_ids) and len(set(native_ids))==1 else None)
        reference=event.get('transcript_path')
        if not isinstance(reference,str) or not Path(reference).is_absolute() or len(reference.encode())>2048 or any(ord(char)<32 for char in reference):
            reference=None
        if enrollment.held and native is None:
            retained_input=turn_sources.retain_unidentified_input(root,'codex',session,turn,text.encode(),reference)
            stage,reason='native-event','identity.unavailable'
            raise ValueError('Retained source lacks an unambiguous native event identity')
        observation=turn_sources.register(root,'codex',session,turn,hashlib.sha256(text.encode()).hexdigest(),native_source_id=native,source_reference=reference)
        stage,reason='guidance','guidance.unavailable'
        guidance=(pack/'hooks'/'boundary-prompt.txt').read_text(encoding='utf-8').strip()
        guidance += (' Observation '+observation['observation_id']+'; session='+session+'; prompt='+turn+'; source_sha256='+observation['source_sha256']+'. Before final use one am codex-boundary --repo REPO --stdin --refresh --quiet with top-level session,prompt,observation_id,source_sha256,outcome,assessment; for no-learning assessment requires source_id=this observation ID, source_review=actual review, candidates=authored list (empty only if none). Never substitute a later input. Earlier pending IDs: am codex-boundary --repo REPO --list-pending.')
        if enrollment.held:
            guidance += ' Recovery-pending: input retained privately; continue the task. Until the verified hold resolves, no canonical admission, refresh or source acknowledgement. Never fabricate no-learning or publication.'
        try:notice=turn_sources.pending_notice(root,'codex',session,turn)
        except (OSError,ValueError):notice=True  # Optional notice bookkeeping cannot veto durable source registration.
        if notice:guidance += ' Earlier pending work exists; use am codex-boundary --list-pending for original IDs.'
        if observation['control_origin_hint']:guidance += ' Generated-control hint, not a new operator decision; assess this source.'
        if len(guidance.encode())>2048:raise ValueError('guidance exceeds bound')
        scope_binding.recheck_private_enrollment(enrollment)
        diagnostic(root)
        print(json.dumps({'hookSpecificOutput':{'hookEventName':'UserPromptSubmit','additionalContext':guidance}}));return 0
    except (OSError,ValueError,TypeError,KeyError,UnicodeError,ImportError,subprocess.SubprocessError) as exc:
        if scope_binding is not None and isinstance(exc,scope_binding.IdentityUnresolved):
            stage,reason='identity','identity.unresolved'
        diagnostic(diagnostic_repo,stage,reason)
        retained=(' Unidentified input retained privately as '+retained_input['retention_id']+'; it is not an observation. Restore the original native event ID/reference before enrollment.' if retained_input else '')
        print(json.dumps({'decision':'block','reason':f'Agent Memory input not accepted [{stage}/{reason}]. Preserve work; run am doctor and am codex-boundary --list-pending for this repository, then retry this input after resolving the fault.'+retained}));return 0

    except Exception:
        diagnostic(diagnostic_repo,stage,'internal.unexpected')
        print(json.dumps({'decision':'block','reason':f'Agent Memory input not accepted [{stage}/internal.unexpected]. Preserve work and inspect am doctor before retrying.'})); return 0

if __name__=='__main__':sys.exit(main())
