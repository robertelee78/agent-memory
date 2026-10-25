"""Source-qualified public-command spike; no production signing material."""
from pathlib import Path
import gzip,hashlib,json,ssl,sys
sys.dont_write_bytecode=True
root=Path(sys.argv[1]).resolve();out=Path(sys.argv[2]);out.mkdir(exist_ok=True)
sys.path.insert(0,str(root/'tests'));sys.path.insert(0,str(root/'skills/agent-memory/scripts'))
from world import World
from runtime_update_fixtures import snapshot,owned,receipt_path,bank
from signed_release_fixtures import SignedRelease
from agent_memory import artifact
base=artifact.inspect(root/'skills/agent-memory')
results=[]
with World() as w, SignedRelease(w) as service:
 repo=w.repository();(repo/'feature.py').write_text('base=True\n')
 w.git('add','.',cwd=repo);w.git('commit','-m','Base feature',cwd=repo)
 memory=bank(w,repo);w.git('add','.',cwd=repo);w.git('commit','-m','Existing durable memory',cwd=repo)
 (repo/'feature.py').write_text('unfinished=True\n');w.git('add','feature.py',cwd=repo)
 service.prepare();installed=service.install(mode='copy',presets='codex')
 if installed.returncode:raise RuntimeError('APPARATUS: signed baseline installation failed: '+installed.stdout+installed.stderr)
 receipt=json.loads(installed.stdout)['receipt'];cli=Path(receipt['bin_dir'])/'am'
 manifest=service.advance();good=dict(service.responses)
 archive_path='/download/v0.0.4/agent-memory.tar.gz'
 original=service.server.RequestHandlerClass
 service.boundary_mode=None
 class Handler(original):
  protocol_version='HTTP/1.1'
  def do_GET(self):
   mode=service.boundary_mode
   if self.path==archive_path and mode in ('truncated-http-body','oversize-chunked-archive'):
    raw=good[archive_path]
    self.send_response(200);self.send_header('Connection','close')
    if mode=='truncated-http-body':self.send_header('Content-Length',str(len(raw)));body=raw[:-37]
    else:self.send_header('Transfer-Encoding','chunked');body=raw+b'excess'
    self.end_headers();self.close_connection=True
    try:
     if mode=='oversize-chunked-archive':
      self.wfile.write(('%x\r\n'%len(body)).encode()+body+b'\r\n0\r\n\r\n')
     else:self.wfile.write(body)
     self.wfile.flush()
    except (BrokenPipeError,ConnectionResetError,ssl.SSLEOFError):pass
    return
   return super().do_GET()
 service.server.RequestHandlerClass=Handler
 def state():
  return {'targets':owned(receipt),'receipt':receipt_path(w).read_bytes().hex(),
          'lifecycle_state':snapshot(w.paths['state']/'agent-memory/install'),
          'bank':snapshot(memory),'feature':snapshot(repo/'feature.py'),
          'index':snapshot(repo/'.git/index'),'head':w.git('rev-parse','HEAD',cwd=repo).stdout.strip()}
 before=state();before_hash=hashlib.sha256(json.dumps(before,sort_keys=True).encode()).hexdigest()
 for case in ('truncated-http-body','oversize-metadata','oversize-archive','oversize-chunked-archive','oversize-expanded-archive'):
  service.responses=dict(good);service.boundary_mode=case
  if case=='oversize-metadata':service.responses['/api/latest']=b' '*(65536+1)
  elif case=='oversize-archive':service.responses[archive_path]=good[archive_path]+b'excess'
  elif case=='oversize-expanded-archive':
   blob=gzip.compress(b'\0'*(16777216+4096*1024+1),mtime=0)
   altered=dict(manifest);altered.update(archive_size=len(blob),archive_sha256=hashlib.sha256(blob).hexdigest())
   raw,sig=service.signature(altered)
   service.responses['/download/v0.0.4/release-manifest.json']=raw
   service.responses['/download/v0.0.4/release-manifest.json.sshsig']=sig
   service.responses[archive_path]=blob
  result=w.run([cli,'--json','update'],check=False,timeout=120)
  (out/(case+'.stdout')).write_text(result.stdout);(out/(case+'.stderr')).write_text(result.stderr)
  value=json.loads(result.stdout);after=state()
  error=value.get('error','')
  wanted='Release expansion exceeds its bound' if case=='oversize-expanded-archive' else 'Signed release download failed'
  passed=result.returncode==1 and value.get('verified') is False and wanted in error and after==before
  record={'case':case,'exit':result.returncode,'error':error,'passed':passed,
          'active_targets_receipt_state_bank_feature_index_head_preserved':after==before,
          'before_state_sha256':before_hash,'after_state_sha256':hashlib.sha256(json.dumps(after,sort_keys=True).encode()).hexdigest()}
  results.append(record);print(json.dumps(record),flush=True)
  if not passed:raise RuntimeError('PRODUCT: transport boundary did not meet refusal/preservation contract')
summary={'candidate':'b068486990d1b05a9f731347d7db08281b13b484','base_artifact_digest':base['artifact_digest'],
         'portable_files':base['file_count'],'supplemental_cases':results,'passed':all(r['passed'] for r in results),
         'registered_method_counts_unchanged':True,'production_private_key_used':False,'public_release_mutated':False}
(out/'results.json').write_text(json.dumps(summary,indent=2)+'\n')
