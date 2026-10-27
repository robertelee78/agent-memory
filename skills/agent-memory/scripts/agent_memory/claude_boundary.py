"""Acknowledge one source-bound Claude boundary without inventing publication."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import uuid

from . import bank_writes, memory_publication, scope_binding, sharing_limits, sharing_recovery, trust_paths
from .developer_identity import load
from .frontmatter import ContractError

UUID = re.compile(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\Z')


def complete(repo, value):
    fields={'session','prompt','source_uuid','source_sha256','outcome','assessment'}
    if (not isinstance(value, dict) or set(value) not in (fields, fields|{'publication_request'})
            or any(not isinstance(value[k],str) or not UUID.fullmatch(value[k]) for k in ('session','prompt','source_uuid'))
            or not isinstance(value['source_sha256'],str) or not re.fullmatch('[0-9a-f]{64}',value['source_sha256'])
            or value['outcome'] not in ('no-learning','published') or not isinstance(value['assessment'],str)
            or not value['assessment'].strip() or len(value['assessment'].encode())>2048):
        raise ContractError('Use an explicit bounded source-bound assessment')
    if (value['outcome']=='published') != ('publication_request' in value):
        raise ContractError('Published completion requires the original publication request')
    root,bank=bank_writes.bank_path(repo);context=scope_binding.require(root)
    operations,_=bank_writes.state_paths(root,bank,str(uuid.uuid4()));directory=operations.parent/'claude-boundaries'
    trust_paths.state(directory)
    key=hashlib.sha256((value['session']+':'+value['prompt']).encode()).hexdigest()
    dfd=os.open(directory,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    directory_identity=os.fstat(dfd)
    try:
        fd=os.open(key+'.pending',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=dfd)
        try:
            info=os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:
                raise ContractError('Pending boundary is unsafe')
            with os.fdopen(fd,'rb',closefd=False) as stream: raw=stream.read(4097)
        finally:os.close(fd)
        pending=load(raw)
        if (not isinstance(pending,dict) or pending.get('schema')!='agent-memory/claude-boundary-pending-v1'
                or pending.get('project_id')!=context['project_id'] or pending.get('source_uuid')!=value['source_uuid']
                or pending.get('source_sha256')!=value['source_sha256']):
            raise ContractError('Pending boundary does not match the assessed source')
        witnessed=None
        if value['outcome']=='published':
            request=value['publication_request']
            memory_publication.request_value(root,request)
            proof=request['proof']
            if proof.get('blocked_paths') or proof.get('blocked_components'):
                raise ContractError('Conflicted publication remains pending')
            replay=sharing_recovery.replay(root,{'base':request['base'],'candidate':request['candidate']},
                    quarantine_records=proof.get('schema')=='agent-memory/semantic-sharing-proof-v2')
            if not replay.get('intended_delta_landed'):
                raise ContractError('Publication is not independently witnessed on authority')
            witnessed=replay['witnessed_commit']
        scope_binding.require(root,context['project_id'],expected=context)
        after=os.stat(key+'.pending',dir_fd=dfd,follow_symlinks=False)
        if (after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns)!=(info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns):
            raise ContractError('Pending boundary changed during acknowledgment')
        record={'schema':'agent-memory/claude-boundary-completed-v1','project_id':context['project_id'],
                'source_uuid':value['source_uuid'],'source_sha256':value['source_sha256'],'outcome':value['outcome'],
                'assessment':value['assessment'],'assessment_is_agent_assertion':True,'source_publication_link_verified':False,'witnessed_commit':witnessed,
                'pending_sha256':hashlib.sha256(raw).hexdigest()}
        output=json.dumps(record,sort_keys=True).encode()
        try:
            done=os.open(key+'.done',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=dfd)
        except FileExistsError:
            done=os.open(key+'.done',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=dfd)
            try:
                dstat=os.fstat(done)
                if (not stat.S_ISREG(dstat.st_mode) or dstat.st_uid!=os.geteuid() or dstat.st_nlink!=1
                        or dstat.st_mode&0o077 or dstat.st_size>4096):
                    raise ContractError('Completed boundary evidence is unsafe')
                with os.fdopen(done,'rb',closefd=False) as stream: existing=stream.read(4097)
                if existing != output:
                    raise ContractError('Completed boundary differs; preserve both records')
            finally:os.close(done)
        else:
            with os.fdopen(done,'wb') as stream:stream.write(output);stream.flush();os.fsync(stream.fileno())
            os.fsync(dfd)
        trust_paths.state(directory)
        current_directory=directory.lstat()
        current_pending=os.stat(key+'.pending',dir_fd=dfd,follow_symlinks=False)
        if ((current_directory.st_dev,current_directory.st_ino)!=(directory_identity.st_dev,directory_identity.st_ino)
                or (current_pending.st_dev,current_pending.st_ino,current_pending.st_size,current_pending.st_mtime_ns)
                != (info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns)):
            raise ContractError('Boundary state changed; preserve pending evidence')
        os.unlink(key+'.pending',dir_fd=dfd);os.fsync(dfd)
        return {'verified':True,'outcome':value['outcome'],'assessment_is_agent_assertion':True,
                'publication_verified':witnessed is not None,'source_publication_link_verified':False,'witnessed_commit':witnessed,'canonical_writes_performed':False}
    finally:os.close(dfd)


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--repo',required=True);parser.add_argument('--input',required=True);args=parser.parse_args(argv)
    try:
        value=load(sharing_limits.read_bytes(args.input,sharing_limits.MAX_PRIVATE));result=complete(args.repo,value)
        print(json.dumps(result));return 0
    except (OSError,ValueError,TypeError,KeyError,UnicodeError) as exc:
        print(json.dumps({'verified':False,'publication_verified':False,'canonical_writes_performed':False,
                          'reason':'Boundary completion remains pending; preserve evidence and inspect the failure.'}));return 1
