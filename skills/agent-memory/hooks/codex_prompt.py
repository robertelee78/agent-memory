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
            print('{}');return 0
        stage,reason='identity','identity.unresolved'
        context=scope_binding.require(root);
        stage,reason='boundary','state.unavailable'
        _,bank=bank_writes.bank_path(root)
        operations,_=bank_writes.state_paths(root,bank,str(uuid.uuid4()));ledger=operations.parent/'codex-boundaries'
        trust_paths.inspect(ledger);ledger.mkdir(parents=True,exist_ok=True,mode=0o700);trust_paths.state(ledger)
        key=hashlib.sha256((session+':'+turn).encode()).hexdigest();data=json.dumps({'schema':'agent-memory/codex-boundary-pending-v1','project_id':context['project_id'],'session':session,'turn':turn,'source_uuid':turn,'source_sha256':hashlib.sha256(text.encode()).hexdigest()},sort_keys=True).encode()
        dfd=os.open(ledger,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try:
            reason='boundary.unsafe'
            try:done=os.open(key+'.done',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=dfd)
            except FileNotFoundError:done=None
            if done is not None:
                reason='boundary.unsafe'
                try:
                    import stat
                    info=os.fstat(done)
                    if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:raise ValueError('unsafe completed record')
                    with os.fdopen(done,'rb',closefd=False) as stream: completed=json.loads(stream.read(4097))
                    reason='boundary.source_mismatch'
                    if (not isinstance(completed,dict) or completed.get('schema')!='agent-memory/codex-boundary-completed-v1' or completed.get('project_id')!=context['project_id'] or completed.get('source_uuid')!=turn or completed.get('source_sha256')!=hashlib.sha256(text.encode()).hexdigest()):raise ValueError('completed prompt source differs')
                finally:os.close(done)
                diagnostic(root);print('{}');return 0
            reason='state.unavailable'
            try:fd=os.open(key+'.pending',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=dfd)
            except FileExistsError:
                reason='boundary.unsafe'
                existing=os.open(key+'.pending',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=dfd)
                try:
                    import stat
                    info=os.fstat(existing)
                    if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:raise ValueError('unsafe pending record')
                    with os.fdopen(existing,'rb',closefd=False) as stream: previous=json.loads(stream.read(4097))
                    reason='boundary.source_mismatch'
                    if previous!=json.loads(data):raise ValueError('prompt source differs from retained boundary')
                finally:os.close(existing)
                diagnostic(root);print('{}');return 0
            with os.fdopen(fd,'wb') as f:f.write(data);f.flush();os.fsync(f.fileno())
            os.fsync(dfd)
        finally:os.close(dfd)
        stage,reason='guidance','guidance.unavailable'
        guidance=(pack/'hooks'/'boundary-prompt.txt').read_text(encoding='utf-8').strip().replace('{{SKILL_DIR}}',str(pack))
        diagnostic(root)
        print(json.dumps({'hookSpecificOutput':{'hookEventName':'UserPromptSubmit','additionalContext':guidance}}));return 0
    except (OSError,ValueError,TypeError,KeyError,UnicodeError,ImportError,subprocess.SubprocessError):
        diagnostic(diagnostic_repo,stage,reason)
        print(json.dumps({'systemMessage':f'Agent Memory Codex prompt pending [{stage}/{reason}]. Run am doctor for this repository; preserve work and retry the actual prompt after resolving the reported cause.'}));return 0

    except Exception:
        diagnostic(diagnostic_repo,stage,'internal.unexpected')
        print(json.dumps({'systemMessage':f'Agent Memory Codex prompt pending [{stage}/internal.unexpected]. Preserve work and run am doctor; no capture or publication is established.'})); return 0

if __name__=='__main__':sys.exit(main())
