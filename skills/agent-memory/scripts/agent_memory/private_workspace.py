"""Create project-bound private artifacts without shell file operations."""
import argparse
import json
import os
from pathlib import Path
import re
import stat
import sys
import uuid

from . import bank_writes, scope_binding, trust_paths
from .developer_identity import encoded, load
from .frontmatter import ContractError

MAX_INPUT = 16 * 1024 * 1024
NAME = re.compile(r'[A-Za-z0-9][A-Za-z0-9._-]{0,127}\.json\Z')


def workspace_root(repo):
    context = scope_binding.require(repo)
    root, bank = bank_writes.bank_path(repo)
    operations, _ = bank_writes.state_paths(root, bank, str(uuid.uuid4()))
    directory = operations.parent / 'private-workspaces'
    trust_paths.inspect(directory)
    return root, context, directory


def create(repo):
    root, context, base = workspace_root(repo)
    base.mkdir(parents=True, exist_ok=True, mode=0o700)
    trust_paths.state(base)
    name = uuid.uuid4().hex
    path = base / name
    path.mkdir(mode=0o700)
    trust_paths.state(path)
    data = encoded({'schema':'agent-memory/private-workspace-v1','repo':str(root),
                    'project_id':context['project_id'], 'workspace':name})
    write_exclusive(path, 'binding.json', data)
    return {'operation':'private-create','workspace':name,'path':str(path),
            'canonical_writes_performed':False,'verified':True}


def checked_directory(path):
    trust_paths.state(path)
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    info = os.fstat(fd)
    if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        os.close(fd)
        raise ContractError('Private workspace must remain owner-only')
    check_directory(path, info)
    return fd, info


def check_directory(path, expected):
    trust_paths.state(path)
    observed = path.lstat()
    if (not stat.S_ISDIR(observed.st_mode) or observed.st_dev != expected.st_dev
            or observed.st_ino != expected.st_ino or observed.st_uid != os.geteuid()
            or stat.S_IMODE(observed.st_mode) != 0o700):
        raise ContractError('Private workspace changed; preserve it for inspection')


def write_fd(dfd, name, raw):
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=dfd)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw); stream.flush(); os.fsync(stream.fileno())
    os.fsync(dfd)


def write_exclusive(directory, name, raw):
    fd, info = checked_directory(directory)
    try:
        write_fd(fd, name, raw)
        check_directory(directory, info)
    finally:
        os.close(fd)


def read_binding(fd):
    record = os.open('binding.json', os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
    try:
        info = os.fstat(record)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
                or info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != 0o600 or info.st_size > 4096):
            raise ContractError('Private workspace binding is unsafe')
        with os.fdopen(record, 'rb', closefd=False) as stream:
            raw = stream.read(4097)
        if len(raw) > 4096:
            raise ContractError('Private workspace binding exceeds its bound')
        return raw
    finally:
        os.close(record)


def put(repo, workspace, name, raw):
    if not re.fullmatch(r'[0-9a-f]{32}', workspace) or not NAME.fullmatch(name) or name == 'binding.json':
        raise ContractError('Use the returned workspace ID and a simple new JSON filename')
    if len(raw) > MAX_INPUT:
        raise ContractError('Private input exceeds the 16 MiB bound')
    value = load(raw)
    if not isinstance(value, (dict, list)):
        raise ContractError('Private input must be a JSON object or array')
    root, context, base = workspace_root(repo)
    path = base / workspace
    fd, info = checked_directory(path)
    try:
        binding = read_binding(fd)
        if load(binding) != {'schema':'agent-memory/private-workspace-v1','repo':str(root),
                             'project_id':context['project_id'],'workspace':workspace}:
            raise ContractError('Private workspace does not match this bound repository')
        check_directory(path, info)
        scope_binding.require(root, context['project_id'], expected=context)
        write_fd(fd, name, raw)
        check_directory(path, info)
        if read_binding(fd) != binding:
            raise ContractError('Private workspace binding changed; preserve written evidence')
        scope_binding.require(root, context['project_id'], expected=context)
    finally:
        os.close(fd)
    return {'operation':'private-put','path':str(path/name),'bytes':len(raw),
            'canonical_writes_performed':False,'verified':True}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    c = commands.add_parser('create');c.add_argument('--repo', required=True)
    w = commands.add_parser('put');w.add_argument('--repo', required=True);w.add_argument('--workspace', required=True);w.add_argument('--name', required=True)
    args = parser.parse_args(argv)
    try:
        result = create(args.repo) if args.command == 'create' else put(args.repo, args.workspace, args.name, sys.stdin.buffer.read(MAX_INPUT + 1))
        print(json.dumps(result));return 0
    except (ContractError, OSError, ValueError, TypeError, UnicodeError) as exc:
        print(json.dumps({'operation':'private-'+args.command,'verified':False,'canonical_writes_performed':False,
                          'error':str(exc),'repair':'Preserve existing private artifacts and inspect the reported workspace before retrying.'}))
        return 1
