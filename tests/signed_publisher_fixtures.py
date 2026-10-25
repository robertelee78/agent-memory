"""Bounded external gh/Git adapter for the real publisher; never a live release."""
import json
import os
from pathlib import Path
import shutil
import sys

from signed_release_fixtures import SignedRelease

GH = r'''#!/usr/bin/env python3
import json,os,shutil,subprocess,sys
from pathlib import Path
path=Path(__STATE__)
s=json.loads(path.read_text()); a=sys.argv[1:]
s['calls'].append(a)
def save(): path.write_text(json.dumps(s))
def git(*args,optional=False):
 r=subprocess.run([s['git'],'--git-dir',s['bare'],*args],capture_output=True,text=True,env=os.environ)
 if r.returncode and not optional: fail('APPARATUS: bare Git command failed',70)
 return r.stdout.strip() if r.returncode==0 else None
def fail(message,code=1):
 save();print(message,file=sys.stderr);sys.exit(code)
def emit(value):
 save();print(json.dumps(value));sys.exit(0)
def option(name): return a[a.index(name)+1] if name in a else None
def record():
 r=dict(s['release']);r['assets']=[{'name':n,'size':v['size']} for n,v in s['assets'].items()]
 return r
prefix='repos/robertelee78/agent-memory'
if a[:2]==['release','view']:
 if s['release'] is None: fail('release not found')
 emit({'apiUrl':'https://api.github.com/'+prefix+'/releases/1'})
if a[0]=='api':
 route=a[1].removeprefix('https://api.github.com/')
 if route==prefix+'/branches/main': emit({'protected':False,'commit':{'sha':git('rev-parse','main')}})
 if route==prefix+'/rules/branches/main': emit([])
 if route==prefix+'/releases/1':
  if s['release'] is None: fail('HTTP 404')
  emit(record())
 if route.startswith(prefix+'/releases/tags/'):
  # Pending drafts deliberately cannot be resolved by the published-tag API.
  if s['release'] is None or s['release']['draft']: fail('HTTP 404')
  emit(record())
 if route==prefix+'/releases/latest':
  if s['release'] is None or s['release']['draft']: fail('HTTP 404')
  emit(record())
 if route.startswith(prefix+'/commits/'):
  value=git('rev-parse',route.rsplit('/',1)[1],optional=True)
  if value is None: fail('HTTP 404')
  if option('--jq')=='.sha': save();print(value);sys.exit(0)
  emit({'sha':value})
 if route.startswith(prefix+'/git/ref/tags/'):
  ref='refs/tags/'+route.rsplit('/',1)[1];value=git('rev-parse','--verify',ref,optional=True)
  if value is None: fail('HTTP 404')
  emit({'object':{'sha':value,'type':git('cat-file','-t',value)}})
 if route.startswith(prefix+'/git/tags/'):
  lines=git('cat-file','-p',route.rsplit('/',1)[1]).splitlines()
  emit({'object':{'sha':lines[0].split()[1],'type':lines[1].split()[1]}})
 if route.startswith(prefix+'/git/trees/'):
  entries=[]
  for row in git('ls-tree',route.rsplit('/',1)[1]).splitlines():
   meta,name=row.split('\t');mode,kind,sha=meta.split()
   entries.append({'path':name,'mode':mode,'type':kind,'sha':sha})
  emit({'tree':entries,'truncated':False})
 if route.startswith(prefix+'/compare/main...'):
  source=route.split('...',1)[1];main=git('rev-parse','main')
  if main==source: status='identical'
  elif git('merge-base','--is-ancestor',main,source,optional=True) is not None: status='ahead'
  elif git('merge-base','--is-ancestor',source,main,optional=True) is not None: status='behind'
  else: status='diverged'
  emit({'status':status})
 fail('APPARATUS: unsupported gh API route: '+route,70)
if a[:2]==['release','create']:
 if s['release'] is not None: fail('APPARATUS: duplicate release creation',70)
 s['release']={'tag_name':a[2],'draft':True,'prerelease':False,
               'target_commitish':option('--target'),'html_url':'https://github.com/'+s['repository']+'/releases/tag/'+a[2]}
 s['created']+=1;emit({'created':True})
if a[:2]==['release','upload']:
 source=Path(a[3]);name=source.name
 if name in s['assets']: fail('APPARATUS: upload replaced an existing asset',70)
 target=Path(s['storage'])/name;shutil.copyfile(source,target)
 s['assets'][name]={'path':str(target),'size':target.stat().st_size}
 if s['mode']=='partial-upload' and not s['interrupted']:
  s['interrupted']=True;fail('fixture transport lost response after first upload')
 if s['mode']=='tag-before' and len(s['assets'])==4:
  git('update-ref','refs/tags/'+s['tag'],s['foreign'])
 emit({'uploaded':name})
if a[:2]==['release','download']:
 names=[option('--pattern')] if option('--pattern') else list(s['assets'])
 destination=Path(option('--dir'))
 for name in names: shutil.copyfile(s['assets'][name]['path'],destination/name)
 save();sys.exit(0)
if a[:2]==['release','edit']:
 s['release']['draft']=False;s['published']+=1
 target=s['foreign'] if s['mode']=='tag-after' else s['source']
 git('update-ref','refs/tags/'+s['tag'],target)
 if s['mode'] in ('harmless-descendant','bootstrap-revert','runtime-revert'):
  git('update-ref','refs/heads/main',s['foreign'])
 emit({'published':True})
fail('APPARATUS: unsupported gh command',70)
'''

GIT = r'''#!/usr/bin/env python3
import json,os,subprocess,sys
from pathlib import Path
path=Path(__STATE__);s=json.loads(path.read_text());a=sys.argv[1:];verb=0
while verb<len(a) and a[verb]=='-c': verb+=2
if a[verb:verb+3]==['remote','get-url','origin']:
 print('https://github.com/robertelee78/agent-memory.git');sys.exit(0)
if a[verb] in ('ls-remote','push'):
 a=[s['bare'] if v=='origin' else v for v in a]
 if a[verb]=='push':
  s['pushes']+=1
  if s['mode']=='push-race':
   subprocess.run([s['git'],'--git-dir',s['bare'],'update-ref','refs/heads/main',s['foreign']],check=True,env=os.environ)
  path.write_text(json.dumps(s))
r=subprocess.run([s['git'],*a],env=os.environ);sys.exit(r.returncode)
'''


class SignedPublisher(SignedRelease):
    def prepare_publisher(self,mode='normal'):
        (self.root/'initial.md').write_text('Previous default remains active before publication.\n')
        self.w.git('add','.',cwd=self.root)
        self.w.git('commit','-m','Previous default source',cwd=self.root)
        base=self.w.git('rev-parse','HEAD',cwd=self.root).stdout.strip()
        super().prepare()
        script=self.root/'scripts/distribution/release.py'
        text=script.read_text().replace("RAW_DEFAULT = 'https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh'",
                                       'RAW_DEFAULT = '+repr(self.base+'/raw/main/install.sh'))
        script.write_text(text)
        self.w.git('add','.',cwd=self.root);self.w.git('commit','-m','Isolated publisher transport',cwd=self.root)
        source=self.w.git('rev-parse','HEAD',cwd=self.root).stdout.strip()
        branch='integrate/v0.0.3'
        self.w.git('checkout','-b',branch,cwd=self.root)
        foreign=base
        if mode in ('harmless-descendant','bootstrap-revert','runtime-revert','push-race','tag-before','tag-after','preexisting-tag'):
            parent=base if mode in ('push-race','tag-before','tag-after','preexisting-tag') else source
            self.w.git('checkout','-b','foreign',parent,cwd=self.root)
            if mode=='bootstrap-revert': (self.root/'install.sh').write_text('#!/bin/sh\necho old bootstrap\n')
            elif mode=='runtime-revert':
                path=self.root/'skills/agent-memory/templates/foreign.txt';path.write_text('different runtime\n')
            else: (self.root/'foreign.md').write_text('Unrelated concurrent documentation.\n')
            self.w.git('add','.',cwd=self.root);self.w.git('commit','-m','Concurrent fixture change',cwd=self.root)
            foreign=self.w.git('rev-parse','HEAD',cwd=self.root).stdout.strip()
            self.w.git('checkout',branch,cwd=self.root)
        bare=self.w.inside('publisher-origin.git');self.w.run(['git','init','--bare',bare])
        self.w.git('push',bare,base+':refs/heads/main',source+':refs/heads/'+branch,
                   foreign+':refs/heads/foreign',cwd=self.root)
        self.w.git('remote','add','origin','https://github.com/robertelee78/agent-memory.git',cwd=self.root)
        storage=self.w.inside('uploaded-assets');storage.mkdir(mode=0o700)
        state=self.w.inside('publisher-authority.json')
        value={'git':shutil.which('git',path=self.w.env['PATH']),'bare':str(bare),'source':source,'foreign':foreign,
               'base':base,'tag':'v0.0.3','repository':'robertelee78/agent-memory','storage':str(storage),
               'release':None,'assets':{},'mode':mode,'interrupted':False,'created':0,'published':0,'pushes':0,'calls':[]}
        if mode=='preexisting-tag':
            self.w.run([value['git'],'--git-dir',bare,'update-ref','refs/tags/v0.0.3',foreign])
        state.write_text(json.dumps(value))
        tools=self.w.inside('publisher-tools');tools.mkdir(mode=0o700)
        for name,body in [('gh',GH),('git',GIT)]:
            path=tools/name;path.write_text(body.replace('__STATE__',repr(str(state))));path.chmod(0o755)
        self.w.env['PATH']=str(tools)+os.pathsep+self.w.env['PATH']
        self.publisher_state=state;self.publisher_branch=branch
        self.notes=self.w.inside('release-notes.md');self.notes.write_text('Isolated publisher qualification.\n')
        return value

    def publish(self):
        return self.w.run([sys.executable,'-B',self.root/'scripts/distribution/release.py','publish',
                           '--private-key',self.key,'--notes-file',self.notes,
                           '--integration-branch',self.publisher_branch],check=False,timeout=120)

    def authority(self): return json.loads(self.publisher_state.read_text())
