import concurrent.futures,datetime,hashlib,json,os,pathlib,platform,stat,subprocess,sys,time
root=pathlib.Path(sys.argv[1]).resolve();out=pathlib.Path(sys.argv[2]).resolve();manifest=pathlib.Path(sys.argv[3]);out.mkdir(parents=True,exist_ok=True)
data=json.loads(manifest.read_text());commit=data['commit']
def verify(label):
 errors=[]
 for row in data['files']:
  p=root/row['path']
  try:
   raw=os.readlink(p).encode() if row['mode']=='120000' else p.read_bytes()
   sha=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
   mode=format((stat.S_IMODE(p.lstat().st_mode)&0o111)!=0,'d')
   if sha!=row['blob'] or (row['mode']!='120000' and (mode=='1') != (row['mode']=='100755')):errors.append(row['path'])
  except Exception: errors.append(row['path'])
 result={'phase':label,'commit':commit,'files':len(data['files']),'mismatches':errors}
 (out/('source-'+label+'.json')).write_text(json.dumps(result,indent=2)+'\n')
 if errors:raise RuntimeError('source bytes or modes mismatch')
verify('before')
cases={
 'v2-refresh':'test_quarantined_sharing_v2.QuarantinedSharingVersionTwoContract.test_local_unsafe_body_stays_local_while_safe_peer_learning_is_imported_and_recalled',
 'v2-publish':'test_quarantined_sharing_v2.QuarantinedSharingVersionTwoContract.test_unsafe_authority_body_is_not_imported_and_independent_safe_local_delta_can_publish',
 'recovery':'test_refresh_recovery.RefreshRecoveryContract',
 'later-merge':'test_multi_machine_merge_replay.MultiMachineMergeReplay',
 'simultaneous':'test_multi_machine_race.MultiMachineRace',
 'cross-clone':'test_multi_machine_compatible.MultiMachineCompatible',
 'conflict':'test_multi_machine_conflicts.MultiMachineConflicts',
}
def run(item):
 name,test=item; argv=[sys.executable,'-B','-m','unittest','-v',test];env={**os.environ,'PYTHONPATH':'tests:tests/contracts','PYTHONDONTWRITEBYTECODE':'1'};start=datetime.datetime.now(datetime.timezone.utc).isoformat(); t=time.monotonic()
 try:
  r=subprocess.run(argv,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=840);code=r.returncode;stdout=r.stdout;stderr=r.stderr;timeout=False
 except subprocess.TimeoutExpired as e:code=None;stdout=e.stdout or b'';stderr=e.stderr or b'';timeout=True
 elapsed=round(time.monotonic()-t,3);(out/(name+'.stdout')).write_bytes(stdout);(out/(name+'.stderr')).write_bytes(stderr)
 receipt={'schema':'am-local-test-receipt-v1','source_commit':commit,'case':name,'argv':argv,'cwd':str(root),'env_overrides':{'PYTHONPATH':'tests:tests/contracts','PYTHONDONTWRITEBYTECODE':'1'},'platform':platform.platform(),'hostname':platform.node(),'python':sys.version,'started_at':start,'elapsed_seconds':elapsed,'exit_code':code,'timeout_seconds':840,'timed_out':timeout,'stdout_sha256':hashlib.sha256(stdout).hexdigest(),'stderr_sha256':hashlib.sha256(stderr).hexdigest()}
 (out/(name+'.json')).write_text(json.dumps(receipt,indent=2)+'\n');print(name,code,elapsed,flush=True);return code==0 and not timeout
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool: results=list(pool.map(run,cases.items()))
verify('after');print(json.dumps({'passed':sum(results),'total':len(results),'source_commit':commit}),flush=True)
if not all(results):sys.exit(1)
