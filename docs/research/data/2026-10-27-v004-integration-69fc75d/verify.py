#!/usr/bin/env python3
"""Verify retained bridge evidence against its exact Git source and raw logs."""
import gzip, hashlib, json, pathlib, re, subprocess, sys
here = pathlib.Path(__file__).resolve().parent
commit = '69fc75d44f76921c6c984a6d4016cc03dd1d7cf1'
root = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else pathlib.Path.cwd()
inventory = json.loads((here/'inventory.json').read_text())
actual = {p.relative_to(here).as_posix() for p in here.rglob('*') if p.is_file()} - {'inventory.json'}
assert actual == {r['path'] for r in inventory['files']}, 'evidence inventory changed'
for row in inventory['files']:
    raw = (here/row['path']).read_bytes()
    assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']
manifest = json.loads((here/'source-manifest.json').read_text())
assert manifest['commit'] == commit
raw = subprocess.check_output(['git','-C',str(root),'ls-tree','-rz','--full-tree',commit,'--','skills/agent-memory','tests','memory-bank','scripts/test_gate.py'])
expected=[]
for item in raw.split(b'\0'):
    if item:
        header,path=item.split(b'\t',1); mode,kind,blob=header.decode().split()
        expected.append({'path':path.decode(),'mode':mode,'blob':blob})
assert manifest['files'] == expected and len(expected) == 343, 'source binding differs from Git'
cases={'v2-refresh':1,'v2-publish':1,'recovery':2,'later-merge':1,'simultaneous':1,'cross-clone':1,'conflict':1}
for host in ('macos','linux'):
    directory=here/host
    for phase in ('before','after'):
        source=json.loads((directory/('source-'+phase+'.json')).read_text())
        assert source['commit']==commit and source['files']==343 and source['mismatches']==[], (host,phase)
    for name,count in cases.items():
        receipt=json.loads((directory/(name+'.json')).read_text())
        stdout=gzip.decompress((directory/(name+'.stdout.gz')).read_bytes())
        stderr=gzip.decompress((directory/(name+'.stderr.gz')).read_bytes())
        assert receipt['source_commit']==commit and receipt['exit_code']==0 and not receipt['timed_out'],(host,name)
        assert receipt['elapsed_seconds'] < 900 and receipt['timeout_seconds'] == 840,(host,name)
        assert receipt['stdout_sha256']==hashlib.sha256(stdout).hexdigest() and receipt['stderr_sha256']==hashlib.sha256(stderr).hexdigest(),(host,name)
        assert '-v' in receipt['argv'] and re.search(fr'Ran {count} tests? in [0-9.]+s\s+\nOK',stderr.decode()),(host,name)
        methods=[line for line in stderr.decode().splitlines() if line.startswith('test_') and ' ... ' in line]
        assert len(methods)==count,(host,name)
print('Verified 14 commands, 16 named tests, 343 source blobs and modes per host.')
