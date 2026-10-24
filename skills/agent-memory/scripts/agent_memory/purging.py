"""Explicit, inventory-bound local bank deletion with resumable quarantine."""
from __future__ import annotations

from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import json
import os
from pathlib import Path
import re
import socket
import stat
import subprocess
import sys
import uuid

from . import installation as install, local_registry, repository_config, runtime_faults
from .bank_contract import parse_router, runtime_path
from .bank_writes import fault
from .bank_transactions import MARKER
from .frontmatter import ContractError, unique_object

SCHEMA = 'agent-memory/purge-receipt-v1'
MAX_RECORD = 32 * 2**20


def canonical(value):
    path = Path(value).expanduser()
    install.require(path.is_absolute() and '..' not in path.parts, 'Purge paths must be explicit absolute paths without traversal')
    install.require(not any(p.is_symlink() for p in (path, *path.parents)), 'Purge paths must not cross symlinks')
    return path.resolve()


def selection(args):
    repo, supplied = canonical(args.repo), canonical(args.path)
    install.require(repo.is_dir() and repo not in (Path('/'), Path.home().resolve()), 'A bounded project repository root is required')
    bank = repository_config.bank_path(repo)
    install.require(supplied == bank and bank.is_relative_to(repo) and bank != repo,
                    'Purge path must be exactly this repository\'s resolved Memory Bank, not a broad directory')
    state, code = install.locations()
    for retained in (state, code, Path.home().resolve()):
        install.require(bank != retained and not retained.is_relative_to(bank), 'Purge path would include retained runtime or home data')
    install.preflight_layout(args, state, code, install.read_receipt(state))
    return repo, bank, state, code


def identity(path):
    value = path.lstat()
    install.require(not stat.S_ISLNK(value.st_mode), 'Purge inventory contains a symlink: '+str(path))
    result = {'device':value.st_dev, 'inode':value.st_ino, 'mode':stat.S_IMODE(value.st_mode)}
    if stat.S_ISDIR(value.st_mode):
        return {**result, 'kind':'directory'}
    install.require(stat.S_ISREG(value.st_mode) and value.st_size <= 64*2**20,
                    'Purge inventory requires bounded ordinary files and directories')
    return {**result, 'kind':'file', 'bytes':value.st_size, 'change_time_ns':value.st_ctime_ns,
            'sha256':install.sha(path.read_bytes())}


def inventory(bank, *, owner=None):
    root = identity(bank)
    install.require(root['kind']=='directory', 'An existing ordinary Memory Bank directory is required')
    lock = bank/'.agent-memory.lock'
    if os.path.lexists(lock):
        install.require(owner is not None and not lock.is_symlink() and lock.read_bytes()==owner,
                        'Memory Bank is locked; preserve it and finish or recover its writer first')
    install.require(not os.path.lexists(bank/MARKER),
                    'Pending bank transaction requires its own recovery before purge')
    entries = {}; total = 0
    for parent, names, files in os.walk(bank, followlinks=False, onerror=lambda exc: (_ for _ in ()).throw(exc)):
        for name in sorted(names + files):
            node = Path(parent)/name; relative = node.relative_to(bank).as_posix()
            if relative == '.agent-memory.lock' and owner is not None:
                continue
            install.require('.git' not in Path(relative).parts, 'Purge must not delete nested Git metadata')
            item = identity(node); entries[relative] = item
            total += item.get('bytes', 0)
            install.require(len(entries)<=40000 and total<=512*2**20, 'Purge inventory exceeds its explicit bound')
    return root, entries


def preview(repo, bank, *, owner=None):
    root, entries = inventory(bank, owner=owner)
    router = bank/'MEMORY.md'
    install.require(router.is_file() and not router.is_symlink() and router.stat().st_size<=16384,
                    'Purge requires the selected bank\'s bounded ordinary router')
    metadata = parse_router(router.read_text())[0]
    result = {'schema':SCHEMA, 'repository':str(repo), 'path':str(bank),
              'project_id':metadata['project-id'], 'root_identity':root, 'entries':entries,
              'counts':{'files':sum(i['kind']=='file' for i in entries.values()),
                        'directories':1+sum(i['kind']=='directory' for i in entries.values()),
                        'bytes':sum(i.get('bytes',0) for i in entries.values()),
                        'durable_files':sum(i['kind']=='file' and not runtime_path(n) for n,i in entries.items()),
                        'generated_runtime_files':sum(i['kind']=='file' and runtime_path(n) for n,i in entries.items())}}
    install.require(len(install.encoded(result))<=MAX_RECORD//2, 'Purge inventory exceeds its journal bound')
    return {**result, 'confirmation':install.sha(install.encoded(result))}


def read_record(path):
    install.require(path.is_file() and not path.is_symlink() and path.stat().st_size<=MAX_RECORD,
                    'Purge record is missing, linked or oversized')
    return json.loads(path.read_bytes(), object_pairs_hook=unique_object)


def save(path, record):
    install.require(len(install.encoded(record))<=MAX_RECORD, 'Purge journal exceeds its bound')
    install.replace_record(path, record)


@contextmanager
def bank_lock(bank, expected_hash=None):
    install.require(bank.is_dir() and not bank.is_symlink(), 'An existing ordinary purge bank/quarantine is required')
    lock = bank/'.agent-memory.lock'
    if not os.path.lexists(lock):
        install.require(expected_hash is None, 'Purge quarantine lock is missing')
        context = local_registry.exclusive(lock, str(uuid.uuid4()), 'bank-purge')
        owner = runtime_faults.mutate('purge-bank-lock', lambda: context.__enter__())
        try:
            yield owner
        finally:
            if os.path.lexists(lock):
                runtime_faults.mutate('purge-bank-unlock', lambda: context.__exit__(None,None,None))
            else:
                context.__exit__(None,None,None)
        return
    install.require(not lock.is_symlink() and lock.is_file() and lock.stat().st_size<=65536,
                    'Purge lock must be an ordinary bounded file')
    owner = lock.read_bytes(); metadata = json.loads(owner, object_pairs_hook=unique_object)
    install.require(metadata.get('operation')=='bank-purge' and metadata.get('host')==socket.gethostname(),
                    'Existing lock belongs to another operation or host; preserve it')
    if expected_hash is not None:
        install.require(install.sha(owner)==expected_hash, 'Purge lock differs from its journal')
    try:
        os.kill(metadata['pid'], 0)
    except ProcessLookupError:
        pass
    else:
        raise ContractError('Recorded purge process is still live; do not steal its lock')
    fd = os.open(lock, os.O_RDWR | getattr(os,'O_NOFOLLOW',0))
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ContractError('Another process still holds the purge lock') from None
        install.require(os.fstat(fd).st_ino==lock.lstat().st_ino and os.fstat(fd).st_nlink==1
                        and lock.read_bytes()==owner, 'Purge lock changed during recovery')
        yield owner
    finally:
        os.close(fd)


def finish(record, record_path, bank, stage, owner):
    root, remaining = inventory(stage, owner=owner)
    approved = record['preview']
    install.require(root==approved['root_identity'], 'Purge quarantine root identity changed')
    install.require(all(name in approved['entries'] and item==approved['entries'][name]
                        for name,item in remaining.items()), 'Purge quarantine contains new or changed data; preserve it')
    record['status']='detached'; save(record_path,record)
    for index,(name,item) in enumerate(sorted(remaining.items(), key=lambda pair:(pair[1]['kind']=='directory',-len(Path(pair[0]).parts),pair[0]))):
        path = stage/name
        install.unchanged_parent(path)
        install.require(identity(path)==item, 'Purge entry changed immediately before removal: '+name)
        def remove():
            path.rmdir() if item['kind']=='directory' else path.unlink()
            install.sync_dir(path.parent)
        runtime_faults.mutate('purge-entry-remove',remove)
        fault('purge-after-entry-'+str(index))
    lock = stage/'.agent-memory.lock'
    if os.path.lexists(lock):
        install.require(owner is not None and not lock.is_symlink() and lock.read_bytes()==owner,
                        'Purge lock changed before final removal')
        runtime_faults.mutate('purge-lock-remove',lambda: (lock.unlink(),install.sync_dir(stage)))
    runtime_faults.mutate('purge-root-remove',lambda: (stage.rmdir(),install.sync_dir(stage.parent)))
    fault('purge-after-root-remove')
    record['status']='complete'; record['completed_at']=datetime.now(timezone.utc).isoformat()
    record['deletion_readback_verified']=not os.path.lexists(stage)
    save(record_path,record)
    fault('purge-after-receipt')
    return outcome(record,bank,changed=True)


def outcome(record, bank, *, changed):
    return {'operation':'purge-data','verified':True,'changed':changed,'receipt':record,
            'replacement_bank_present':os.path.lexists(bank),
            'scope':'Only the confirmed local bank; Git history, remote copies, runtime and external backups are retained'}


def run(args):
    repo, bank, state, code = selection(args)
    if args.check:
        return {'operation':'purge-data','verified':False,'check_complete':True,
                'writes_performed':False,'preview':preview(repo,bank),
                'retained':{'installation_state':str(state),'runtime':str(code),'git_history_and_external_backups':True}}
    install.require(re.fullmatch(r'[0-9a-f]{64}',args.confirm or '') is not None,
                    'Explicit confirmation must equal the exact preview digest')
    records = state/'purges'; record_path = records/(args.confirm+'.json')
    install.unchanged_parent(record_path)
    with runtime_faults.operation('purge',[state,bank]), install.locked(state):
        record = read_record(record_path) if os.path.lexists(record_path) else None
        if record is not None:
            install.require(record.get('schema')==SCHEMA and record.get('confirmation')==args.confirm
                            and record.get('status') in ('staging','detached','complete')
                            and record['preview']['repository']==str(repo) and record['preview']['path']==str(bank),
                            'Purge journal does not bind this exact confirmed repository/path')
            proof = dict(record['preview']); confirmation = proof.pop('confirmation')
            install.require(confirmation==args.confirm and install.sha(install.encoded(proof))==args.confirm,
                            'Purge journal inventory differs from the confirmed preview')
            stage = Path(record['stage'])
            install.require(stage.parent==bank.parent and re.fullmatch(r'\.agent-memory-purge-[0-9a-f]{32}',stage.name),
                            'Purge journal quarantine path is invalid')
            if record['status']=='complete':
                install.require(record.get('deletion_readback_verified') is True and not os.path.lexists(stage)
                                and (not os.path.lexists(bank) or identity(bank)!=record['preview']['root_identity']),
                                'Completed purge readback differs from its receipt; preserve and reconcile the targets')
                return outcome(record,bank,changed=False)
            if os.path.lexists(stage):
                install.require(identity(stage)==record['preview']['root_identity'], 'Purge quarantine root identity changed')
                lock = stage/'.agent-memory.lock'
                if not os.path.lexists(lock):
                    install.require(not any(stage.iterdir()) and identity(stage)==record['preview']['root_identity'],
                                    'Purge quarantine lost its lock while containing data')
                    return finish(record,record_path,bank,stage,None)
                with bank_lock(stage,record['lock_hash']) as owner:
                    return finish(record,record_path,bank,stage,owner)
            if record['status']=='detached':
                install.require(not os.path.lexists(bank) or identity(bank)!=record['preview']['root_identity'],
                                'Confirmed bank reappeared at its original path; preserve and reconcile it')
                record['status']='complete'; record['deletion_readback_verified']=True
                record['completed_at']=datetime.now(timezone.utc).isoformat()
                save(record_path,record)
                return outcome(record,bank,changed=False)
        with bank_lock(bank) as owner:
            fault('purge-bank-lock-ready')
            selected = preview(repo,bank,owner=owner)
            install.require(selected['confirmation']==args.confirm, 'Purge preview is stale; inspect a fresh exact preview')
            install.require(repository_config.bank_path(repo)==bank, 'Bank selection changed during purge preflight')
            if record is None:
                install.require(not records.is_symlink() and (not records.exists() or records.is_dir()),
                                'Purge journal directory is linked or unrelated')
                if not records.exists():
                    runtime_faults.mutate('purge-journal-directory',lambda:(records.mkdir(mode=0o700),install.sync_dir(state)))
                stage = bank.parent/('.agent-memory-purge-'+uuid.uuid4().hex)
                record = {'schema':SCHEMA,'status':'staging','confirmation':args.confirm,'preview':selected,
                          'stage':str(stage),'lock_hash':install.sha(owner)}
                save(record_path,record)
            else:
                install.require(selected==record['preview'], 'Current bank differs from pending purge consent')
                record['lock_hash']=install.sha(owner); save(record_path,record)
            install.native_rename(bank,stage)
            fault('purge-after-detach')
            return finish(record,record_path,bank,stage,owner)


def execute(args):
    try:
        install.require(sys.version_info >= (3,11), 'Python 3.11 or later is required')
        result = run(args)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, subprocess.SubprocessError) as exc:
        result = {'operation':'purge-data','verified':False,'error':str(exc),
                  'repair':'Preserve the bank, quarantine and external purge journal. Inspect the exact reported path; retry the same confirmed operation only after its writer has ended, or obtain a fresh preview for changed data.'}
    print(json.dumps(result,indent=2))
    return 0 if result.get('verified') or result.get('check_complete') else 1
