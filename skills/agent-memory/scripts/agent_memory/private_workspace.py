"""Create project-bound private artifacts without shell file operations."""
import argparse
import json
import os
from pathlib import Path
import re
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


def write_exclusive(directory, name, raw):
    trust_paths.state(directory)
    dfd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=dfd)
        try:
            with os.fdopen(fd, 'wb') as stream:
                stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        except BaseException:
            raise  # Preserve any interrupted file; never guess or overwrite it.
        os.fsync(dfd)
    finally:
        os.close(dfd)


def put(repo, workspace, name, raw):
    if not re.fullmatch(r'[0-9a-f]{32}', workspace) or not NAME.fullmatch(name) or name == 'binding.json':
        raise ContractError('Use the returned workspace ID and a simple new JSON filename')
    if len(raw) > MAX_INPUT:
        raise ContractError('Private input exceeds the 16 MiB bound')
    value = load(raw)  # Reject malformed or duplicate-key JSON; keep explicit bytes.
    if not isinstance(value, (dict, list)):
        raise ContractError('Private input must be a JSON object or array')
    root, context, base = workspace_root(repo)
    path = base / workspace
    trust_paths.state(path)
    binding = path / 'binding.json'
    trust_paths.record(binding)
    if load(binding.read_bytes()) != {'schema':'agent-memory/private-workspace-v1','repo':str(root),
                                      'project_id':context['project_id'],'workspace':workspace}:
        raise ContractError('Private workspace does not match this bound repository')
    write_exclusive(path, name, raw)
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
