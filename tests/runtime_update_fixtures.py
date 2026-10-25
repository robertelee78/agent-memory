"""Real managed installations and local channels for updater acceptance."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
PACK = ROOT/'skills/agent-memory'


def snapshot(root):
    if root.is_symlink():
        return {'link':os.readlink(root)}
    if not root.exists():
        return None
    if root.is_file():
        return {'bytes':root.read_bytes().hex(),'mode':root.stat().st_mode & 0o777}
    result = {}
    for path in sorted(root.rglob('*')):
        key = path.relative_to(root).as_posix()
        if path.is_symlink(): result[key] = {'link':os.readlink(path)}
        elif path.is_dir(): result[key] = {'directory_mode':path.stat().st_mode & 0o777}
        elif path.is_file():
            result[key] = {'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'mode':path.stat().st_mode & 0o777}
    return result


def version(pack, value):
    path = pack/'SKILL.md'
    text, count = re.subn(r'(metadata: \{"version": ")[^"]+', lambda match:match[1]+value, path.read_text())
    if count != 1: raise AssertionError('Fixture requires the actual root version declaration')
    path.write_text(text)


def fixture(world, mode='copy', *, bootstrap=False):
    channel = world.repository('release channel with spaces')
    pack = channel/'skills/agent-memory'; shutil.copytree(PACK,pack); version(pack,'1.0.0')
    if bootstrap:
        path=pack/'scripts/agent_memory/installation.py'
        text=path.read_text()
        start=text.index("    update = commands.add_parser('update'")
        end=text.index("    commands.add_parser('recover'",start)
        path.write_text(text[:start]+text[end:])
    world.git('add','.',cwd=channel); world.git('commit','-m','Disposable base runtime',cwd=channel)
    config = world.paths['codex']/'AGENTS.md'; config.write_text('User instructions stay here.\n')
    custom = world.inside('custom discovery with spaces')
    installed = world.run([sys.executable,'-I',ROOT/'scripts/installer.py','install','--remote',channel,
                           '--ref','main','--mode',mode,'--presets','codex','--target-dir',custom])
    result = json.loads(installed.stdout)
    if not result['verified']: raise AssertionError(result)
    return channel, pack, result['receipt']


def advance(world, channel, pack, value='1.0.1', *, shim=False):
    version(pack,value)
    (pack/'templates/new-runtime-asset.txt').write_text('Useful new runtime asset.\n')
    if shim:
        path = pack/'templates/activation-markdown.example.txt'
        path.write_text(path.read_text().replace('When this repository', 'For this project, when this repository'))
    world.git('add','.',cwd=channel); world.git('commit','-m','Disposable candidate runtime',cwd=channel)
    return world.git('rev-parse','HEAD',cwd=channel).stdout.strip()


def call(world, receipt, *args, expected=0, env=None, source=False):
    command = [sys.executable,'-I',ROOT/'scripts/update_runtime.py','update'] if source else [Path(receipt['bin_dir'])/'agent-memory', '--json','update']
    result = world.run([*command,*args],check=False,env=env,timeout=120)
    if result.returncode != expected: raise AssertionError((result.returncode,result.stdout,result.stderr))
    return result, json.loads(result.stdout) if result.stdout else None


def receipt_path(world):
    return world.paths['state']/'agent-memory/install/receipt.json'


def owned(receipt):
    return {target['path']:snapshot(Path(target['path'])) for target in receipt['targets']}


def bank(world, repo):
    proposal={'project-id':'8e0b66aa-2be5-4fa3-b47f-1245474aa7dd','router-updated':'2026-10-22T17:00:00Z',
              'contexts':{'projectbrief.md':{'body':'# Project\n\nPreserve durable shared learning.\n',
                  'purpose':'Orient the agent','scope':'repository','lifecycle':'active','read-when':'When resuming work'}},
              'omitted':{name:'No applicable evidence yet' for name in
                         ('productContext.md','activeContext.md','systemPatterns.md','techContext.md','progress.md')}}
    path=world.inside('bank-input.json'); path.write_text(json.dumps(proposal))
    rendered=json.loads(world.run([sys.executable,'-I',ROOT/'scripts/bank_contract.py','render','--input',path]).stdout)
    directory=repo/'memory-bank'; directory.mkdir()
    for name,body in rendered['files'].items(): (directory/name).write_text(body)
    world.run([sys.executable,'-I',ROOT/'scripts/bank_contract.py','validate','--repo',repo])
    return directory
