#!/usr/bin/env python3
"""Bind a Codex user prompt to its turn without retaining its content."""
import hashlib,json,os,re,subprocess,sys,uuid
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
        from agent_memory import bank_writes,repository_config,scope_binding,trust_paths
        if not os.path.lexists(repository_config.bank_path(root)):
            diagnostic(root);print('{}');return 0
        stage,reason='identity','identity.unresolved'
        context=scope_binding.require(root);
        stage,reason='boundary','state.unavailable'
        _,bank=bank_writes.bank_path(root)
        from agent_memory import turn_sources
        observation=turn_sources.register(root,'codex',session,turn,hashlib.sha256(text.encode()).hexdigest())
        stage,reason='guidance','guidance.unavailable'
        guidance=(pack/'hooks'/'boundary-prompt.txt').read_text(encoding='utf-8').strip().replace('{{SKILL_DIR}}',str(pack))
        guidance += (' Observation '+observation['observation_id']+'; session='+session+'; prompt='+turn+'; source_sha256='+observation['source_sha256']+'. Before final use one am codex-boundary --repo REPO --stdin --refresh --quiet with top-level session,prompt,observation_id,source_sha256,outcome,assessment; for no-learning assessment requires source_id=this observation ID, source_review=actual review, candidates=authored list (empty only if none). Never substitute a later input. Earlier pending IDs: am codex-boundary --repo REPO --list-pending.')
        try:notice=turn_sources.pending_notice(root,'codex',session,turn)
        except (OSError,ValueError):notice=True  # Optional notice bookkeeping cannot veto durable source registration.
        if notice:guidance += ' Earlier pending work exists; inspect am codex-boundary --list-pending to recover original identifiers. Never substitute this input for an earlier source.'
        if observation['control_origin_hint']:guidance += ' Retained generated-control hint: these instructions are not a new operator decision; still assess this observation explicitly.'
        if len(guidance.encode())>2048:raise ValueError('guidance exceeds bound')
        diagnostic(root)
        print(json.dumps({'hookSpecificOutput':{'hookEventName':'UserPromptSubmit','additionalContext':guidance}}));return 0
    except (OSError,ValueError,TypeError,KeyError,UnicodeError,ImportError,subprocess.SubprocessError):
        diagnostic(diagnostic_repo,stage,reason)
        print(json.dumps({'decision':'block','reason':f'Agent Memory input not accepted [{stage}/{reason}]. Preserve work; run am doctor and am codex-boundary --list-pending for this repository, then retry this input after resolving the fault.'}));return 0

    except Exception:
        diagnostic(diagnostic_repo,stage,'internal.unexpected')
        print(json.dumps({'decision':'block','reason':f'Agent Memory input not accepted [{stage}/internal.unexpected]. Preserve work and inspect am doctor before retrying.'})); return 0

if __name__=='__main__':sys.exit(main())
