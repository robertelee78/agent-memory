"""Offline GitHub protocol fixture: policy responses plus real bare Git PR merge refs.

This adapter never uses credentials or a socket. Git's protocol is restricted to
file by World; no response is treated as a substitute for actual Git readback.
"""
import json
from pathlib import Path
import shutil
import shlex
import sys


def install(world, repo, remote, *, protected=False):
    state = world.inside('github-policy.json')
    state.write_text(json.dumps({'repo': str(repo), 'remote': str(remote), 'protected': protected,
        'review': 'REVIEW_REQUIRED' if protected else None, 'merge_state': 'CLEAN', 'checks': 'SUCCESS',
        'rules': [], 'push': True, 'pulls': [], 'events': [],
        'real_git': shutil.which('git', path=world.env['PATH'])}))
    binary = world.inside('fixture-bin'); binary.mkdir(exist_ok=True)
    program = binary/'gh'
    program.write_text('#!'+sys.executable+'\n'+SERVER)
    program.chmod(0o755)
    transport = binary/'git'
    real_git = json.loads(state.read_text())['real_git']
    executable = shlex.quote(real_git)
    rewrite = shlex.quote('url.'+str(remote)+'.insteadOf=https://github.com/fixture/memory.git')
    transport.write_text('#!/bin/sh\nfor arg do\n case "$arg" in\n fetch|push|ls-remote) exec '+executable+
                         ' -c '+rewrite+' "$@" ;;\n esac\ndone\nexec '+executable+' "$@"\n')
    transport.chmod(0o755)
    world.env['PATH'] = str(binary)+':'+world.env['PATH']
    return state


def enforce(world, remote):
    hook = Path(remote)/'hooks/pre-receive'
    hook.write_text('#!'+sys.executable+'\n'+GUARD)
    hook.chmod(0o755)


GUARD = r'''
import json,os,subprocess,sys
from pathlib import Path
d=json.loads((Path(os.environ['AGENT_MEMORY_TEST_ROOT'])/'github-policy.json').read_text())
def git(*a):return subprocess.run([d['real_git'],'-C',d['remote'],*a],text=True,capture_output=True,check=True).stdout.strip()
for line in sys.stdin:
 old,new,ref=line.split()
 if ref=='refs/heads/main' and d['protected']:
  if d['review']!='APPROVED' or d['checks']!='SUCCESS' or d['merge_state']!='CLEAN' or not d['pulls']:sys.exit(1)
  candidate=d['pulls'][0]['head']['sha']
  if git('rev-list','--parents','-n','1',new).split()[1:]!=[old,candidate]:sys.exit(1)
  if git('rev-parse',new+'^{tree}')!=git('rev-parse',candidate+'^{tree}'):sys.exit(1)
'''


SERVER = r'''
import fcntl, json, os, subprocess, sys
from datetime import datetime, timezone
from pathlib import Path
state=Path(os.environ['AGENT_MEMORY_TEST_ROOT'])/'github-policy.json'
# Concurrent clients model one provider, not racing read/modify/write copies
# of its metadata. Git ref updates still run independently against the real
# bare remote. Readers (including the receive hook) see complete JSON only.
state_lock=state.with_suffix('.lock').open('a')
fcntl.flock(state_lock,fcntl.LOCK_EX)
d=json.loads(state.read_text());args=sys.argv[1:]
# Endpoint is the argument following the version header, before optional --input.
endpoint=args[args.index('X-GitHub-Api-Version: 2026-03-10')+1]
method=args[args.index('--method')+1]
body=json.load(sys.stdin) if '--input' in args else None
assert args[:3]==['api','--hostname','github.com']
d['events'].append({'endpoint':endpoint,'method':method,'body':body})
def git(*a):return subprocess.run(['git','-C',d['remote'],*a],check=True,text=True,capture_output=True).stdout.strip()
def response(status,value,extra=''):
 pending=state.with_suffix('.next');pending.write_text(json.dumps(d));os.replace(pending,state)
 print('HTTP/2.0 '+str(status)+' Fixture\nContent-Type: application/json\n'+extra+'\n'+json.dumps(value));sys.exit(0 if status<400 else 1)
repo={'full_name':'fixture/memory','default_branch':'main','permissions':{'push':d['push'],'admin':True},'archived':False,'disabled':False,'allow_merge_commit':True,'pushed_at':len(d['events'])}
if d.get('unavailable'):response(403,{'message':'Fixture permission failure'})
if endpoint=='repos/fixture/memory':response(200,repo)
if endpoint=='repos/fixture/memory/branches/main':response(200,{'name':'main','protected':d['protected'],'commit':{'sha':git('rev-parse','refs/heads/main')}})
if endpoint.startswith('repos/fixture/memory/rules/branches/main?'):
 page=int(endpoint.split('page=')[-1]);rows=d['rules'][(page-1)*100:page*100]
 response(200,rows,'Link: <fixture>; rel="next"\n' if len(d['rules'])>page*100 else '')
if endpoint=='repos/fixture/memory/branches/main/protection':
 if not d['protected']:response(404,{'message':'Branch not protected'})
 response(200,{'required_pull_request_reviews':{'required_approving_review_count':1},'enforce_admins':{'enabled':False},'required_status_checks':{'contexts':['fixture-ci']},'restrictions':None})
if endpoint.startswith('repos/fixture/memory/pulls?'):response(200,d['pulls'])
if endpoint=='repos/fixture/memory/pulls' and method=='POST':
 c=git('rev-parse','refs/heads/'+body['head']);base=git('rev-parse','refs/heads/'+body['base'])
 p={'number':1,'state':'open','html_url':'https://github.com/fixture/memory/pull/1',
 'head':{'sha':c,'ref':body['head'],'repo':{'full_name':'fixture/memory'}},
 'base':{'sha':base,'ref':body['base'],'repo':{'full_name':'fixture/memory'}}}
 d['pulls']=[p];response(201,p)
if endpoint=='graphql':
 p=d['pulls'][0];c=p['head']['sha'];base=git('rev-parse','refs/heads/main')
 tree=git('rev-parse',c+'^{tree}');m=git('commit-tree',tree,'-p',base,'-p',c,'-m','Actual offline GitHub generated merge')
 git('update-ref','refs/pull/1/merge',m)
 response(200,{'data':{'repository':{'pullRequest':{'state':'OPEN','isDraft':False,'headRefOid':c,'baseRefOid':base,
 'reviewDecision':d['review'],'mergeStateStatus':d['merge_state'],
 'commits':{'nodes':[{'commit':{'oid':c,'statusCheckRollup':{'state':d['checks']}}}]},
 'reviewThreads':{'pageInfo':{'hasNextPage':False},'nodes':[]}}}}})
if endpoint.startswith('repos/fixture/memory/commits/'):
 sha=endpoint.split('/')[4]
 if '/statuses?' in endpoint:response(200,[])
 if '/check-runs?' in endpoint:
  runs=[]
  if sha==d['pulls'][0]['head']['sha'] and not d.get('missing_required_check'):
   runs=[{'id':1,'head_sha':sha,'name':'fixture-ci','app':{'id':d.get('check_app',1)},
   'status':'completed' if d['checks']=='SUCCESS' else 'in_progress',
   'conclusion':'success' if d['checks']=='SUCCESS' else None,'completed_at':datetime.now(timezone.utc).isoformat()}]
  response(200,{'check_runs':runs,'total_count':len(runs)})
response(404,{'message':'Unknown fixture route'})
'''
