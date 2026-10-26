"""Public-reader observations against fixed labels; not an acceptance threshold.

Quality cases may run concurrently against an immutable disposable bank; their
latencies are marked contended. Isolated latency samples are separate. No cache
flush is attempted and no cold-cache claim is made.
"""
from __future__ import annotations
import sys
sys.dont_write_bytecode = True
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import threading
import time

from labeled_recall_fixture import corpus, PID


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def inventory(root):
    return {p.relative_to(root).as_posix(): {'sha256': hashlib.sha256(p.read_bytes()).hexdigest(), 'mode': p.stat().st_mode & 0o777} for p in root.rglob('*') if p.is_file()}


def source(root):
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all'], cwd=root, text=True).strip():
        raise ValueError('Qualification source must be clean, including untracked files')
    tracked = {}
    for line in subprocess.check_output(['git', 'ls-tree', '-r', revision], cwd=root, text=True).splitlines():
        descriptor, name = line.split('\t', 1)
        mode, kind, oid = descriptor.split()
        if kind != 'blob' or mode not in {'100644', '100755'}:
            raise ValueError('Only ordinary tracked files qualify in source')
        raw = (root / name).read_bytes()
        actual = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
        if actual != oid or ((root/name).stat().st_mode & 0o777) != (0o755 if mode == '100755' else 0o644):
            raise ValueError('Tracked source bytes/full modes differ from Git')
        tracked[name] = {'sha256': hashlib.sha256(raw).hexdigest(), 'mode': int(mode[-3:], 8)}
    portable = {n.removeprefix('skills/agent-memory/'): v for n, v in tracked.items() if n.startswith('skills/agent-memory/')}
    if inventory(root/'skills/agent-memory') != portable:
        raise ValueError('Complete portable source differs from committed files')
    return {'commit': revision, 'tracked': tracked, 'portable': portable}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', required=True, help='Trusted immutable source for World and fixture template')
    p.add_argument('--baseline', required=True, help='Actual published v0.0.1 checkout')
    p.add_argument('--out', required=True)
    p.add_argument('--counts', nargs='+', type=int, choices=[128,1000,10000,50000], default=[1000,10000,50000])
    p.add_argument('--sources', nargs='+', choices=['baseline','candidate'], default=['baseline','candidate'])
    p.add_argument('--quality-workers', type=int, choices=[1,2,4], default=4)
    p.add_argument('--latency-samples', type=int, choices=[0,1,2], default=2)
    a = p.parse_args()
    root, base = Path(a.root).resolve(strict=True), Path(a.baseline).resolve(strict=True)
    out = Path(a.out).expanduser()
    if not out.is_absolute() or out.is_symlink(): raise ValueError('Output requires an absolute private directory')
    out = out.resolve()
    if any(out.is_relative_to(v) or v.is_relative_to(out) for v in (root,base)): raise ValueError('Output must remain outside sources')
    out.mkdir(mode=0o700,parents=True,exist_ok=True)
    if out.stat().st_uid != os.getuid() or out.stat().st_mode & 0o077: raise ValueError('Preserve output; choose an owner-only private directory')
    if any(out.iterdir()): raise ValueError('Preserve previous observations; choose a new empty private output directory')
    sys.path.insert(0,str(root/'tests'))
    from world import World
    template = json.loads((root/'skills/agent-memory/templates/record-proposal.example.json').read_text())['metadata']
    producer_binding = source(root)
    sources = {name: (value, producer_binding if value == root else source(value)) for name,value in [('baseline',base),('candidate',root)] if name in a.sources}
    if 'baseline' in sources and sources['baseline'][1]['commit'] != '63d3a64c0e535148cc1b5deb9d67169dbbc7c525': raise ValueError('Baseline must be actual published v0.0.1')
    (out/'source-bindings.json').write_text(json.dumps({'producer':producer_binding, 'readers':{n:s for n,(_,s) in sources.items()}},indent=2)+'\n')
    summary = {'schema':'agent-memory/labeled-recall-research-v1','intent':__doc__, 'platform':platform.platform(),'python':platform.python_version(),'processor':platform.processor(),'producer_commit':producer_binding['commit'],'producer_files':{n:producer_binding['tracked'][n] for n in ['scripts/evaluations/labeled_recall_probe.py','scripts/evaluations/labeled_recall_fixture.py']},'argv':sys.argv,'quality_workers':a.quality_workers,'cold_cache_claimed':False,'tokenizer':None,'token_measurements':None,'performance_pass':False,'quality_acceptance_pass':False,'observations':[]}
    commands=out/'commands';commands.mkdir(mode=0o700)
    command_lock=threading.Lock();command_count=0
    def save_summary(): (out/'observations.json').write_text(json.dumps(summary,indent=2)+'\n')
    def invoke(w,pack,helper,*args):
        nonlocal command_count
        argv=[sys.executable,'-I',str(pack/'scripts'/helper),*[str(v) for v in args]]
        with command_lock:
            command_count+=1;name=str(command_count).zfill(5)+'-'+helper
        started=time.perf_counter();failure=None;timed_out=False
        try: r=w.run(argv,check=False,timeout=600,group=True)
        except (subprocess.TimeoutExpired,OSError) as exc:
            failure={'type':type(exc).__name__,'message':str(exc)}
            timed_out=isinstance(exc,subprocess.TimeoutExpired)
            def text(value): return value.decode('utf-8',errors='replace') if isinstance(value,bytes) else value or ''
            r=subprocess.CompletedProcess(argv,None,text(getattr(exc,'output',None)),text(getattr(exc,'stderr',None)))
        # Retain raw process evidence before parsing or interpreting success.
        (commands/(name+'.stdout')).write_text(r.stdout)
        (commands/(name+'.stderr')).write_text(r.stderr)
        state={'argv':argv,'exit':r.returncode,'timed_out':timed_out,'failure':failure,'seconds':time.perf_counter()-started}
        (commands/(name+'.execution.json')).write_text(json.dumps(state,indent=2)+'\n')
        try: value=json.loads(r.stdout) if r.stdout else None;parse_error=None
        except ValueError as exc: value=None;parse_error=str(exc)
        r.parse_error=parse_error;r.timed_out=timed_out;r.execution_failure=failure
        if parse_error:
            (commands/(name+'.parse-error.json')).write_text(json.dumps({'error':parse_error})+'\n')
        return r,value
    def write(w,value,name):
        path=w.inside(name);path.write_bytes(encoded(value));return path
    for count in a.counts:
        files,rows,queries=corpus(template,count)
        lengths=sorted(len(r['body'].encode()) for r in rows)
        fixture={'distinct_lessons':count,'record_versions':len(rows),'canonical_files':len(files),'canonical_bytes':sum(len(x.encode()) for x in files.values()),'fixture_sha256':hashlib.sha256(encoded({n:hashlib.sha256(b.encode()).hexdigest() for n,b in files.items()})).hexdigest(),'labels_sha256':hashlib.sha256(encoded(queries)).hexdigest(),'body_utf8_bytes':{'min':lengths[0],'median':statistics.median(lengths),'p95':lengths[int(.95*(len(lengths)-1))],'max':lengths[-1]}}
        (out/(str(count)+'-labels.json')).write_text(json.dumps({'fixture':fixture,'queries':queries,'fixed_record_sources':rows[:24]},indent=2)+'\n')
        for label,(src,binding) in sources.items():
            directory=out/(str(count)+'-'+label);directory.mkdir(mode=0o700,exist_ok=True)
            with World() as w:
                pack=w.inside('trusted-source/skills/agent-memory');shutil.copytree(src/'skills/agent-memory',pack)
                assert inventory(pack)==binding['portable']
                r,installed=invoke(w,pack,'installer.py','install','--development-source',pack,'--presets','none')
                assert r.returncode==0, installed or r.stderr
                receipt=installed['receipt']; assert receipt['source']['version']==('0.0.1' if label=='baseline' else '0.0.2')
                repo=w.repository();(repo/'feature.py').write_text('DISPATCH_LIMIT = 3\n')
                w.env.update({'GIT_AUTHOR_DATE':'2026-10-25T08:00:00Z','GIT_COMMITTER_DATE':'2026-10-25T08:00:00Z'})
                w.git('add','feature.py',cwd=repo);w.git('commit','-m','Fixed synthetic source evidence',cwd=repo)
                (repo/'feature.py').write_text('DISPATCH_LIMIT = 4  # staged feature work\n');w.git('add','feature.py',cwd=repo)
                bank=repo/'memory-bank';bank.mkdir()
                for name,body in files.items():
                    path=bank/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(body)
                r,status=invoke(w,pack,'developer_identity.py','status','--repo',repo)
                assert r.returncode==0,status
                if status['setup_required']:
                    r,proposal=invoke(w,pack,'developer_identity.py','preview','--create','--label','Fixed quality fixture developer');assert r.returncode==0,proposal
                    r,status=invoke(w,pack,'developer_identity.py','apply','--proposal',write(w,proposal,'developer.json'));assert r.returncode==0,status
                r,proposal=invoke(w,pack,'scope_binding.py','preview','--repo',repo,'--action','bind','--reason','Authored synthetic quality fixture');assert r.returncode==0,proposal
                r,status=invoke(w,pack,'scope_binding.py','apply','--proposal',write(w,proposal,'bind.json'));assert r.returncode==0,status
                validation_started=time.perf_counter()
                r,validation=invoke(w,pack,'bank_contract.py','validate','--repo',repo)
                structural={'exit':r.returncode,'valid':isinstance(validation,dict) and validation.get('valid') is True,'seconds':time.perf_counter()-validation_started,'parse_error':r.parse_error,'timed_out':r.timed_out}
                (directory/'structural-validation.json').write_text(json.dumps(structural,indent=2)+'\n')
                assert structural['valid'] and r.returncode==0,structural
                before=inventory(bank);index=(repo/'.git/index').read_bytes();head=w.git('rev-parse','HEAD',cwd=repo).stdout
                by_path={r['path']:r for r in rows}
                excluded_labels={'timeout-v1','cache-v1','cache-replacement','quota-v1','quota-replacement','expired-endpoint','revoked-fallback','superseded-protocol','unsafe-secret','session-bound'}
                def read(case,mode):
                    name=mode+'-'+case['case'];request=write(w,case['request'],name+'.json')
                    started=time.perf_counter();r,value=invoke(w,pack,'retrieve.py','--repo',repo,'--request',request);seconds=time.perf_counter()-started
                    (directory/(name+'.stdout.json')).write_text(r.stdout);(directory/(name+'.stderr')).write_text(r.stderr)
                    available=r.returncode==0 and isinstance(value,dict) and isinstance(value.get('results'),list) and not value.get('error')
                    selected=[v['path'] for v in value['results']] if available else None
                    expected=set(case['relevant_paths']);actual=set(selected) if selected is not None else set()
                    checks={}
                    if available:
                        checks['canonical_source_hashes']=all(item['source_sha256']==before[item['path']]['sha256'] for item in value['results'])
                        checks['exact_source_metadata_body_and_version']=all((item.get('record')==by_path[item['path']]['metadata'] and item['body']==by_path[item['path']]['body'] and item['selection_class']=='current-record' and item['selection']['current'] is True) if item['path'] in by_path else (item['path']=='projectbrief.md' and item['body']==files['projectbrief.md'] and item['selection_class']=='orientation-only' and item['trust'] is None) for item in value['results'])
                        checks['excluded_current_guidance_withheld']=not any(by_path[item['path']]['label'] in excluded_labels for item in value['results'] if item['path'] in by_path)
                        checks['actual_checkout_context']=value['context'].get('revision')==head.strip() and value['context'].get('branch')=='main'
                        if case['case']=='working-vs-head': checks['working_code_not_certified']=value['code_working_bytes_verified'] is False
                        if case['case']=='orientation-and-typed': checks['orientation_separate']=any(v['selection_class']=='orientation-only' and v['trust'] is None for v in value['results'])
                    checks['unsafe_body_and_path_not_disclosed']=not any(s in r.stdout+r.stderr for s in ['WITHHELD_REPLACEMENT_FIXTURE','WITHHELD_SOURCE_FIXTURE','topics/records/unsafe-secret.md','topics/records/quota-replacement.md','topics/records/cache-replacement.md'])
                    return {'case':case['case'],'mode':mode,'latency_regime':'isolated-sequential' if mode.startswith('latency') else ('concurrent-immutable-bank' if a.quality_workers>1 else 'sequential-quality'),'seconds':seconds,'exit':r.returncode,'available':available,'parse_error':r.parse_error,'timed_out':r.timed_out,'execution_failure':r.execution_failure,'expected_paths':sorted(expected),'selected_paths':selected,'found_relevant':sorted(expected & actual) if available else None,'missed_relevant':sorted(expected-actual) if available else None,'unexpected_paths':sorted(actual-expected) if available else None,'metrics':value.get('metrics') if isinstance(value,dict) else None,'encoded_response_bytes':len(r.stdout.encode()),'checks':checks,'safety_contract_failures':[name for name,ok in checks.items() if not ok],'error':value.get('error') if isinstance(value,dict) else None}
                latency=[read(queries[2],'latency-'+str(n)) for n in range(a.latency_samples)]
                with ThreadPoolExecutor(max_workers=a.quality_workers) as pool: observed=list(pool.map(lambda c:read(c,'quality'),queries))
                preserved=(inventory(bank)==before and (repo/'.git/index').read_bytes()==index and w.git('rev-parse','HEAD',cwd=repo).stdout==head and (repo/'feature.py').read_text()=='DISPATCH_LIMIT = 4  # staged feature work\n')
                assert preserved and inventory(pack)==binding['portable']
                observation={'source_label':label,'source_commit':binding['commit'],'artifact_digest':receipt['source']['artifact_digest'],'declared_installed_version':receipt['source']['version'],'fixture':fixture,'structural_validation':structural,'observed_head':head.strip(),'feature_index_head_bank_preserved':preserved,'latency_samples':latency,'quality_cases':observed,'available_cases':sum(v['available'] for v in observed),'relevant_found':sum(len(v['found_relevant'] or []) for v in observed),'relevant_missed':sum(len(v['missed_relevant'] or []) for v in observed),'unexpected_returned':sum(len(v['unexpected_paths'] or []) for v in observed),'safety_contract_failure_count':sum(len(v['safety_contract_failures']) for v in observed)}
                summary['observations'].append(observation);save_summary()
                print(json.dumps({k:v for k,v in observation.items() if k not in {'latency_samples','quality_cases'}},sort_keys=True),flush=True)
    assert source(root)==producer_binding and all(source(src)==binding for src,binding in sources.values()),'Source changed during qualification'
    summary['sources_unchanged']=True;save_summary()


if __name__=='__main__': main()
