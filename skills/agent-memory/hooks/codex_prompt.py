#!/usr/bin/env python3
"""Bind a Codex user prompt to its turn without retaining its content."""
import hashlib,json,os,re,subprocess,sys,uuid
from pathlib import Path

def main():
    raw=sys.stdin.buffer.read(65537)
    try:
        event=json.loads(raw)
        session,turn,text=event['session_id'],event['turn_id'],event['prompt']
        if len(raw)>65536 or event.get('hook_event_name')!='UserPromptSubmit' or any(not isinstance(v,str) or not re.fullmatch(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}',v) for v in (session,turn)) or not isinstance(text,str) or not text or len(text.encode())>65536:
            raise ValueError('invalid event')
        cwd=Path(event['cwd']).resolve(strict=True)
        result=subprocess.run(['git','-C',str(cwd),'rev-parse','--show-toplevel'],capture_output=True,text=True,timeout=5)
        if result.returncode:
            if any(os.path.lexists(p/'.git') for p in (cwd,*cwd.parents)):raise ValueError('repository unavailable')
            print('{}');return 0
        root=Path(result.stdout.strip()).resolve(strict=True);pack=Path(__file__).resolve().parent.parent
        sys.path.insert(0,str(pack/'scripts'))
        from agent_memory import bank_writes,repository_config,scope_binding,trust_paths
        if not os.path.lexists(repository_config.bank_path(root)):
            print('{}');return 0
        context=scope_binding.require(root);_,bank=bank_writes.bank_path(root)
        operations,_=bank_writes.state_paths(root,bank,str(uuid.uuid4()));ledger=operations.parent/'codex-boundaries'
        trust_paths.inspect(ledger);ledger.mkdir(parents=True,exist_ok=True,mode=0o700);trust_paths.state(ledger)
        key=hashlib.sha256((session+':'+turn).encode()).hexdigest();data=json.dumps({'schema':'agent-memory/codex-boundary-pending-v1','project_id':context['project_id'],'source_uuid':turn,'source_sha256':hashlib.sha256(text.encode()).hexdigest()},sort_keys=True).encode()
        dfd=os.open(ledger,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try:
            try:done=os.open(key+'.done',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=dfd)
            except FileNotFoundError:done=None
            if done is not None:
                try:
                    import stat
                    info=os.fstat(done)
                    if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:raise ValueError('unsafe completed record')
                    with os.fdopen(done,'rb',closefd=False) as stream: completed=json.loads(stream.read(4097))
                    if (completed.get('schema')!='agent-memory/codex-boundary-completed-v1' or completed.get('project_id')!=context['project_id'] or completed.get('source_uuid')!=turn or completed.get('source_sha256')!=hashlib.sha256(text.encode()).hexdigest()):raise ValueError('completed prompt source differs')
                finally:os.close(done)
                print('{}');return 0
            try:fd=os.open(key+'.pending',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=dfd)
            except FileExistsError:
                existing=os.open(key+'.pending',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=dfd)
                try:
                    import stat
                    info=os.fstat(existing)
                    if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:raise ValueError('unsafe pending record')
                    with os.fdopen(existing,'rb',closefd=False) as stream: previous=json.loads(stream.read(4097))
                    if previous!=json.loads(data):raise ValueError('prompt source differs from retained boundary')
                finally:os.close(existing)
                print('{}');return 0
            with os.fdopen(fd,'wb') as f:f.write(data);f.flush();os.fsync(f.fileno())
            os.fsync(dfd)
        finally:os.close(dfd)
        guidance=(pack/'hooks'/'boundary-prompt.txt').read_text(encoding='utf-8').strip().replace('{{SKILL_DIR}}',str(pack))
        print(json.dumps({'hookSpecificOutput':{'hookEventName':'UserPromptSubmit','additionalContext':guidance}}));return 0
    except (OSError,ValueError,TypeError,KeyError,UnicodeError,ImportError,subprocess.SubprocessError):
        print(json.dumps({'systemMessage':'Agent Memory could not bind the Codex prompt; memory capture remains pending.'}));return 0

if __name__=='__main__':sys.exit(main())
