#!/usr/bin/env python3
"""Observe small public command examples in an isolated credential-free world."""
import hashlib
import json
from pathlib import Path
import sys
import uuid

ROOT=Path(__file__).resolve().parents[1]
sys.dont_write_bytecode=True
sys.path.insert(0,str(ROOT/'tests'))
from world import World


def main():
    pack=ROOT/'skills/agent-memory';helper=pack/'scripts/bank_contract.py'
    fixture=json.loads((pack/'templates/router-proposal.example.json').read_bytes());fixture['project-id']=str(uuid.uuid4())
    examples=[]
    with World() as world:
        good=world.inside('render.json');good.write_text(json.dumps(fixture))
        bad=world.inside('invalid.json');bad.write_text('{}')
        cases=[('useful-router-render',['render','--input',good],['render','--input','REVIEWED_INPUT.json']),
               ('missing-render-fields',['render','--input',bad],['render','--input','EMPTY_INPUT.json']),
               ('unknown-operation',['unknown-operation'],['unknown-operation'])]
        for name,args,display in cases:
            actual=[sys.executable,'-I',helper,*args];result=world.run(actual,check=False)
            item={'id':name,'observed_argv':[str(a) for a in actual],
                  'command':['python3','-I','$SKILL_DIR/scripts/bank_contract.py',*display],
                  'exit':result.returncode,'stdout_sha256':hashlib.sha256(result.stdout.encode()).hexdigest(),
                  'stderr_sha256':hashlib.sha256(result.stderr.encode()).hexdigest()}
            if result.stdout:
                output=json.loads(result.stdout)
                item['observed_fields']={k:output[k] for k in ('schema','operation','valid','writes_performed','error') if k in output}
                if name=='useful-router-render':item['observed_file_paths']=sorted(output['files'])
            if result.stderr:item['observed_stderr']=result.stderr
            examples.append(item)
    print(json.dumps({'schema':'agent-memory/observed-guide-examples-v1',
        'helper_path':'skills/agent-memory/scripts/bank_contract.py','helper_sha256':hashlib.sha256(helper.read_bytes()).hexdigest(),
        'artifact_files':{p.relative_to(pack).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(pack.rglob('*')) if p.is_file() and '__pycache__' not in p.parts},
        'examples':examples,'live_credentials_required':False,'live_configuration_changed':False,
        'runtime_paths_are_disposable':True},indent=2));return 0


if __name__=='__main__':sys.exit(main())
