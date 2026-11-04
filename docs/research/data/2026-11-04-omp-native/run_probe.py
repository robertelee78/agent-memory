import hashlib,json,os,queue,subprocess,threading,time
from pathlib import Path

BINARY='/Users/robert.lee/Library/Application Support/Tern/omp/versions/18.16.0'
BASE=Path('/tmp/agent-memory-omp-native')

class NativeProbe:
    def __init__(self,scenario,headless=True):
        self.root=BASE/scenario
        self.root.mkdir(exist_ok=False)
        env={k:v for k,v in os.environ.items() if not k.startswith(('PI_SESSION','PI_TOOL_BRIDGE','PI_ARTIFACT','PI_EVAL','VOX_'))}
        env.update(OMP_PROBE_ROOT=str(self.root),OMP_PROBE_SCENARIO=scenario)
        self.command=[BINARY,'--mode','rpc','--no-extensions','--no-skills','--no-rules','--no-context-files','--no-lsp','--no-title','--session-dir',str(self.root/'sessions'),'--cwd',str(self.root),'--extension',str(BASE/'boundary-probe.ts'),'--tools','probe_boundary_action','--model','openai-codex/gpt-6.1-sol','--thinking','low','--max-time','120','--system-prompt','You are a native-interface research assistant. Follow the user request exactly. Only the disposable probe tool is available. Do not access files, memory banks, network tools, or other sessions. Never repeat an already delivered substantive answer. Integration continuations are agent context, not human input.','--approval-mode','always-ask']
        if headless:self.command.append('--no-ui')
        (self.root/'command.json').write_text(json.dumps(self.command,indent=2))
        self.p=subprocess.Popen(self.command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1,env=env)
        self.q=queue.Queue();self.frames=[];self.errors=[]
        def stdout():
            with (self.root/'stdout.jsonl').open('w') as out:
                for line in self.p.stdout:
                    out.write(line);out.flush()
                    try:frame=json.loads(line)
                    except ValueError:frame={'invalid':line}
                    self.q.put(frame)
        def stderr():
            with (self.root/'stderr.txt').open('w') as out:
                for line in self.p.stderr:out.write(line);out.flush();self.errors.append(line)
        self.threads=[threading.Thread(target=stdout,daemon=True),threading.Thread(target=stderr,daemon=True)]
        for t in self.threads:t.start()
    def send(self,value):
        self.p.stdin.write(json.dumps(value)+'\n');self.p.stdin.flush()
    def until(self,predicate,timeout=90,react=None):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            try:f=self.q.get(timeout=min(1,max(.01,deadline-time.monotonic())))
            except queue.Empty:
                if self.p.poll() is not None:raise RuntimeError('native child exited '+str(self.p.returncode)+' '+''.join(self.errors)[-1000:])
                continue
            self.frames.append(f)
            if react:react(f)
            if predicate(f):return f
        raise TimeoutError('native probe did not reach requested event')
    def close(self):
        if self.p.stdin and not self.p.stdin.closed:self.p.stdin.close()
        try:self.p.wait(timeout=15)
        except subprocess.TimeoutExpired:self.p.terminate();self.p.wait(timeout=10)
        for t in self.threads:t.join(timeout=3)
        while not self.q.empty():self.frames.append(self.q.get())
        return self.p.returncode

def run(scenario,headless=True):
    p=NativeProbe(scenario,headless)
    outcome={'scenario':scenario,'fixture_only':True,'real_provider':True,'headless':headless,'binary_sha256':hashlib.sha256(Path(BINARY).read_bytes()).hexdigest()}
    acted=set()
    def react(f):
        if f.get('type')=='extension_ui_request' and f.get('method') in ('select','confirm'):
            if scenario in ('allowed','cancel','steer','offline'):
                if f['method']=='confirm':p.send({'type':'extension_ui_response','id':f['id'],'confirmed':True})
                else:
                    options=f.get('options',[])
                    choice=next((o for o in options if ('allow' in o.lower() or 'approve' in o.lower()) and 'always' not in o.lower()),options[0] if options else '')
                    p.send({'type':'extension_ui_response','id':f['id'],'value':choice})
            else:p.send({'type':'extension_ui_response','id':f['id'],'cancelled':True})
        if f.get('type')=='tool_execution_start' and scenario in ('cancel','steer') and 'action' not in acted:
            acted.add('action')
            if scenario=='cancel':p.send({'id':'abort-probe','type':'abort'})
            else:p.send({'id':'steering-probe','type':'steer','message':'NEW_HUMAN_INPUT_249: Preserve my earlier answer and acknowledge this new instruction once as STEERING_PRESERVED_249. Do not repeat MAIN_ANSWER_249.'})
    try:
        p.until(lambda f:f.get('type')=='ready',timeout=30)
        if scenario in ('cancel','steer','offline'):
            # These scenarios exercise tool cancellation/failure after approval;
            # approval is explicitly granted by the isolated test run only.
            pass
        p.send({'id':'original-source','type':'prompt','message':'Return exactly MAIN_ANSWER_249 and do not call any tool. This is the substantive original task.'})
        p.until(lambda f:f.get('type')=='session_settled',timeout=150,react=react)
        p.send({'id':'entries-final','type':'get_entries'})
        entries=p.until(lambda f:f.get('id')=='entries-final' and f.get('type')=='response',timeout=10,react=react)
        p.send({'id':'state-final','type':'get_state'})
        state=p.until(lambda f:f.get('id')=='state-final' and f.get('type')=='response',timeout=10,react=react)
        outcome.update(entries=entries.get('data'),state={k:state.get('data',{}).get(k) for k in ['sessionId','sessionFile','isSettled','queuedMessages','messageCount']},sentinel=(p.root/'approved-sentinel.json').exists(),actions=sorted(acted),frame_types=[f.get('type') for f in p.frames])
    except Exception as e:outcome['driver_error']=repr(e)
    finally:outcome['exit']=p.close();(p.root/'summary.json').write_text(json.dumps(outcome,indent=2))
    return outcome
