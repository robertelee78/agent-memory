#!/usr/bin/env python3
"""Actual Linux-arm64/Podman-VM CPU qualification; reserved serial host slot only.

Usage: python3 -B qualify_linux.py --config recipe.json --output actual-RUN --reserved-host-slot
The exact public archive must already be acquired at config.archive; no downloads,
builds, package installation, user-store adoption, image pull, or foreign execution.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import secrets
import socket
import tarfile
import time
import urllib.error
import urllib.request
from discover import command, save, sha

CLOSURE = r'''
import hashlib,json,os,platform
entries=[]
paths=sorted(set(line.split()[-1] for line in open('/proc/1/maps') if len(line.split())>=6 and line.split()[-1].startswith('/')))
for path in paths:
    try:
        p=os.path.realpath(path)
        h=hashlib.sha256()
        with open(p,'rb') as f:
            for b in iter(lambda:f.read(1048576),b''): h.update(b)
        entries.append({'mapped_path':path,'real_path':p,'sha256':h.hexdigest(),'bytes':os.stat(p).st_size})
    except OSError as e: entries.append({'mapped_path':path,'unavailable':str(e)})
print(json.dumps({'platform':platform.platform(),'uname':list(os.uname()),'uid':os.getuid(),'maps':open('/proc/1/maps').read(),'files':entries,'status':open('/proc/1/status').read()},indent=2))
'''


def request(root, label, base, route, payload=None, timeout=300):
    data = None if payload is None else json.dumps(payload).encode()
    save(root / (label + '-request.json'), {"url": base + route, "method": "GET" if data is None else "POST", "body": payload})
    req = urllib.request.Request(base + route, data=data, headers={"Content-Type": "application/json"})
    start = time.time()
    try:
        response = urllib.request.urlopen(req, timeout=timeout)
    except urllib.error.HTTPError as exc:
        response = exc
    with response:
        raw = response.read()
        try:
            parsed = json.loads(raw)
        except ValueError:
            parsed = None
        result = {"status": response.status, "body": parsed, "raw_body": raw.decode(errors='replace'),
                  "duration_seconds": time.time()-start, "headers": dict(response.headers.items())}
    save(root / (label + '-response.json'), result)
    return result


def vectors(result, dimension, count):
    if result['status'] != 200:
        raise ValueError('Expected actual embeddings, received non-200')
    rows = result['body']['data']
    if len(rows) != count or sorted(x['index'] for x in rows) != list(range(count)):
        raise ValueError('Embedding row/index mismatch')
    values = [r['embedding'] for r in sorted(rows, key=lambda r:r['index'])]
    if any(len(v) != dimension or not all(isinstance(x,(int,float)) and math.isfinite(x) for x in v) or not any(x != 0 for x in v) for v in values):
        raise ValueError('Actual embedding dimension/finite/nonzero failure')
    return values


def cosine(a, b):
    return math.fsum(x*y for x,y in zip(a,b)) / math.sqrt(math.fsum(x*x for x in a)*math.fsum(y*y for y in b))


def cell(root, config, profile, release, owner):
    root.mkdir()
    expected = config['profiles'][profile]
    weight = Path(expected['weight']).resolve()
    if weight.stat().st_size != expected['bytes'] or sha(weight) != expected['sha256']:
        raise ValueError('Exact requested weight unavailable or different')
    save(root/'weight.json', {'path': str(weight), 'bytes':weight.stat().st_size, 'sha256': expected['sha256']})
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0))
        port = sock.getsockname()[1]
    name = 'am255-linux-' + owner + '-' + profile
    base = 'http://127.0.0.1:' + str(port)
    argv = ['podman','create','--name',name,'--label','am255.linux.owner='+owner,'--pull=never','--platform=linux/arm64',
            '--cpus','4','--memory','12g','--memory-swap','12g','--pids-limit','128','--cap-drop','ALL',
            '--security-opt','no-new-privileges','--read-only','--tmpfs','/tmp:rw,noexec,size=64m', '--image-volume=ignore',
            '--publish','127.0.0.1:'+str(port)+':8080','--mount','type=bind,src='+str(release)+',dst=/release,ro=true',
            '--mount','type=bind,src='+str(weight)+',dst=/model.gguf,ro=true','--env','LD_LIBRARY_PATH=/release',
            '--entrypoint','/release/llama-server',config['carrier_image'],
            '--host','0.0.0.0','--port','8080','--no-webui','--model','/model.gguf','--alias',expected['alias'],
            '--ctx-size',str(expected['context']),'--parallel','1','-b','8192','-ub','8192','-t','4','-tb','4',
            '-ngl','0','--pooling','last','--embedding','--flash-attn','off','--fit','off','--cache-ram','0','--offline']
    created = command(root,'create',argv)
    if created['exit'] != 0:
        return {'profile':profile,'missing_prerequisite':'Container create failed; raw failure retained'}
    cid = created['stdout'].strip()
    save(root/'owner.json',{'name':name,'id':cid,'label':owner,'physical_resources':'new container overlay + bounded tmpfs; no volume, graph, user-store, or persistent provider cache',
         'borrowed_read_only':[config['carrier_image'],str(release),str(weight)], 'network':'loopback publication; default container network is NOT egress isolation'})
    result = {'profile':profile, 'actual_linux_vm':True, 'bare_metal':False, 'performance_acceptance':False}
    try:
        started = command(root,'start',['podman','start',cid])
        inspected = command(root,'inspect-start',['podman','inspect',cid])
        if started['exit'] != 0:
            result['missing_prerequisite'] = 'Owned container did not start; see raw startup evidence'
            return result
        attempts = []
        deadline = time.monotonic()+180
        while True:
            try:
                with urllib.request.urlopen(base+'/health',timeout=2) as health:
                    attempts.append({'status':health.status,'body':health.read().decode()})
                    if health.status == 200:
                        break
            except (OSError, urllib.error.HTTPError) as exc:
                attempts.append({'error':str(exc)})
            # Container state is a readiness condition, not an external job poll.
            state = __import__('subprocess').run(['podman','inspect','--format','{{.State.Status}}',cid],capture_output=True,text=True)
            if state.stdout.strip() in ('exited','dead') or time.monotonic() > deadline:
                save(root/'readiness.json',attempts)
                result['missing_prerequisite'] = 'Exact public binary failed readiness/ABI/runtime closure or exceeded 180 seconds; see raw container logs/state'
                return result
            time.sleep(.25)
        save(root/'readiness.json',attempts)
        command(root,'version',['podman','exec',cid,'/release/llama-server','--version'])
        command(root,'loaded-closure',['podman','exec',cid,'python3','-c',CLOSURE])
        command(root,'runtime-ldd',['podman','exec',cid,'ldd','/release/llama-server'])
        models = request(root,'models',base,'/v1/models')
        request(root,'props',base,'/props')
        listed = models['body']['data']
        if len(listed) != 1 or listed[0]['id'] != expected['alias']:
            raise ValueError('Actual single-model discovery failed')
        def embed(label,texts,model=None):
            response = request(root,label,base,'/v1/embeddings',{'model':model or listed[0]['id'],'input':texts,'encoding_format':'float'})
            return vectors(response,expected['dimension'],len(texts))
        spec = config['probes']
        single = embed('single',[spec['short']])
        unknown = embed('unknown-alias',[spec['short']],spec['unknown_alias'])
        result['discovery'] = {'single_model':True,'alias':listed[0]['id'],'metadata':listed[0],
            'identity_evidence':'owned byte-hashed weight + release archive + launch; response echo alone is NOT identity',
            'unknown_alias_returned_embeddings':True,'unknown_alias_equals_loaded_single':unknown==single}
        own = embed('own-text',[d['own'] for d in spec['documents']])
        related = embed('relationship-text',[d['own']+spec['relationship_separator']+next(x['own'] for x in spec['documents'] if x['id']==d['neighbor']) for d in spec['documents']])
        queries = embed('queries',[spec['query_prefix']+q['text'] for q in spec['queries']])
        result['own_text_controls'] = {'scope':'synthetic portability controls; NOT independent held-out quality or inherited macOS quality',
            'own_relationship_cosines':[cosine(a,b) for a,b in zip(own,related)],
            'ranks_own':[[spec['documents'][j]['id'] for j in sorted(range(len(own)),key=lambda j:cosine(query,own[j]),reverse=True)] for query in queries],
            'ranks_relationship':[[spec['documents'][j]['id'] for j in sorted(range(len(related)),key=lambda j:cosine(query,related[j]),reverse=True)] for query in queries]}
        result['batch_counts'] = {}
        for count in (1,4,8,9):
            rows = embed('batch-'+str(count),spec['batch_rows'][:count])
            result['batch_counts'][str(count)] = len(rows)
        long_input = spec['over_context']['repeat']*(expected['context']+64)+spec['over_context']['suffix']
        tokenized = request(root,'tokenize-over-context',base,'/tokenize',{'content':long_input,'add_special':True})
        over = request(root,'over-context',base,'/v1/embeddings',{'model':expected['alias'],'input':[long_input]})
        malformed = request(root,'invalid-input',base,'/v1/embeddings',{'model':expected['alias'],'input':{'not':'text'}})
        empty = request(root,'empty-input',base,'/v1/embeddings',{'model':expected['alias'],'input':[]})
        result.update({'dimension':expected['dimension'],'finite_nonzero_embeddings':True,
            'limits':{'context':expected['context'],'actual_over_context_status':over['status'],'tokenized_count':len(tokenized['body'].get('tokens',[])),
                'over_context_error':over['body'],'malformed_status':malformed['status'],'empty_status':empty['status'],
                'no_truncation_observed_for_over_context_control':over['status']>=400 and not (over['body'] or {}).get('data'),
                'batch9_is_server_observation_not_AM_policy_acceptance':True,
                'truncation_assurance':'This exact synthetic over-context refusal only; not a generic BYO assurance or arbitrary-input proof'}})
        command(root,'stats',['podman','stats','--no-stream','--format','json',cid])
        return result
    finally:
        command(root,'logs',['podman','logs',cid])
        command(root,'inspect-final',['podman','inspect',cid])
        # Exact full container ID from our create receipt only; never a name/global prune.
        command(root,'stop',['podman','stop','--time','15',cid])
        command(root,'remove',['podman','rm',cid])
        absence = command(root,'absence',['podman','container','exists',cid])
        save(root/'cleanup.json',{'id':cid,'absent':absence['exit']==1,'volumes_created':[],
             'borrowed_weights_images_preserved':True,'forensic_media_erasure':False})
        if absence['exit'] != 1:
            raise RuntimeError('Owned Linux container disposition not verified')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--reserved-host-slot',action='store_true',required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True,exist_ok=False)
    config = json.loads(args.config.read_text())
    owner = secrets.token_hex(6)
    save(root/'frozen-inputs.json',{'config':config,'config_sha256':sha(args.config),'script_sha256':sha(__file__),
         'helper_sha256':sha(Path(__file__).with_name('discover.py')),'am_source':config['am_baseline'], 'ssg':config['ssg'],
         'owner':owner,'start_unix':time.time(),'limits':'shared Darwin host; Linux-arm64 CPU container in AppleHV VM; not bare metal or clean performance'})
    before = command(root,'containers-before',['podman','ps','-aq','--no-trunc'])
    command(root,'volumes-before',['podman','volume','ls','--format','json'])
    command(root,'vm-platform',['podman','machine','ssh','podman-machine-default','uname','-a'])
    command(root,'host-load',['uptime'])
    archive = Path(config['archive']).resolve()
    if sha(archive) != config['archive_sha256']:
        raise ValueError('Public release archive identity mismatch')
    release_root = archive.parent / ('extracted-'+owner)
    release_root.mkdir()
    with tarfile.open(archive,'r:gz') as tar:
        inventory = [{'path':m.name,'bytes':m.size,'type':str(m.type),'link':m.linkname} for m in tar]
        save(root/'archive-members.json',inventory)
        tar.extractall(release_root,filter='data')
    release = release_root / 'llama-b9360'
    save(root/'release-files.json',[{'path':str(p.relative_to(release)), 'bytes':p.stat().st_size,'sha256':sha(p),'symlink':p.is_symlink()} for p in sorted(release.rglob('*')) if p.is_file()])
    (root/'release-LICENSE').write_bytes((release/'LICENSE').read_bytes())
    # Preflight executes only the exact release binary in an owned nonservice carrier.
    preflight = command(root,'runtime-preflight',['podman','run','--rm','--pull=never','--platform=linux/arm64','--cpus','1','--memory','512m',
        '--network','none','--image-volume=ignore','--read-only','--mount','type=bind,src='+str(release)+',dst=/release,ro=true',
        '--env','LD_LIBRARY_PATH=/release','--entrypoint','/release/llama-server',config['carrier_image'],'--version'])
    results = []
    try:
        if preflight['exit'] == 0:
            for profile in ('reference','alternate'):
                try:
                    result = cell(root/profile,config,profile,release,owner)
                except Exception as exc:
                    result = {'profile':profile,'error':type(exc).__name__+': '+str(exc),'pass':False}
                save(root/(profile+'-result.json'),result)
                results.append(result)
        else:
            results.append({'missing_prerequisite':'Pinned release cannot execute in existing pinned carrier; actual ABI/runtime diagnostic retained. No package install or substitute binary.'})
    finally:
        after = command(root,'containers-after',['podman','ps','-aq','--no-trunc'])
        command(root,'volumes-after',['podman','volume','ls','--format','json'])
        save(root/'report.json',{'schema':'am255/linux-arm64-portability-v1','results':results,
             'baseline_containers_preserved':set(before['stdout'].split()) <= set(after['stdout'].split()),
             'new_container_ids_remaining':sorted(set(after['stdout'].split())-set(before['stdout'].split())),
             'new_container_ids_may_include_concurrent_other_owners':True,
             'extracted_release_retained_at':str(release_root),'production':False,'legal_acceptance':False,'G1_acceptance':False,
             'unknowns':['No native Linux bare-metal or Linux-amd64 embedding execution','No independent-suite quality or clean performance pass',
               'Release/source binding is upstream publication metadata, not reproducible-build attestation','Runtime carrier legal/BOM and model attribution unknowns remain pending']})
    print(json.dumps(results,indent=2))

if __name__ == '__main__':
    main()
