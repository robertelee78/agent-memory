#!/usr/bin/env python3
"""Synthetic-only actual SSG workspace qualification; never runs a completion model.

Freeze before run. Use the parent's serial host reservation. Nothing outside this
new data subtree is written except the exact receipted loopback tmpfs container.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parent
SSG_HOME = Path('/opt/ssg')
SSG = Path('/tmp/am-v15-delivery/ssg-target/release/ssg')
SSG_COMMIT = '9c4fd08c29f37bffe89ecdf75b069a6baeb8a5e0'
IMAGE = 'docker.io/memgraph/memgraph-mage@sha256:e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27'
CONTAINER = 'am255-workspace-critic'
PORT = 17749
LABEL = 'org.agent-memory.workspace-critic'
SOURCES = ['crates/ssg/src/workspace.rs', 'crates/ssg/src/workspace/paths.rs',
           'crates/ssg/src/workspace/profile.rs', 'crates/ssg/src/workspace/load.rs',
           'crates/ssg/src/trace.rs', 'crates/ssg/src/trace/evidence.rs',
           'crates/ssg/src/assess.rs', 'crates/ssg/src/config.rs',
           'crates/ssg/src/fingerprint/mod.rs', 'crates/ssg/src/fingerprint/cgg.rs',
           'crates/ssg/src/critique.rs', 'crates/ssg/src/critique_local.rs',
           'crates/ssg/src/critique_slotted.rs', 'crates/ssg/src/main.rs',
           'critic/system.md', 'critic/defender.md', 'critic/arbiter.md',
           'expertise/packages/lang/python/facets/sink-eval-exec.md',
           'expertise/packages/lang/python/facets/safe-literal-eval.md',
           'expertise/frameworks/flask/entries/route.md']


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def save(p, value):
    p = Path(p)
    p.parent.mkdir(parents=True, exist_ok=True)
    with p.open('x') as f:
        json.dump(value, f, sort_keys=True, indent=2)
        f.write('\n')


def capture(argv, *, cwd=None, env=None, timeout=180):
    started = time.monotonic()
    try:
        p = subprocess.run([str(x) for x in argv], cwd=cwd, env=env,
                           stdin=subprocess.DEVNULL, capture_output=True,
                           text=True, timeout=timeout)
        return {'argv': [str(x) for x in argv], 'cwd': str(cwd or ROOT),
                'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr,
                'elapsed_seconds_shared_host': time.monotonic() - started}
    except subprocess.TimeoutExpired as e:
        def text(x):
            return x.decode(errors='replace') if isinstance(x, bytes) else (x or '')
        return {'argv': [str(x) for x in argv], 'cwd': str(cwd or ROOT),
                'exit': None, 'timeout_seconds': timeout,
                'stdout': text(e.stdout), 'stderr': text(e.stderr),
                'elapsed_seconds_shared_host': time.monotonic() - started}


def identity():
    source = capture(['git', '-C', SSG_HOME, 'rev-parse', 'HEAD'])
    if source['exit'] or source['stdout'].strip() != SSG_COMMIT:
        raise RuntimeError('Pinned SSG source commit missing or mismatched')
    cgg = shutil.which('cgg')
    engine = shutil.which('podman')
    if not cgg or not engine:
        raise RuntimeError('Actual cgg and podman executables are required')
    files = ['qualify.py', 'expected-facts.json', 'source-analysis.json']
    files += [str(p.relative_to(ROOT)) for p in sorted((ROOT / 'fixture').rglob('*')) if p.is_file()]
    return {'schema': 'am255/workspace-critic-freeze-v1',
            'ssg_commit': SSG_COMMIT, 'ssg_binary': str(SSG), 'ssg_sha256': sha(SSG),
            'cgg': cgg, 'cgg_sha256': sha(cgg), 'cgg_version': capture([cgg, '--version']),
            'engine': engine, 'image': IMAGE, 'container': CONTAINER, 'port': PORT,
            'resources': {'cpus': 2, 'memory_bytes': 2147483648, 'data': 'tmpfs', 'logs': 'tmpfs'},
            'recipe_sha256': {p: sha(ROOT / p) for p in files},
            'source_sha256': {p: sha(SSG_HOME / p) for p in SOURCES},
            'model_calls_authorized': False,
            'shared_graph': 'Never connected. One owned graph first holds expertise, then is replaced by the synthetic repository workspace. During workspace commands SSG_BOLT is a non-listening sentinel, not a shared service.',
            'expected_facts_frozen_before_outputs': True}


def run(out):
    from neo4j import GraphDatabase
    frozen = json.loads((ROOT / 'freeze.json').read_text())
    fresh = identity()
    # Runtime durations are observations, not frozen identity.
    for d in (fresh, frozen):
        d['cgg_version'].pop('elapsed_seconds_shared_host', None)
    if fresh != frozen:
        raise RuntimeError('Frozen recipe, fixture, binary or source identity changed')
    out.mkdir(parents=True, exist_ok=False)
    run_id = uuid.uuid4().hex
    save(out / 'owner.json', {'run_id': run_id, 'container': CONTAINER, 'port': PORT})
    scratch = out / 'scratch'
    scratch.mkdir(mode=0o700)
    (scratch / 'owner').write_text(run_id)
    records = []
    created = None
    absence_proven = False
    report = {'schema': 'am255/workspace-critic-observations-v1', 'run_id': run_id,
              'status': 'unavailable', 'critic_executed': False, 'critic_quality': 'unmeasured',
              'raw_receipts': 'commands.json', 'timings': 'shared host descriptive only'}

    def command(name, argv, **kw):
        result = capture(argv, **kw)
        records.append({'name': name, **result})
        return result

    def need(result):
        if result['exit'] != 0:
            raise RuntimeError('Required command unavailable: ' + str(result['argv']) + ': ' + result['stderr'])
        return result

    def graph(q):
        with GraphDatabase.driver('bolt://127.0.0.1:' + str(PORT), auth=('', ''),
                                  connection_timeout=2, max_connection_pool_size=1) as d:
            with d.session(database='memgraph') as session:
                return [r.data() for r in session.run(q)]

    env = dict(os.environ)
    for key in list(env):
        if key.startswith('SSG_'):
            env.pop(key)
    env.update(SSG_HOME=str(SSG_HOME), SSG_CACHE=str(scratch / 'cache'),
               SSG_CRITIC_CONFIG=str(scratch / 'no-critic-config'),
               SSG_BOLT='127.0.0.1:1', SSG_WORKSPACE_BOLT='127.0.0.1:' + str(PORT),
               SSG_WORKSPACE_CONTAINER=CONTAINER)
    owned_env = dict(env, SSG_BOLT='127.0.0.1:' + str(PORT))

    def ssg(name, args, *, owned=False):
        return command(name, [SSG, *args], env=owned_env if owned else env, cwd=ROOT)

    try:
        names = need(command('container-absence-list', ['podman', 'ps', '-a', '--format', '{{.Names}}']))
        if CONTAINER in names['stdout'].splitlines():
            raise RuntimeError('Refusing existing container name')
        with socket.socket() as s:
            s.bind(('127.0.0.1', PORT))
        absence_proven = True
        save(out / 'precreation.json', {'name_absent': True, 'port_available': True})
        # Podman rejects Docker's tmpfs uid/gid options. Own root init touches only
        # these two private tmpfs roots, then drops to the pinned memgraph UID/GID.
        init = ("import os,sys; "
                "[os.chown(p,101,103) for p in ('/var/lib/memgraph','/var/log/memgraph')]; "
                "os.setgroups([]); os.setgid(103); os.setuid(101); "
                "os.execv('/usr/lib/memgraph/memgraph',['memgraph']+sys.argv[1:])")
        # Retain exit logs until exact ownership-checked cleanup; unlike SSG up,
        # this diagnostic launcher deliberately uses manual removal, not --rm.
        r = need(command('owned-create', ['podman', 'run', '-d', '--pull=never',
            '--name', CONTAINER, '--label', LABEL + '=' + run_id, '--cpus=2', '--memory=2g',
            '-p', '127.0.0.1:' + str(PORT) + ':7687', '--user', '0:0',
            '--tmpfs', '/var/lib/memgraph:rw,mode=0700',
            '--tmpfs', '/var/log/memgraph:rw,mode=0700',
            '--entrypoint', '/usr/bin/python3', IMAGE, '-c', init,
            '--telemetry-enabled=false', '--storage-properties-on-edges=true',
            '--storage-wal-enabled=false', '--storage-snapshot-interval-sec=0',
            '--storage-snapshot-on-exit=false', '--log-level=WARNING']))
        created = r['stdout'].strip()
        inspect = need(command('owned-inspect', ['podman', 'inspect', created]))
        desc = json.loads(inspect['stdout'])[0]
        if desc['Config']['Labels'].get(LABEL) != run_id or desc['Id'] != created:
            raise RuntimeError('Owned creation receipt mismatch')
        save(out / 'container-inspect.json', desc)
        deadline = time.monotonic() + 40
        startup_errors = []
        while True:
            try:
                save(out / 'bolt-ready.json', graph('RETURN 1 AS ready'))
                break
            except Exception as e:
                startup_errors.append(str(e))
                if time.monotonic() > deadline:
                    save(out / 'startup-probes.json', startup_errors)
                    raise RuntimeError('Owned graph startup deadline exceeded') from e
                time.sleep(.25)
        save(out / 'startup-probes.json', startup_errors)
        posture = need(command('actual-tmpfs-process-posture', ['podman', 'exec', '--user', '0:0',
            created, '/usr/bin/python3', '-c',
            "import os,json; print(json.dumps({'pid1_status':open('/proc/1/status').read(),"
            "'mounts':open('/proc/mounts').read(),'directories':"
            "{p:{'uid':os.stat(p).st_uid,'gid':os.stat(p).st_gid,'mode':oct(os.stat(p).st_mode & 0o777)}"
            " for p in ('/var/lib/memgraph','/var/log/memgraph')}}))"]))
        save(out / 'actual-tmpfs-process-posture.json', json.loads(posture['stdout']))
        repo = scratch / 'repo'
        shutil.copytree(ROOT / 'fixture', repo)
        git_env = dict(env, GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null',
                       GIT_TERMINAL_PROMPT='0')
        for key in ('GIT_DIR', 'GIT_WORK_TREE'):
            git_env.pop(key, None)
        for name, args in [('init', ['init', '--initial-branch=main']), ('add', ['add', '.']),
                           ('commit', ['-c', 'user.name=AM synthetic fixture', '-c',
                            'user.email=synthetic@example.invalid', '-c', 'core.hooksPath=/dev/null',
                            'commit', '-m', 'Freeze synthetic workspace controls'])]:
            need(command('fixture-git-' + name, ['git', *args], cwd=repo, env=git_env))
        need(ssg('expertise-build-owned', ['build'], owned=True))
        save(out / 'before-repository.json', graph('MATCH (r:Repo) RETURN count(r) AS repositories'))
        need(ssg('actual-fingerprint', ['fingerprint', str(repo), '--json'], owned=True))
        fps = list((scratch / 'cache' / 'repos').glob('*/*/fingerprint.json'))
        if len(fps) != 1:
            raise RuntimeError('Exactly one synthetic fingerprint required')
        fpdir = fps[0].parent
        for p in fpdir.iterdir():
            if p.is_file():
                shutil.copy2(p, out / ('cached-' + p.name))
        loaded = need(ssg('workspace-load', ['workspace', 'load', '--repo', str(repo), '--json']))
        save(out / 'workspace-load.json', json.loads(loaded['stdout']))
        for name, args in [('paths-default', []), ('paths-high', ['--confidence', 'high']),
                           ('paths-test', ['--trust', 'test']), ('paths-zero-hops', ['--hops', '0'])]:
            r = ssg(name, ['workspace', 'paths', '--repo', str(repo), '--json', *args])
            if r['exit'] == 0:
                save(out / (name + '.json'), json.loads(r['stdout']))
        r = need(ssg('profile-default', ['workspace', 'profile', '--repo', str(repo), '--json']))
        save(out / 'profile-default.json', json.loads(r['stdout']))
        save(out / 'stored-graph-facts.json', {
            'repositories': graph('MATCH (r:Repo) RETURN properties(r) AS repository'),
            'entries': graph('MATCH (e:Entry) RETURN e.qname AS name, e.file AS file, e.entry_trust AS trust ORDER BY name'),
            'typed_edges': graph('MATCH ()-[r]->() RETURN type(r) AS type, count(r) AS count ORDER BY type'),
            'callables': graph('MATCH (c:Callable) WHERE NOT c.synthetic RETURN c.qname AS name, c.file AS file ORDER BY name')})
        cgg = json.loads((fpdir / 'graph.json').read_text())
        def qname(simple):
            found = [c['qualified_name'] for c in cgg['callables'].values()
                     if c.get('simple_name') == simple and not c.get('synthetic')]
            if len(found) != 1:
                raise RuntimeError('Cannot identify unique fixture callable: ' + simple)
            return found[0]
        direct, evaluator, orphan = [qname(s) for s in ('direct', 'evaluate_text', 'orphan')]
        assessment = out / 'assessment.json'
        assess = ['assess', '--repo', str(repo), '--file', str(assessment)]
        need(ssg('assessment-init', [*assess, 'init', '--posture', 'scoped', '--scope',
                                   'Synthetic static fixture only, not a security assessment'], owned=True))
        def claim(name, evidence, statement='Synthetic evidence-resolution control, not a security finding.'):
            args = [*assess, 'claim', '--id', name, '--kind', 'coverage', '--depth', 'scoped',
                    '--statement', statement, '--reasoning', 'Synthetic control; graph structure does not prove dataflow or safety. Proposed expertise remains unreviewed.']
            for e in evidence:
                args += ['--evidence', e]
            return ssg(name, args, owned=True)
        controls = {}
        controls['positive-evidence'] = claim('positive-evidence', ['code:app.py:8-14',
            'fact:lang/python/sink/eval-exec', 'path:' + direct + '->' + evaluator])['exit']
        controls['missing-code'] = claim('missing-code', ['code:missing.py:1-1'])['exit']
        controls['invalid-range'] = claim('invalid-range', ['code:app.py:0-1'])['exit']
        controls['missing-fact'] = claim('missing-fact', ['fact:synthetic/does-not-exist'])['exit']
        controls['unconnected-path'] = claim('unconnected-path', ['path:' + direct + '->' + orphan])['exit']
        controls['unsupported-claim-boundary'] = claim('unsupported-claim-boundary', ['code:app.py:18-19'],
            'DELIBERATELY FALSE NEGATIVE CONTROL: every application path has been proven safe.')['exit']
        (repo / 'app.py').write_text((repo / 'app.py').read_text().replace('return eval(text)', 'return text'))
        save(out / 'mutation.json', {'operation': 'Replace both return eval(text) lines with return text',
                                    'app_sha256_after': sha(repo / 'app.py'), 'refingerprinted': False})
        controls['changed-code'] = claim('changed-code', ['code:app.py:8-14'])['exit']
        stale = ssg('stale-path-boundary', ['workspace', 'paths', '--repo', str(repo), '--json'])
        controls['stale-path-boundary'] = stale['exit']
        if stale['exit'] == 0:
            save(out / 'paths-after-source-mutation.json', json.loads(stale['stdout']))
        save(out / 'evidence-controls.json', controls)
        report.update(status='observed', controls=controls,
                      stored_graph_claim_boundary='No critic/model verdicts computed; unsupported statement control is intentionally false.',
                      shared_graph_isolation='No existing shared graph touched; structural endpoint separation only, not credential-based isolation or alias-proofing.')
    except Exception as e:
        report['unavailable_input_or_failure'] = str(e)
    finally:
        cleanup = {'container_created': created, 'precreation_absence_proven': absence_proven,
                   'synthetic_raw_evidence_retained': True}
        if created:
            result = command('cleanup-owned-inspect', ['podman', 'inspect', created])
            if result['exit'] == 0:
                desc = json.loads(result['stdout'])[0]
                if desc['Id'] == created and desc['Config']['Labels'].get(LABEL) == run_id:
                    command('owned-logs-before-disposal', ['podman', 'logs', created])
                    cleanup['remove'] = command('cleanup-owned-remove', ['podman', 'rm', '-f', created])
                else:
                    cleanup['refused'] = 'Ownership/identity changed; no removal attempted'
            cleanup['inspect_after'] = command('cleanup-inspect-after', ['podman', 'inspect', created])
        names = command('cleanup-authoritative-list', ['podman', 'ps', '-a', '--format', '{{.Names}}'])
        cleanup['container_absent'] = names['exit'] == 0 and CONTAINER not in names['stdout'].splitlines()
        with socket.socket() as s:
            s.settimeout(1)
            cleanup['loopback_listener_absent'] = s.connect_ex(('127.0.0.1', PORT)) != 0
        if (scratch / 'owner').read_text() == run_id:
            cleanup['scratch_inventory'] = [{'path': str(p.relative_to(scratch)), 'sha256': sha(p),
                                             'bytes': p.stat().st_size}
                                            for p in sorted(scratch.rglob('*')) if p.is_file()]
            shutil.rmtree(scratch)
        cleanup['scratch_absent'] = not scratch.exists()
        cleanup['slot_released'] = all(cleanup.get(k) for k in
            ('container_absent', 'loopback_listener_absent', 'scratch_absent'))
        cleanup['guarantee_limits'] = 'Engine-visible absence, tmpfs config and local scratch removal only; not forensic RAM/VM swap/crash-dump/backup erasure.'
        save(out / 'cleanup.json', cleanup)
        save(out / 'commands.json', records)
        save(out / 'observations.json', report)
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report['status'] == 'observed' and cleanup['slot_released'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('freeze', 'run'))
    parser.add_argument('--out', default='actual-20261106')
    args = parser.parse_args()
    if args.action == 'freeze':
        save(ROOT / 'freeze.json', identity())
    else:
        destination = (ROOT / args.out).resolve()
        if destination.parent != ROOT or destination.name in ('fixture', 'scratch'):
            raise SystemExit('Output must be a new direct child of the owned subtree')
        raise SystemExit(run(destination))
