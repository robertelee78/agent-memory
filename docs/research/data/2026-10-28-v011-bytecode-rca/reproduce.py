"""Reproduce an external diagnostic import, never against an operator profile.

Run with python3 -I -B; --signed-root is copied before any candidate import.
Only the deliberately unguarded child omits -B. It runs in a disposable World.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--signed-root', required=True)
location = Path(__file__).resolve()
parser.add_argument('--tests-root', default=str(location.parents[4]) if len(location.parents) > 4 else None)
args = parser.parse_args()
if not args.tests_root:
    parser.error('--tests-root is required outside the repository data directory')
sys.path.insert(0, str(Path(args.tests_root)/'tests'))
from world import World
from contracts.test_sharing_boundary import SharingBoundaryContract

proof = Path(tempfile.mkdtemp(prefix='am-v011-cache-repro-complete-', dir='/tmp')).resolve()
proof.chmod(0o700)
commands = []

def run(label, argv, env, cwd, input=None):
    start = time.monotonic()
    result = subprocess.run([str(a) for a in argv], env=env, cwd=cwd, input=input,
                            capture_output=True, text=True, timeout=180)
    commands.append({'label': label, 'argv': [str(a) for a in argv], 'exit': result.returncode,
                     'seconds': time.monotonic()-start, 'stdout': result.stdout, 'stderr': result.stderr})
    (proof/'commands.json').write_text(json.dumps(commands, indent=2)+'\n')
    return result

with World() as w:
    signed = w.inside('signed-original'); shutil.copytree(args.signed_root, signed)
    manifest = json.loads((signed/'release-manifest.json').read_text())
    assert not list(signed.rglob('*.pyc'))
    bootstrap = ('from pathlib import Path;import sys;sys.path.insert(0,sys.argv[1]);'
                 'from agent_memory import installation;installation.BOOTSTRAP_SOURCE=Path(sys.argv[2]);'
                 "raise SystemExit(installation.main(['install','--presets','none']))")
    installed = run('verified-signed-bootstrap-install',
                    [sys.executable, '-I', '-B', '-c', bootstrap, signed/'skills/agent-memory/scripts', signed],
                    w.env, w.paths['repo'])
    assert installed.returncode == 0, (installed.stdout, installed.stderr)
    install = json.loads(installed.stdout); receipt = install['receipt']
    assert install['verified'] and receipt['source']['kind'] == 'release'
    assert receipt['source']['revision'] == manifest['source_commit']
    active = Path(receipt['source']['path']); cli = Path(receipt['bin_dir'])/'am'
    receipt_path = Path(w.env['AGENT_MEMORY_STATE_HOME'])/'install/receipt.json'
    scripts = active/'skills/agent-memory/scripts'
    helper = SharingBoundaryContract(); repo, bank = helper.fixture(w)
    w.git('push', 'origin', 'HEAD:main', cwd=repo)
    env = w.env.copy(); env.pop('PYTHONDONTWRITEBYTECODE', None)
    version = run('guarded-cli-without-environment-guard', [cli, '--version'], env, repo)
    assert version.returncode == 0
    help_result = run('owned-helper-without-environment-guard',
                      [sys.executable, scripts/'reconcile_memory.py', '--help'], env, repo)
    assert help_result.returncode == 0
    inspection = w.inside('public-inspection.json')
    observed = run('public-inspection-before-diagnostic',
                   [cli, 'semantic', 'inspect-v2', '--repo', repo, '--output', inspection], env, repo)
    assert observed.returncode == 0, (observed.stdout, observed.stderr)
    assert not list(active.rglob('*.pyc'))
    diagnostic = '''import json,sys
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from agent_memory import semantic_sharing
manifest=json.loads(Path(sys.argv[3]).read_text())
decisions={'pages':[p['sha256'] for p in manifest['pages']],
 'pairs':[{'index':i,'classification':'related','reason':'Read actual tiny disposable fixture sources.'} for i,_ in enumerate(manifest['required_pairs'])],
 'consolidations':[],'rationale':'Mechanical cache reproduction using a complete disposable public manifest.'}
try:
 result=semantic_sharing.compact_review(sys.argv[2],manifest,decisions)
 print(json.dumps({'review_returned':True,'canonical_writes_performed':result.get('canonical_writes_performed')}))
except Exception as exc:
 print(json.dumps({'review_returned':False,'exception_type':type(exc).__name__}))
'''
    before = helper.state(w, repo, bank)
    control = run('same-direct-diagnostic-with-I-B',
                  [sys.executable, '-I', '-B', '-', scripts, repo, inspection], env, repo, diagnostic)
    assert control.returncode == 0 and not list(active.rglob('*.pyc'))
    receipt_before = hashlib.sha256(receipt_path.read_bytes()).hexdigest()
    raw = run('observed-plain-python-diagnostic',
              [sys.executable, '-', scripts, repo, inspection], env, repo, diagnostic)
    assert raw.returncode == 0
    caches = sorted(str(p.relative_to(active)) for p in active.rglob('*.pyc')); assert caches
    update = run('public-update-after-raw-import', [cli, 'update'], env, repo)
    assert update.returncode != 0 and 'Installed portable identity differs from its signed release' in update.stderr
    doctor = run('public-doctor-after-raw-import', [cli, '--json', 'doctor', '--repo', repo], env, repo)
    value = json.loads(doctor.stdout); assert doctor.returncode != 0 and not value['ok']
    result = {'reproduced': True, 'version': manifest['version'], 'source_commit': manifest['source_commit'],
              'signed_install_verified': install['verified'], 'normal_cli_and_owned_helper_caches': 0,
              'same_diagnostic_I_B_caches': 0, 'raw_python_cache_count': len(caches), 'cache_names': caches,
              'public_update_refused': True, 'public_update_error': update.stderr,
              'doctor_ok': value['ok'], 'doctor_failure_components': [c['component'] for c in value['failures']],
              'receipt_preserved': hashlib.sha256(receipt_path.read_bytes()).hexdigest() == receipt_before,
              'bank_index_HEAD_preserved': helper.state(w, repo, bank) == before,
              'actual_profile_modified': False, 'world_retained': False,
              'proof': str(proof), 'python': sys.version}
    assert result['receipt_preserved'] and result['bank_index_HEAD_preserved']
    (proof/'result.json').write_text(json.dumps(result, indent=2)+'\n')
    (proof/'controller.py').write_text(Path(__file__).read_text())
    print(json.dumps(result))
