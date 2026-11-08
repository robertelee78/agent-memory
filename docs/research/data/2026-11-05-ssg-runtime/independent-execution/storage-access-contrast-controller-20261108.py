#!/usr/bin/env python3
"""New synthetic storage-access contrast; never retries DDL or reruns the study.

Parent execution requires independent review of the exact source SHA first:
  qualification-venv/bin/python -I -B THIS --output NEW_DIRECTORY \
      --source-sha256 REVIEWED_SHA256 --reserved-host-slot

This controller never starts a model, contacts an embedding endpoint, changes
Memgraph flags/settings, adopts old resources, or infers historical causation.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import socket
import stat
import subprocess
import sys
import time
import uuid

IMAGE = 'docker.io/memgraph/memgraph-mage@sha256:e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27'
CHILD = 'sha256:fe2f37e45e263225129bcb6955612ac9956dea0e8db614f9c39eb74b105c6c39'
CONFIG_IMAGE = '44b5244a3622425584cae52b0cdf1ea1690ebb98e27110b3adfd1af7ab8227a9'
LABEL = 'org.agent-memory.storage255'
FLAGS = ['--telemetry-enabled=false', '--storage-properties-on-edges=true',
         '--storage-wal-enabled=true', '--storage-snapshot-interval-sec=300',
         '--storage-snapshot-on-exit=true', '--log-level=WARNING',
         '--also-log-to-stderr=true']
INDEX = 'am255_diag_storage'
DDL = ('CREATE VECTOR INDEX ' + INDEX + ' ON :AM255Eligible(embedding) '
       'WITH CONFIG {"dimension":4096,"capacity":16,"metric":"cos"}')
INDEX_INFO = 'CALL vector_search.show_index_info() YIELD * RETURN *'
INSERT = 'UNWIND $rows AS row CREATE (n:Source:AM255Eligible) SET n.key=row.key,n.embedding=row.vector'
COUNT = 'MATCH (n:AM255Eligible) RETURN count(n) AS count'
HOLDER_READ = 'MATCH (n:Source) RETURN count(n) AS count'
LIMIT = 30.0


class Deadline(Exception):
    pass


def interrupted(signum, frame):
    raise Deadline('signal/deadline ' + str(signum))


def sha_bytes(value):
    return hashlib.sha256(value).hexdigest()


def sha_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def now():
    return {'wall_ns': time.time_ns(), 'monotonic_ns': time.monotonic_ns()}


def error(exc):
    return {'type': type(exc).__name__, 'message': str(exc),
            'code': getattr(exc, 'code', None)}


def persist(path, value):
    """Replace only this run's receipt, publishing after file+directory fsync."""
    path = Path(path)
    payload = json.dumps(value, indent=2, sort_keys=True, allow_nan=False).encode() + b'\n'
    temp = path.parent / ('.' + path.name + '.' + uuid.uuid4().hex)
    fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if temp.exists():
            temp.unlink()


def private_directory(path):
    os.mkdir(path, 0o700)


def validate_directory(path):
    path = Path(path)
    data = path.lstat()
    if not stat.S_ISDIR(data.st_mode) or data.st_uid != os.getuid() or stat.S_IMODE(data.st_mode) != 0o700:
        raise ValueError('Unsafe private run directory: ' + str(path))
    return path.resolve(strict=True)


def read_private(path):
    data = Path(path).lstat()
    if not stat.S_ISREG(data.st_mode) or data.st_uid != os.getuid() or data.st_nlink != 1 or stat.S_IMODE(data.st_mode) != 0o600:
        raise ValueError('Unsafe private receipt: ' + str(path))
    return json.loads(Path(path).read_text())


def run_command(directory, argv, timeout=30, check=True, new_session=True):
    directory = Path(directory)
    receipt = directory / ('command-' + uuid.uuid4().hex + '.json')
    item = {'argv': [str(v) for v in argv], 'cwd': str(directory), 'started': now(),
            'timeout_seconds': timeout, 'transport_outcome': 'intent-recorded'}
    persist(receipt, item)
    process = None
    try:
        process = subprocess.Popen(item['argv'], cwd=directory, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=new_session)
        item['pid'] = process.pid
        persist(receipt, item)
        out, err = process.communicate(timeout=timeout)
        item.update(exit=process.returncode, stdout=out.decode(errors='replace'),
                    stderr=err.decode(errors='replace'), transport_outcome='settled')
    except BaseException as exc:
        # Once cancellation starts, the cell timer must not interrupt child reaping.
        signal.setitimer(signal.ITIMER_REAL, 0)
        item['error'] = error(exc)
        item['transport_outcome'] = 'unconfirmed'
        if process is not None:
            try:
                if new_session:
                    os.killpg(process.pid, signal.SIGTERM)
                else:
                    process.terminate()
            except ProcessLookupError:
                pass
            try:
                out, err = process.communicate(timeout=2)
            except subprocess.TimeoutExpired:
                try:
                    if new_session:
                        os.killpg(process.pid, signal.SIGKILL)
                    else:
                        process.kill()
                except ProcessLookupError:
                    pass
                out, err = process.communicate()
            item.update(exit=process.returncode, stdout=out.decode(errors='replace'),
                        stderr=err.decode(errors='replace'), reaped=True)
        item['finished'] = now()
        persist(receipt, item)
        raise
    item['finished'] = now()
    persist(receipt, item)
    if check and item['exit']:
        raise RuntimeError('Command failed; receipt=' + str(receipt))
    return item


def driver(uri):
    from neo4j import GraphDatabase
    return GraphDatabase.driver(uri, auth=('', ''), connection_timeout=3,
                                connection_acquisition_timeout=3, max_connection_pool_size=1,
                                max_transaction_retry_time=0, liveness_check_timeout=0)


def query(connection, directory, text, parameters=None, transaction=None, timeout=5):
    """Auto-commit only; no execute_read/write managed retry facility."""
    from neo4j import Query
    receipt = Path(directory) / ('query-' + uuid.uuid4().hex + '.json')
    item = {'cypher': text, 'parameters': parameters or {}, 'started': now(),
            'timeout_seconds': timeout, 'outcome': 'intent-recorded'}
    persist(receipt, item)
    session = None
    try:
        if transaction is None:
            session = connection.session(database='memgraph')
            rows = session.run(Query(text, timeout=timeout), **(parameters or {}))
        else:
            rows = transaction.run(text, **(parameters or {}))
        item['rows'] = [row.data() for row in rows]
        item['outcome'] = 'success'
    except BaseException as exc:
        item.update(outcome='error', error=error(exc))
        raise
    finally:
        item['finished'] = now()
        persist(receipt, item)
        if session is not None:
            session.close()
    return item


def callback(args, mode, directory, config, timeout):
    """All workers execute the retained reviewed bytes with the exact interpreter."""
    return run_command(directory, [sys.executable, '-I', '-B', str(Path(args.output) / 'controller-source.py'),
                       '--internal', mode, '--output', str(args.output), '--config', str(config),
                       '--source-sha256', args.source_sha256], timeout=timeout,
                       new_session=mode != 'ddl')


def internal(args):
    root = validate_directory(args.output)
    config_path = Path(args.config).resolve(strict=True)
    if config_path.parent.parent != root or config_path.name != 'cell-config.json':
        raise ValueError('Callback config outside owned arm directory')
    arm = validate_directory(config_path.parent)
    config = read_private(config_path)
    if config['source_sha256'] != args.source_sha256 or config['arm'] not in ('idle', 'held-transaction'):
        raise ValueError('Callback source/arm mismatch')
    if config['uri'] != 'bolt://127.0.0.1:' + str(config['port']) or not 1 <= config['port'] <= 65535:
        raise ValueError('Nonloopback callback refused')
    if importlib.metadata.version('neo4j') != '5.28.2':
        raise ValueError('Callback driver changed')
    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGALRM):
        signal.signal(sig, interrupted)
    if args.internal == 'ddl':
        return ddl_worker(arm, config)
    return cell_worker(args, arm, config)


def ddl_worker(arm, config):
    result = {'schema': 'am255/storage-ddl-v1', 'started': now(), 'settled': False,
              'source_sha256': config['source_sha256'], 'attempt_count': 1}
    connection = None
    try:
        connection = driver(config['uri'])
        result['ddl'] = query(connection, arm, DDL, timeout=25)
        result['outcome'] = 'success'
        result['settled'] = True
    except Deadline as exc:
        result.update(outcome='unconfirmed', error=error(exc))
    except Exception as exc:
        result.update(outcome='error', error=error(exc))
        # Only an actual server response confirms this failure, not client timeouts.
        result['settled'] = bool(getattr(exc, 'code', None))
    finally:
        result['ddl_finished'] = now()
        if connection is not None:
            try:
                connection.close()
                result['driver_closed'] = True
            except BaseException as exc:
                result['close_error'] = error(exc)
        result['finished'] = now()
        persist(arm / 'ddl-result.json', result)
    return 0 if result['settled'] else 1


def cell_worker(args, arm, config):
    result = {'schema': 'am255/storage-cell-v1', 'arm': config['arm'], 'started': now(),
              'source_sha256': args.source_sha256, 'cell_limit_seconds': LIMIT,
              'interpretation': 'Mechanism contrast only; not historical snapshot cause or G1 acceptance'}
    connection = holder = holder_session = transaction = None
    ddl_process_result = None
    try:
        connection = driver(config['uri'])
        # Startup readiness retries are read-only; the experimental CREATE is never retried.
        ready_deadline = time.monotonic() + 60
        failures = []
        while True:
            try:
                query(connection, arm, 'RETURN 1 AS ok', timeout=2)
                break
            except Exception as exc:
                failures.append(error(exc))
                if time.monotonic() >= ready_deadline:
                    raise RuntimeError('Graph not ready within60seconds') from exc
                time.sleep(.2)
        result['readiness_failures'] = failures
        result['configuration'] = query(connection, arm, 'SHOW CONFIG')['rows']
        settings = {row['name']: row['current_value'] for row in result['configuration']}
        if settings.get('storage_wal_enabled') != 'true' or settings.get('storage_snapshot_interval_sec') != '300':
            raise ValueError('Effective WAL/snapshot configuration differs')
        access = float(settings['storage_access_timeout_sec'])
        if not 0 < access < 20:
            raise ValueError('Access timeout not bounded for30secondcell; do not change setting')
        result['effective_access_timeout_seconds'] = access
        result['cell_started'] = now()
        signal.setitimer(signal.ITIMER_REAL, LIMIT)
        rows = [{'key': 'diag:' + format(i, '03d'),
                 'vector': [1.0 if j == i else 0.0 for j in range(4096)]} for i in range(11)]
        result['fixture_sha256'] = sha_bytes(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode())
        query(connection, arm, INSERT, {'rows': rows})
        if query(connection, arm, COUNT)['rows'] != [{'count': 11}]:
            raise ValueError('Synthetic eligible cardinality differs from11')
        if config['arm'] == 'held-transaction':
            holder = driver(config['uri'])
            holder_session = holder.session(database='memgraph')
            transaction = holder_session.begin_transaction(timeout=25)
            read = query(holder, arm, HOLDER_READ, transaction=transaction)
            if read['rows'] != [{'count': 11}]:
                raise ValueError('Holder read failed cardinality barrier')
            result['holder_ready'] = now()
            result['holder_read'] = read
            result['holder_transaction_identity'] = 'Explicit client transaction object; no server transaction ID claimed'
            persist(arm / 'holder-ready.json', {'ready': result['holder_ready'], 'read': read,
                                               'transaction_open': not transaction.closed()})
            if transaction.closed():
                raise ValueError('Holder transaction already closed')
        remaining = LIMIT - (time.monotonic_ns() - result['cell_started']['monotonic_ns']) / 1e9
        if remaining <= 0:
            raise Deadline('Cell budget exhausted before DDL')
        ddl_process_result = callback(args, 'ddl', arm, arm / 'cell-config.json', remaining)
        result['ddl_callback_exit'] = ddl_process_result['exit']
        result['ddl'] = read_private(arm / 'ddl-result.json')
        if transaction is not None:
            # A local closed() flag alone cannot prove server-side transaction survival.
            result['holder_post_ddl_read'] = query(holder, arm, HOLDER_READ, transaction=transaction)
            if result['holder_post_ddl_read']['rows'] != [{'count': 11}]:
                raise ValueError('Same explicit holder transaction did not survive DDL')
            result['holder_still_open_after_ddl'] = not transaction.closed()
            result['holder_release_started'] = now()
            transaction.rollback()
            result['holder_rollback_finished'] = now()
            transaction = None
        result['cell_finished'] = now()
        result['cell_elapsed_seconds'] = (result['cell_finished']['monotonic_ns'] - result['cell_started']['monotonic_ns']) / 1e9
        result['within_deadline'] = result['cell_elapsed_seconds'] <= LIMIT
    except BaseException as exc:
        result['error'] = error(exc)
        result['within_deadline'] = False
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        cleanup_errors = []
        if transaction is not None:
            try:
                result['holder_release_started'] = now()
                transaction.rollback()
                result['holder_rollback_finished'] = now()
            except BaseException as exc:
                cleanup_errors.append({'rollback': error(exc)})
        for obj, name in ((holder_session, 'holder_session'), (holder, 'holder_driver')):
            if obj is not None:
                try:
                    obj.close()
                except BaseException as exc:
                    cleanup_errors.append({name: error(exc)})
        if connection is not None:
            try:
                result['indexes_before_cleanup'] = query(connection, arm, INDEX_INFO)['rows']
                present = any(INDEX in row.values() for row in result['indexes_before_cleanup'])
                if present:
                    query(connection, arm, 'DROP VECTOR INDEX ' + INDEX)
                query(connection, arm, 'MATCH (n:AM255Eligible) REMOVE n:AM255Eligible')
                result['indexes_after_drop'] = query(connection, arm, INDEX_INFO)['rows']
                result['eligible_labels_after_removal'] = query(connection, arm, COUNT)['rows']
                if any(INDEX in row.values() for row in result['indexes_after_drop']) or result['eligible_labels_after_removal'] != [{'count': 0}]:
                    raise ValueError('Owned diagnostic index/label cleanup incomplete')
            except BaseException as exc:
                cleanup_errors.append({'index_inventory_cleanup': error(exc)})
            finally:
                try:
                    connection.close()
                except BaseException as exc:
                    cleanup_errors.append({'fixture_driver': error(exc)})
        result['client_cleanup_errors'] = cleanup_errors
        result['finished'] = now()
        persist(arm / 'cell-result.json', result)
    return 0 if not result.get('error') and not result['client_cleanup_errors'] and result.get('within_deadline') else 1


class Owned:
    def __init__(self, engine, directory, run_id):
        self.engine, self.directory, self.run_id = engine, directory, run_id
        self.resources = []
        self.uncertain = False

    def journal(self):
        persist(self.directory / 'ownership.json', {'run_id': self.run_id,
                'resources': self.resources, 'allocation_outcome_uncertain': self.uncertain})

    def command(self, *argv, **kwargs):
        return run_command(self.directory, [self.engine, *argv], **kwargs)

    def inspect(self, resource):
        argv = ['volume', 'inspect', resource['name']] if resource['type'] == 'volume' else ['inspect', resource.get('id', resource['name'])]
        receipt = self.command(*argv, check=False)
        if receipt['exit']:
            names = self.command(*(['volume', 'ls', '--format', '{{.Name}}'] if resource['type'] == 'volume'
                                   else ['ps', '-a', '--format', '{{.Names}}']))['stdout'].splitlines()
            if resource['name'] in names:
                raise ValueError('Failed inspect contradicted by resource listing')
            return None
        value = json.loads(receipt['stdout'])[0]
        labels = value.get('Labels') if resource['type'] == 'volume' else value.get('Config', {}).get('Labels')
        if not labels or labels.get(LABEL) != self.run_id:
            raise ValueError('Refusing foreign/recycled resource identity')
        identity = str(value.get('CreatedAt', value.get('Created')))
        if identity == 'None':
            raise ValueError('Resource lacks creation identity')
        if resource.get('created') and resource['created'] != identity:
            raise ValueError('Recycled resource creation identity')
        if resource['type'] == 'container':
            if resource.get('id') and value['Id'] != resource['id']:
                raise ValueError('Container ID changed')
            if value.get('Image', '').removeprefix('sha256:') != CONFIG_IMAGE:
                raise ValueError('Container config image differs')
            resource['id'] = value['Id']
        resource['created'] = identity
        self.journal()
        return value

    def allocate(self, kind, name, argv):
        # Read-only collision check precedes durable allocation intent.
        inspect_args = ['volume', 'inspect', name] if kind == 'volume' else ['inspect', name]
        existing = self.command(*inspect_args, check=False)
        listing = self.command(*(['volume', 'ls', '--format', '{{.Name}}'] if kind == 'volume'
                                 else ['ps', '-a', '--format', '{{.Names}}']))['stdout'].splitlines()
        if existing['exit'] == 0 or name in listing:
            raise ValueError('Fresh resource name already exists; no adoption')
        resource = {'type': kind, 'name': name, 'allocation_intent': now(), 'absent_before_allocation': True}
        self.resources.append(resource)
        self.journal()
        try:
            self.command(*argv)
            if self.inspect(resource) is None:
                raise ValueError('Allocated resource not found')
        except BaseException:
            self.uncertain = True
            self.journal()
            raise
        return resource

    def cleanup(self):
        rows = []
        retain_volumes = False
        for resource in sorted(self.resources, key=lambda r: r['type'] != 'container'):
            row = {'resource': dict(resource), 'absent': False}
            if resource['type'] == 'volume' and retain_volumes:
                row['retained'] = 'Container/log state not safely disposed; preserve all owned volumes'
                rows.append(row)
                continue
            try:
                value = self.inspect(resource)
                if value is None and resource['type'] == 'container' and resource.get('id'):
                    retain_volumes = True
                    row['vanished_before_cleanup'] = now()
                    row['retained'] = 'Previously verified container vanished; preserve unread evidence volumes'
                if value is not None:
                    if resource['type'] == 'container':
                        try:
                            self.command('logs', resource['id'])
                        except Exception as exc:
                            # Failed log retention must not leave verified owned work running.
                            # Do not erase the container or either unread evidence volume.
                            retain_volumes = True
                            row['log_read_error'] = error(exc)
                            row['retained'] = 'Unread logs; container and all volumes retained'
                            if self.inspect(resource) is None:
                                raise ValueError('Owned container disappeared before safe stop')
                            self.command('stop', '--time', '10', resource['id'])
                            stopped = self.inspect(resource)
                            if stopped is None or stopped.get('State', {}).get('Running') is not False:
                                raise ValueError('Exact-owned container stop not verified')
                            row['stopped_verified'] = True
                        else:
                            self.command('exec', resource['id'], 'find', '/var/lib/memgraph', '/var/log/memgraph', '-type', 'f', check=False)
                            self.command('rm', '-f', resource['id'])
                    else:
                        self.command('volume', 'rm', resource['name'])
                row['absent'] = self.inspect(resource) is None
            except BaseException as exc:
                row['error'] = error(exc)
                if resource['type'] == 'container':
                    retain_volumes = True
            row['resource'] = dict(resource)
            rows.append(row)
        result = {'resources': rows, 'allocation_outcome_uncertain': self.uncertain,
                  'unread_or_unknown_container_state_volumes_retained': retain_volumes,
                  'status': 'disposed' if all(r['absent'] for r in rows) and not self.uncertain else 'erasure-pending',
                  'forensic_erasure_claim': False, 'scientific_receipts_retained': True}
        persist(self.directory / 'cleanup.json', result)
        return result


def preflight(args, root, engine):
    receipt = {'schema': 'am255/storage-preflight-v1', 'started': now(),
               'interpreter': sys.executable, 'interpreter_realpath': str(Path(sys.executable).resolve()),
               'interpreter_sha256': sha_file(Path(sys.executable).resolve()), 'python': sys.version,
               'platform': platform.platform(), 'machine': platform.machine(),
               'engine': engine, 'engine_sha256': sha_file(engine), 'source_sha256': args.source_sha256,
               'reservation_attested': args.reserved_host_slot}
    try:
        if platform.system() != 'Darwin' or platform.machine() != 'arm64':
            raise ValueError('This diagnostic profile requires macOSarm64 with Linuxarm64 Podman VM')
        receipt['neo4j_version'] = importlib.metadata.version('neo4j')
        if receipt['neo4j_version'] != '5.28.2':
            raise ValueError('Required interpreter dependency: neo4j==5.28.2')
        distribution = importlib.metadata.distribution('neo4j')
        receipt['driver_files'] = [{'path': str(distribution.locate_file(p).resolve()),
                                   'sha256': sha_file(distribution.locate_file(p))}
                                  for p in distribution.files or []
                                  if p.suffix == '.py' and str(p).startswith('neo4j/')]
        if not receipt['driver_files']:
            raise ValueError('Driver source closure unavailable')
        image = json.loads(run_command(root, [engine, 'image', 'inspect', IMAGE])['stdout'])[0]
        receipt['local_image'] = image
        if image.get('Architecture') != 'arm64' or image.get('Os') != 'linux' or image.get('Id', '').removeprefix('sha256:') != CONFIG_IMAGE:
            raise ValueError('Local image OS/architecture/config pin differs')
        if image.get('Digest') != IMAGE.split('@')[1] or IMAGE.split('@')[0] + '@' + CHILD not in image.get('RepoDigests', []):
            raise ValueError('Local index/arm64 manifest bindings differ; no pull is permitted')
        receipt['before_containers'] = json.loads(run_command(root, [engine, 'ps', '-a', '--no-trunc', '--format', 'json'])['stdout'])
        receipt['before_volumes'] = json.loads(run_command(root, [engine, 'volume', 'ls', '--format', 'json'])['stdout'])
        receipt['machine'] = json.loads(run_command(root, [engine, 'machine', 'inspect'])['stdout'])
        memory = run_command(root, [engine, 'machine', 'ssh', 'cat', '/proc/meminfo'])['stdout']
        receipt['vm_meminfo'] = memory
        values = {k: int(v) * 1024 for k, v in re.findall(r'^(MemTotal|MemAvailable):\s+(\d+)\s+kB', memory, re.M)}
        if min(values.get('MemTotal', 0), values.get('MemAvailable', 0)) < 12 * 1024 ** 3:
            raise ValueError('Actual VM total/available memory must both be>=12GiB')
        receipt['satisfied'] = True
    except BaseException as exc:
        receipt.update(satisfied=False, error=error(exc))
        raise
    finally:
        receipt['finished'] = now()
        persist(root / 'preflight.json', receipt)
    return receipt


def one_arm(args, root, engine, run_id, arm_name):
    directory = root / arm_name
    private_directory(directory)
    owned = Owned(engine, directory, run_id)
    result = {'arm': arm_name, 'started': now()}
    try:
        # Recheck available VM memory immediately before each owned graph starts.
        mem = owned.command('machine', 'ssh', 'cat', '/proc/meminfo')['stdout']
        values = {k: int(v) * 1024 for k, v in re.findall(r'^(MemTotal|MemAvailable):\s+(\d+)\s+kB', mem, re.M)}
        if min(values.get('MemTotal', 0), values.get('MemAvailable', 0)) < 12 * 1024 ** 3:
            raise ValueError('VM memory prerequisite unavailable before arm allocation')
        name = 'am255-storage-' + run_id[:16] + '-' + arm_name
        for role in ('data', 'logs'):
            volume = name + '-' + role
            owned.allocate('volume', volume, ['volume', 'create', '--label', LABEL + '=' + run_id, volume])
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        container = owned.allocate('container', name,
                    ['run', '-d', '--pull=never', '--name', name, '--label', LABEL + '=' + run_id,
                     '--cpus=2', '--memory=12g', '-p', '127.0.0.1:' + str(port) + ':7687',
                     '-v', name + '-data:/var/lib/memgraph', '-v', name + '-logs:/var/log/memgraph', IMAGE, *FLAGS])
        value = owned.inspect(container)
        persist(directory / 'instance.json', value)
        if value.get('Args') != FLAGS:
            raise ValueError('Effective process argv differs from unchanged reference flags')
        uid = owned.command('exec', container['id'], 'id', '-u')['stdout'].strip()
        gid = owned.command('exec', container['id'], 'id', '-g')['stdout'].strip()
        memmax = owned.command('exec', container['id'], 'cat', '/sys/fs/cgroup/memory.max')['stdout'].strip()
        cpumax = owned.command('exec', container['id'], 'cat', '/sys/fs/cgroup/cpu.max')['stdout'].split()
        if (uid, gid) != ('101', '103') or memmax != str(12 * 1024 ** 3) or len(cpumax) != 2 or cpumax[0] == 'max' or int(cpumax[0]) != 2 * int(cpumax[1]):
            raise ValueError('Actual image user/cgroup ceilings differ')
        config = {'arm': arm_name, 'port': port, 'uri': 'bolt://127.0.0.1:' + str(port),
                  'run_id': run_id, 'source_sha256': args.source_sha256,
                  'container_id': container['id'], 'image': IMAGE, 'config_image': CONFIG_IMAGE}
        persist(directory / 'cell-config.json', config)
        callback(args, 'cell', directory, directory / 'cell-config.json', 110)
        result['cell'] = read_private(directory / 'cell-result.json')
    except BaseException as exc:
        result['error'] = error(exc)
        path = directory / 'cell-result.json'
        if path.exists():
            result['cell'] = read_private(path)
    finally:
        result['cleanup'] = owned.cleanup()
        result['finished'] = now()
        persist(directory / 'arm-result.json', result)
    return result


def classify(arms):
    if len(arms) != 2 or any(a.get('error') or a['cleanup']['status'] != 'disposed' for a in arms):
        return 'inconclusive-or-cleanup-failed'
    cells = [a.get('cell', {}) for a in arms]
    if any(c.get('error') or c.get('client_cleanup_errors') or not c.get('within_deadline') or not c.get('ddl', {}).get('settled') for c in cells):
        return 'inconclusive-or-cell-failed'
    idle, held = cells
    if idle.get('configuration') != held.get('configuration'):
        return 'inconclusive-effective-configurations-differ'
    ddl = held['ddl']
    overlap = (held.get('holder_still_open_after_ddl') is True and
               held['holder_ready']['monotonic_ns'] < ddl['started']['monotonic_ns'] <
               ddl['ddl_finished']['monotonic_ns'] <= held['holder_release_started']['monotonic_ns'])
    if not overlap:
        return 'inconclusive-unproven-holder-overlap'
    idle_ok = idle['ddl'].get('outcome') == 'success'
    held_ok = ddl.get('outcome') == 'success'
    same_error = ('Cannot get unique access to the storage' in ddl.get('error', {}).get('message', '') and
                  str(ddl.get('error', {}).get('code', '')).startswith('Memgraph.TransientError'))
    if idle_ok and same_error:
        return 'deliberate-transaction-contention-mechanism-observed-not-historical-cause'
    if idle_ok and held_ok:
        return 'both-succeeded-no-contention-reproduction'
    if not idle_ok and not held_ok:
        return 'both-failed-idle-ddl-prerequisite-investigation-required'
    return 'different-or-unexplained-outcomes'


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--reserved-host-slot', action='store_true')
    parser.add_argument('--engine', default='podman')
    parser.add_argument('--internal', choices=('cell', 'ddl'), help=argparse.SUPPRESS)
    parser.add_argument('--config', help=argparse.SUPPRESS)
    args = parser.parse_args()
    source = Path(__file__).resolve(strict=True)
    actual_sha = sha_file(source)
    if not re.fullmatch('[0-9a-f]{64}', args.source_sha256) or actual_sha != args.source_sha256:
        parser.error('Controller bytes differ from independently reviewed --source-sha256')
    if args.internal:
        return internal(args)
    if not args.reserved_host_slot:
        parser.error('Main must reserve the serial graph lane; --reserved-host-slot attests it')
    requested = Path(args.output)
    root = requested.parent.resolve(strict=True) / requested.name
    if requested.name in ('', '.', '..') or root.exists() or root.is_symlink():
        parser.error('--output must name a new directory, never an existing run')
    private_directory(root)
    args.output = str(root)
    frozen = root / 'controller-source.py'
    fd = os.open(frozen, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(source.read_bytes())
        stream.flush()
        os.fsync(stream.fileno())
    if sha_file(frozen) != args.source_sha256:
        raise ValueError('Source changed during retained callback copy; no resources allocated')
    run_id = uuid.uuid4().hex
    result = {'schema': 'am255/storage-diagnostic-v1', 'run_id': run_id, 'started': now(),
              'source_path': str(source), 'source_sha256': actual_sha,
              'callback_source': str(frozen), 'callback_source_sha256': sha_file(frozen),
              'argv': sys.argv, 'arms': [], 'G1_acceptance': False,
              'historical_snapshot_cause_established': False, 'model_endpoint_calls': 0,
              'original_experiment_modified': False, 'profile_flags_changed': False}
    persist(root / 'run.json', result)
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, interrupted)
    try:
        engine = shutil.which(args.engine)
        if not engine:
            raise ValueError('Locally installed podman executable unavailable')
        engine = str(Path(engine).resolve(strict=True))
        prerequisites = preflight(args, root, engine)
        for arm in ('idle', 'held-transaction'):
            observed = one_arm(args, root, engine, run_id, arm)
            result['arms'].append(observed)
            persist(root / 'run.json', result)
            if observed.get('error') or observed['cleanup']['status'] != 'disposed':
                break
        result['diagnostic_result'] = classify(result['arms'])
        after_containers = json.loads(run_command(root, [engine, 'ps', '-a', '--no-trunc', '--format', 'json'])['stdout'])
        after_volumes = json.loads(run_command(root, [engine, 'volume', 'ls', '--format', 'json'])['stdout'])
        before_ids = {row['Id'] for row in prerequisites['before_containers']}
        after_ids = {row['Id'] for row in after_containers}
        before_names = {row['Name'] for row in prerequisites['before_volumes']}
        after_names = {row['Name'] for row in after_volumes}
        result['external_preservation'] = {
            'before_container_ids': sorted(before_ids), 'after_container_ids': sorted(after_ids),
            'before_volume_names': sorted(before_names), 'after_volume_names': sorted(after_names),
            'unchanged': before_ids == after_ids and before_names == after_names,
            'unknown_concurrent_owner_attribution': None}
        if not result['external_preservation']['unchanged']:
            result['diagnostic_result'] = 'inconclusive-external-inventory-changed-no-owner-inferred'
        result['cleanup_status'] = ('disposed' if len(result['arms']) == 2 and all(a['cleanup']['status'] == 'disposed' for a in result['arms'])
                                    else 'erasure-pending' if any(a['cleanup']['status'] != 'disposed' for a in result['arms']) else 'no-unresolved-owned-resources')
    except BaseException as exc:
        result['error'] = error(exc)
        result['diagnostic_result'] = 'prerequisite-or-controller-failed-not-pass'
        allocated_intent = any((root / name / 'ownership.json').exists()
                               for name in ('idle', 'held-transaction'))
        result['cleanup_status'] = 'unknown-see-ownership-receipts' if allocated_intent else 'no-owned-resources-allocated'
    finally:
        result['finished'] = now()
        persist(root / 'run.json', result)
    print(json.dumps({'output': str(root), 'diagnostic_result': result['diagnostic_result'],
                      'cleanup_status': result['cleanup_status'], 'G1_acceptance': False}))
    return 0 if result['diagnostic_result'] in ('deliberate-transaction-contention-mechanism-observed-not-historical-cause',
                                                'both-succeeded-no-contention-reproduction') else 1


if __name__ == '__main__':
    raise SystemExit(main())
