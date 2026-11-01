"""Bounded, private evidence for failures before publisher authentication."""
import json, os, signal, subprocess, sys, tempfile, time, unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'skills/agent-memory/scripts'))
from agent_memory import release_distribution as r
FAKE='''import os,sys,time
from pathlib import Path
a=sys.argv; p=Path(os.environ['COUNTER']); n=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n))
m=os.environ['FAULT']; code=0;status='200';body=b'complete'
if m in ('receive','flaky','redact') and (m!='flaky' or n==1):code=56;status='000';body=b'partial'
if m=='http' and n==1:code=22;status='503'
if m=='tls':code=60;status='503'
if m=='missing':code=22;status='404'
if m=='rate':code=22;status='429'
if m=='malformed':code=22;status='503bad'
if m=='oversize':body=b'x'*100
if m=='slow':time.sleep(10)
Path(a[a.index('--output')+1]).write_bytes(body)
sys.stdout.write(status);sys.stderr.write('receive error')
if m=='redact':sys.stderr.write('\\nAuthorization: Bearer AUTH_SECRET\\nhttps://a:PASS_SECRET@host/?token=URL_SECRET\\ntoken=TOKEN_SECRET\\n\\x1b[31m')
sys.exit(code)
'''
class DownloadDiagnostics(unittest.TestCase):
 def setUp(self):
  t=tempfile.TemporaryDirectory();self.addCleanup(t.cleanup);self.root=Path(t.name);self.counter=self.root/'count'
  f=self.root/'curl';f.write_text('#!'+sys.executable+'\n'+FAKE);f.chmod(0o755)
  p=patch.dict(os.environ,{'PATH':str(self.root)+os.pathsep+os.environ['PATH'],'COUNTER':str(self.counter),'FAULT':'receive'});p.start();self.addCleanup(p.stop)
 def failure(self,mode='receive',maximum=1024):
  os.environ['FAULT']=mode
  with self.assertRaises(r.DownloadError) as c:r.download('https://example.com/release-manifest.json?token=URL_SECRET',maximum)
  return c.exception
 def test_recovery_discards_partial_bytes(self):
  os.environ['FAULT']='flaky';self.assertEqual(r.download('https://example.com/asset',1024),b'complete');self.assertEqual(self.counter.read_text(),'2')
 def test_private_bounded_redacted_evidence(self):
  e=self.failure('redact');d=e.details;self.assertEqual(d['attempt_count'],3);self.assertEqual(d['reason'],'receive failure')
  f=Path(d['diagnostic_path']);self.assertEqual(f.stat().st_mode&0o777,0o600);self.assertEqual(f.parent.stat().st_mode&0o777,0o700);self.assertLessEqual(f.stat().st_size,8192)
  text=f.read_text()+str(e)+e.repair
  for s in ('AUTH_SECRET','PASS_SECRET','URL_SECRET','TOKEN_SECRET','\x1b'):self.assertNotIn(s,text)
  self.assertEqual(d['stage'],'release manifest');f.unlink();f.parent.rmdir()
 def test_no_retry_for_permanent_errors(self):
  for m,reason in [('tls','certificate validation failure'),('missing','HTTP rejection'),('rate','HTTP rejection'),('malformed','HTTP rejection'),('oversize','size-limit')]:
   with self.subTest(mode=m):
    self.counter.unlink(missing_ok=True);e=self.failure(m,16 if m=='oversize' else 1024);self.assertEqual(e.details['attempt_count'],1);self.assertEqual(e.details['reason'],reason)
 def test_http_retry_and_deadline(self):
  os.environ['FAULT']='http';self.assertEqual(r.download('https://example.com/asset',1024),b'complete');self.counter.unlink();start=time.monotonic()
  with patch.object(r,'DOWNLOAD_BUDGET',0.2):e=self.failure('slow')
  self.assertLess(time.monotonic()-start,2);self.assertEqual(e.details['reason'],'deadline');self.assertEqual(e.details['attempt_count'],1)
 def test_retention_failure_preserves_primary_error(self):
  with patch.object(r,'retain_download_failure',side_effect=OSError('disk full')):e=self.failure()
  self.assertIsNone(e.details['diagnostic_path']);self.assertIn('receive failure',str(e));self.assertIn('Could not retain',e.repair)
 def test_resource_signal_not_retried(self):
  with patch.object(r.subprocess,'run',return_value=subprocess.CompletedProcess([],-signal.SIGXFSZ)):e=self.failure()
  self.assertEqual(e.details['reason'],'size-limit');self.assertEqual(e.details['attempt_count'],1)

from world import World
from signed_release_fixtures import SignedRelease
from runtime_update_fixtures import snapshot, receipt_path
import shutil

WRAPPER='''import os,sys
from pathlib import Path
a=sys.argv[1:]
if '/release-manifest.json' in a[-1]:
 p=Path(os.environ['COUNTER']);n=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n))
 if os.environ['FAULT']=='receive' or n==1:
  Path(a[a.index('--output')+1]).write_bytes(b'partial');sys.stderr.write('curl: (56) receive failure');sys.stdout.write('000');sys.exit(56)
os.execv(os.environ['REAL_CURL'],['curl']+a)
'''
class DownloadBootstrap(unittest.TestCase):
 def wrapper(self,w,mode):
  root=w.inside('fault-bin');root.mkdir();f=root/'curl';f.write_text('#!'+sys.executable+'\n'+WRAPPER);f.chmod(0o755)
  counter=w.inside('counter');w.env.update(PATH=str(root)+os.pathsep+w.env['PATH'],REAL_CURL=shutil.which('curl'),FAULT=mode,COUNTER=str(counter))
  return counter
 def test_transient_manifest_failure_recovers_to_authenticated_install(self):
  with World() as w,SignedRelease(w) as service:
   service.prepare();counter=self.wrapper(w,'flaky');p=service.install();self.assertEqual(p.returncode,0,p.stdout+p.stderr);d=json.loads(p.stdout)
   self.assertTrue(d['verified']);self.assertEqual(d['receipt']['source']['kind'],'release');self.assertGreaterEqual(int(counter.read_text()),2)
 def test_failed_download_never_activates_and_has_guidance(self):
  with World() as w,SignedRelease(w) as service:
   service.prepare();counter=self.wrapper(w,'receive');before={k:snapshot(p) for k,p in w.paths.items()};p=service.install()
   self.assertEqual(p.returncode,1,p.stdout+p.stderr);d=json.loads(p.stdout);self.assertEqual(d['download']['attempt_count'],3);self.assertIn('Private diagnostic:',d['repair'])
   self.assertFalse(receipt_path(w).exists());self.assertEqual(before,{k:snapshot(p) for k,p in w.paths.items()})
 def test_invalid_signature_is_not_retried(self):
  with World() as w,SignedRelease(w) as service:
   service.prepare();path='/download/v0.0.3/release-manifest.json.sshsig';service.responses[path]=b'invalid';p=service.install()
   self.assertNotEqual(p.returncode,0);self.assertEqual(service.requests.count(path),1);self.assertFalse(receipt_path(w).exists())

import contextlib, io
from agent_memory import console
class PreviewGuidance(unittest.TestCase):
 def test_preview_is_not_installation_and_does_not_request_restart(self):
  out=io.StringIO()
  with contextlib.redirect_stdout(out):console.render({'schema':'agent-memory/install-plan-v1','restart_clients':['codex'],'targets':[],'unrelated_option':'OPTION_SECRET'},'install','am',0)
  text=out.getvalue();self.assertIn('does not install am',text);self.assertIn('without --check or --dry-run',text);self.assertNotIn('Restart',text);self.assertNotIn('PATH',text);self.assertNotIn('OPTION_SECRET',text)

class ReviewRegressions(unittest.TestCase):
 def test_diagnostic_redaction_normalizes_before_matching(self):
  raw=(b'curl: Authorization: Bearer AUTH_SECRET\ntoken="SAFE_PREFIX SECRET_TAIL"\n'
       b'CURL_CA_BUNDLE=/private/ACCOUNT_SECRET.pem\nhttps:\x00//host/path?sig=URL_SECRET\n'
       b'auth_token=AUTH_TOKEN_SECRET client_secret="CLIENT SECRET_TAIL"\n'
       b'curl: (77) error setting certificate file: /private/BARE_ACCOUNT_SECRET.pem')
  detail=r.safe_download_detail(raw)
  e=r.DownloadError({'stage':'release manifest','host':'example.com'},[{'reason':'transport','curl_exit':56,'http_status':None,'detail':detail}],0.1)
  f=Path(e.details['diagnostic_path']);text=f.read_text()+str(e)+json.dumps(e.details)+e.repair
  for secret in ('AUTH_SECRET','SECRET_TAIL','ACCOUNT_SECRET','URL_SECRET','AUTH_TOKEN_SECRET','SECRET_TAIL','BARE_ACCOUNT_SECRET','https://'):self.assertNotIn(secret,text)
  f.unlink();f.parent.rmdir()
 def test_http_permanent_status_wins_over_transient_curl_exit(self):
  for status in (429,403,404):
   def run(*args,**kwargs):
    kwargs['stdout'].write(str(status).encode());return subprocess.CompletedProcess([],56)
   with patch.object(r.subprocess,'run',side_effect=run) as mocked:
    with self.assertRaises(r.DownloadError) as caught:r.download('https://example.com/file',1024)
   self.assertEqual(mocked.call_count,1);self.assertEqual(caught.exception.details['http_status'],status)
 def test_report_policy_failure_preserves_primary_error(self):
  with patch.object(r,'retain_download_failure',side_effect=r.DistributionError('Unsafe root')):
   e=r.DownloadError({'stage':'archive','host':'example.com'},[{'reason':'transport','curl_exit':56,'http_status':None,'detail':''}],0.1)
  self.assertIn('receive failure',str(e));self.assertIsNone(e.details['diagnostic_path'])

class BootstrapPreview(unittest.TestCase):
 def test_signed_preview_uses_packaged_guidance_and_preserves_profiles(self):
  with World() as w,SignedRelease(w) as service:
   service.prepare();before={k:snapshot(p) for k,p in w.paths.items()};w.env['AM_TEST_SECRET']='ENV_SECRET'
   p=w.run(['bash',service.root/'install.sh','--check'],check=False,timeout=45)
   self.assertEqual(p.returncode,0,p.stdout+p.stderr);self.assertIn('does not install am',p.stdout)
   self.assertIn('without --check or --dry-run',p.stdout);self.assertNotIn('Restart',p.stdout);self.assertNotIn('PATH',p.stdout);self.assertNotIn('ENV_SECRET',p.stdout)
   self.assertFalse(receipt_path(w).exists());self.assertEqual(before,{k:snapshot(p) for k,p in w.paths.items()})
