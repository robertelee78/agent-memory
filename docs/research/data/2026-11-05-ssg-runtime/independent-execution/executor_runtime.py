"""Owned serial reference runtime, exact artifact freeze and float32 scientific store."""
from __future__ import annotations
import importlib.metadata
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import re
import socket
import subprocess
import sys
import time
from executor_native import compact, digest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'runtime-controls'))
from common import command, request, save, sha, validate, choose_model
LABEL = 'org.agent-memory.independent255'


def pointer(root, path):
    return {'path': path.relative_to(root).as_posix(), 'sha256': sha(path)}


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def dependencies(binary):
    roots = [binary.parent, Path('/opt/homebrew/opt/openssl@4/lib'), Path('/opt/homebrew/opt/libomp/lib')]
    files = {binary}
    for root in roots:
        files.update(p.resolve() for p in root.rglob('*') if p.is_file()
                     and p.suffix in ('.dylib', '.so', '.metal', '.metallib'))
    pending = list(files)
    while pending:
        path = pending.pop()
        if path.suffix in ('.metal', '.metallib'):
            continue
        for line in command(['otool', '-L', path])['stdout'].splitlines()[1:]:
            name = line.strip().split(' (', 1)[0]
            if name.startswith('/') and Path(name).is_file():
                resolved = Path(name).resolve(strict=True)
                if resolved not in files:
                    files.add(resolved)
                    pending.append(resolved)
            elif name.startswith('@'):
                # The build stores its loader-relative dylibs beside the executable.
                candidate = binary.parent / Path(name).name
                if not candidate.is_file():
                    raise ValueError('Unresolved local dependency: ' + name)
                resolved = candidate.resolve(strict=True)
                if resolved not in files:
                    files.add(resolved)
                    pending.append(resolved)
    return [{'path': str(p), 'sha256': sha(p), 'bytes': p.stat().st_size} for p in sorted(files)]


def identity_command(argv):
    return {k: v for k, v in command(argv).items() if k != 'seconds'}


def artifacts(config, spec):
    if platform.system() != 'Darwin' or platform.machine() != 'arm64' or config.get('backend') != 'metal':
        raise ValueError('This frozen reference profile requires actual macOS arm64 Metal')
    binary = Path(config['reference_binary']).resolve(strict=True)
    weight = Path(config['reference_weight']).resolve(strict=True)
    source = Path(config['reference_source']).resolve(strict=True)
    revision = command(['git', '-C', source, 'rev-parse', 'HEAD'])['stdout'].strip()
    if revision != spec['profile']['reference_server_source']:
        raise ValueError('Reference source revision changed')
    if command(['git', '-C', source, 'diff', '--quiet', 'HEAD', '--'], check=False)['exit']:
        raise ValueError('Reference tracked source modified')
    if sha(weight) != spec['profile']['reference_weight_sha256'] or weight.stat().st_size != 8047105824:
        raise ValueError('Reference exact 8B Q8_0 weight differs')
    version = identity_command([binary, '--version'])
    if 'version: 9341' not in version['stdout'] + version['stderr'] or revision[:9] not in version['stdout'] + version['stderr']:
        raise ValueError('Reference build/source output differs')
    if sha(binary) != 'b57ea6bb97b43a4f74b5a5ab71f1cc2ef292899d83c0e45ddfa9290b169598b5':
        raise ValueError('Reference qualified executable bytes differ')
    expected = {'numpy': '2.2.6', 'neo4j': '5.28.2'}
    versions = {name: importlib.metadata.version(name) for name in expected}
    if versions != expected:
        raise ValueError('Actual numeric/driver dependency pins differ: ' + compact(versions))
    engine = config['engine']
    inspect = command([engine, 'image', 'inspect', spec['graph_profile']['image']])
    image = json.loads(inspect['stdout'])[0]
    if image.get('Architecture') != 'arm64' or image.get('Id', '').removeprefix('sha256:') != spec['graph_profile']['config_image']:
        raise ValueError('Actual local MAGE image/config/architecture differs')
    image_digest = image.get('Digest')
    if image_digest != spec['graph_profile']['arm64_manifest']:
        raise ValueError('Actual local MAGE arm64 manifest differs')
    return {'schema': 'am255/independent-runtime-profile-v1', 'reference_source_commit': revision,
            'binary': str(binary), 'binary_sha256': sha(binary), 'version': version,
            'weight': {'path': str(weight), 'sha256': spec['profile']['reference_weight_sha256'], 'bytes': weight.stat().st_size},
            'local_library_inventory': dependencies(binary), 'dependencies': versions,
            'graph_image': image, 'graph_profile': spec['graph_profile'], 'profile': spec['profile'],
            'os': identity_command(['sw_vers']), 'kernel': identity_command(['uname', '-a']),
            'identity_limit': 'Exact local artifacts and observed loads; no reproduced build or OS shared-cache file attestation'}


def memory_precondition(config):
    engine = config['engine']
    inventory = command([engine, 'ps', '-a', '--format', 'json'], check=False)
    machine = command([engine, 'machine', 'inspect'], check=False)
    actual = command([engine, 'machine', 'ssh', 'cat', '/proc/meminfo'], check=False)
    values = {k: int(v) * 1024 for k, v in re.findall(r'^(MemTotal|MemAvailable):\s+(\d+)\s+kB', actual['stdout'], re.M)}
    required = 12 * 1024 ** 3
    available = actual['exit'] == 0 and values.get('MemTotal', 0) >= required and values.get('MemAvailable', 0) >= required
    return {'schema': 'am255/graph-vm-memory-precondition-v1', 'inventory': inventory, 'machine': machine,
            'actual_vm_meminfo': actual, 'bytes': values, 'required_available_bytes': required,
            'satisfied': available, 'prerequisite': None if available else 'Actual VM total AND available memory >=12GiB before new graph start; container cap alone is insufficient',
            'container_memory_ceiling_bytes': required, 'container_cpu_ceiling': 2}


def no_active_services(engine):
    model = command(['pgrep', '-x', 'llama-server'], check=False)
    if model['exit'] not in (0, 1) or model['stdout'].strip():
        raise ValueError('Another model process exists; reserved nonoverlapping host slot required')
    current = command([engine, 'ps', '--filter', 'label=' + LABEL, '--format', '{{.ID}}'])
    if current['stdout'].strip():
        raise ValueError('Another independent owned generation remains active; inspect/clean its own receipt first')


def serving_parser():
    path = HERE.parent / 'serving-comparison/compare_serving.py'
    spec = importlib.util.spec_from_file_location('am255_retained_serving_parser', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Model:
    def __init__(self, root, profile, run_id):
        self.root, self.profile, self.run_id = root, profile, run_id
        self.process = None
        self.traces, self.requests = [], []
        self.parser = serving_parser()
        self.alias = 'qwen3-embedding-8b-q8_0-d20ddc71e8a5'
        self.base = 'http://127.0.0.1:' + str(free_port())

    def start(self):
        p = self.profile
        argv = [p['binary'], '--model', p['weight']['path'], '--host', '127.0.0.1', '--port', self.base.rsplit(':', 1)[1],
                '--alias', self.alias, '--embedding', '--pooling', 'last', '--ctx-size', '8192', '--parallel', '1',
                '--threads', '4', '--threads-batch', '4', '--threads-http', '4', '--batch-size', '8192', '--ubatch-size', '8192',
                '--device', 'Metal', '--gpu-layers', '99', '--flash-attn', 'off', '--fit', 'off', '--offline', '--no-webui',
                '--cache-ram', '0', '--log-verbosity', '4']
        env = {k: v for k, v in os.environ.items() if not k.startswith(('LLAMA_', 'GGML_', 'TURBO_', 'DYLD_'))
               and k not in ('OPENAI_API_KEY', 'HF_TOKEN')}
        env.update(OMP_NUM_THREADS='4', DYLD_PRINT_LIBRARIES='1')
        save(self.root / 'launch.json', {'argv': argv, 'run_id': self.run_id, 'binary_sha256': p['binary_sha256'],
                                      'cold_definition': 'New process and model load; OS cache not purged; shared host'})
        with (self.root / 'server-stdout.log').open('xb') as out, (self.root / 'server-stderr.log').open('xb') as err:
            self.process = subprocess.Popen(argv, env=env, stdout=out, stderr=err)
        save(self.root / 'owned-process.json', {'pid': self.process.pid, 'run_id': self.run_id, 'argv': argv,
             'ps': command(['ps', '-p', str(self.process.pid), '-o', 'lstart=', '-o', 'command='], check=False)})
        deadline = time.monotonic() + 240
        while True:
            if self.process.poll() is not None:
                raise RuntimeError('Owned model exited; retained real logs are authoritative')
            try:
                if request(self.base + '/health', '', timeout=2)['status'] == 200:
                    break
            except (OSError, ValueError):
                pass
            if time.monotonic() >= deadline:
                raise TimeoutError('Owned model readiness exceeded240s')
            time.sleep(.2)
        metadata = request(self.base + '/v1', '/models')
        save(self.root / 'models.json', metadata)
        choose_model(metadata, self.alias)
        bound = {'servers': {'fork': {'local_library_inventory': p['local_library_inventory']}}}
        self.parser.loaded_closure(self.root, 'fork', bound)

    def embed(self, texts, kind, source_keys, case_ordinal=None):
        if self.process is None or self.process.poll() is not None:
            raise RuntimeError('Only this exact live owned model may receive synthetic inputs')
        if not 1 <= len(texts) <= 8:
            raise ValueError('Frozen batch exceeds8 inputs')
        ordinal = len(self.requests) + 1
        counts = []
        for i, text in enumerate(texts):
            answer = request(self.base + '/tokenize', '', {'content': text, 'add_special': True})
            save(self.root / ('tokenize-' + str(ordinal) + '-' + str(i) + '.json'), answer)
            tokens = answer['body'].get('tokens')
            if answer['status'] != 200 or not isinstance(tokens, list):
                raise ValueError('Actual native tokenizer unavailable; no nontruncation assurance')
            counts.append(len(tokens))
        if any(n > 8192 for n in counts) or sum(counts) > 8192:
            raise ValueError('Frozen batch token budget exceeds context/aggregate8192; never truncate or silently reschedule')
        sent = {'request_ordinal': ordinal, 'kind': kind, 'source_keys': source_keys,
                'rendered_sha256': [digest(t.encode('utf-8')) for t in texts], 'token_counts': counts,
                'case_ordinal': case_ordinal, 'body': {'input': texts, 'model': self.alias, 'encoding_format': 'float'}}
        if kind == 'query':
            sent['query_sha256'] = sent['rendered_sha256'][0]
        path = self.root / ('request-' + str(ordinal) + '.json')
        save(path, sent)
        self.requests.append(path)
        answer = request(self.base + '/v1', '/embeddings', sent['body'], timeout=600)
        save(self.root / ('response-' + str(ordinal) + '.json'), answer)
        validate(answer, self.alias, len(texts), 4096)
        self.traces.append({'name': path.name, 'expected_rows': len(texts),
                            'input_sha256': digest(compact(texts).encode())})
        raw = [r['embedding'] for r in sorted(answer['body']['data'], key=lambda r: r['index'])]
        return raw, path, {'seconds': answer['seconds'], 'usage': answer['body'].get('usage'), 'token_counts': counts,
                           'actual_process_rss_kib': command(['ps', '-p', str(self.process.pid), '-o', 'rss='], check=False)}

    def stop(self):
        if self.process is None:
            return {'started': False, 'reaped': True}
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
        receipt = {'pid': self.process.pid, 'run_id': self.run_id, 'exit': self.process.returncode, 'reaped': True}
        save(self.root / 'process-stop.json', receipt)
        bound = {'servers': {'fork': {'local_library_inventory': self.profile['local_library_inventory']}}}
        self.parser.loaded_closure(self.root, 'fork', bound)
        self.parser.cache_evidence(self.root, {'spec': {'cache_profile': {'ram_cache_mib': 0, 'slot_prefix_reuse': 'enabled and measured'}}}, self.traces)
        return receipt


class VectorStore:
    def __init__(self, path, count, dimension=4096):
        import numpy as np
        self.np, self.path = np, path
        self.array = np.memmap(path, dtype='<f4', mode='w+', shape=(count, dimension))
        self.filled = set()

    def put(self, index, vector):
        np = self.np
        values = np.asarray(vector, dtype=np.float32)
        norm = np.linalg.norm(values.astype(np.float64))
        if not np.isfinite(values).all() or not math.isfinite(norm) or norm == 0:
            raise ValueError('Persisted float32 vector finite/nonzero validation failed')
        self.array[index] = values
        self.filled.add(index)

    def finish(self):
        if len(self.filled) != len(self.array):
            raise ValueError('Persisted vector store incomplete')
        self.array.flush()
        with self.path.open('rb') as stream:
            os.fsync(stream.fileno())
        return sha(self.path)

    def rank(self, domain, query):
        np = self.np
        q = np.asarray(query, dtype=np.float64)
        qnorm = np.linalg.norm(q)
        if not np.isfinite(q).all() or not math.isfinite(qnorm) or qnorm == 0:
            raise ValueError('Invalid query vector')
        rows = []
        for start in range(0, len(domain), 256):
            part = domain[start:start + 256]
            chunk = np.asarray(self.array[[item[1] for item in part]], dtype=np.float64)
            norms = np.linalg.norm(chunk, axis=1)
            if not np.isfinite(chunk).all() or not np.isfinite(norms).all() or np.any(norms == 0):
                raise ValueError('Persisted eligible domain has invalid vectors')
            scores = (chunk @ q) / (norms * qnorm)
            for (source, _), score in zip(part, scores):
                rows.append({k: source[k] for k in ('key', 'path', 'id', 'version') if k in source} | {'cosine': float(score)})
        return sorted(rows, key=lambda r: (-r['cosine'], r['key']))
