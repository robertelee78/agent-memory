"""Public launchers must never import opaque installed bytecode caches."""
from pathlib import Path
import hashlib, importlib.util, os, py_compile, shutil, subprocess, sys, tempfile, unittest
ROOT=Path(__file__).resolve().parents[1]
PACK=ROOT/'skills/agent-memory'

class SourceStartupContract(unittest.TestCase):
    def test_entrypoints_and_hook_ignore_preserved_poison_cache(self):
        with tempfile.TemporaryDirectory(prefix='am-startup-contract-') as temporary:
            root=Path(temporary); pack=root/'agent-memory'; shutil.copytree(PACK,pack)
            module=pack/'scripts/agent_memory/console.py'; marker=root/'cache-executed'; poison=root/'poison.py'
            body=f"from pathlib import Path\nPath({str(marker)!r}).write_text('unexpected')\nraise RuntimeError('poison loaded')\n"
            self.assertLess(len(body.encode()),module.stat().st_size)
            poison.write_text(body+'#'*(module.stat().st_size-len(body.encode())))
            stamp=module.stat(); os.utime(poison,ns=(stamp.st_atime_ns,stamp.st_mtime_ns))
            cached=Path(importlib.util.cache_from_source(str(module))); cached.parent.mkdir(exist_ok=True)
            py_compile.compile(str(poison),cfile=str(cached),dfile=str(module),doraise=True,invalidation_mode=py_compile.PycInvalidationMode.TIMESTAMP)
            original=hashlib.sha256(cached.read_bytes()).hexdigest()
            target=root/'probe.py'
            target.write_text('import sys\nsys.path.insert(0,'+repr(str(pack/'scripts'))+')\nfrom agent_memory import console\nprint("source-only")\nprint(sys.pycache_prefix)\n')
            commands=[[sys.executable,'-I','-B',pack/'scripts'/name,'--help'] for name in ('installer.py','am_console.py','update_runtime.py')]
            commands.append(['/bin/sh',pack/'hooks/python3.sh',target])
            for command in commands:
                with self.subTest(command=str(command[-2:])):
                    result=subprocess.run([str(v) for v in command],capture_output=True,text=True,timeout=20)
                    self.assertEqual(result.returncode,0,(result.stdout,result.stderr)); self.assertFalse(marker.exists())
                    self.assertEqual(hashlib.sha256(cached.read_bytes()).hexdigest(),original)
            prefix=Path(result.stdout.splitlines()[-1]); self.assertIn('am-source-only-',prefix.name)
            self.assertFalse(prefix.exists(),'Private prefix survives normal exit')
            self.assertEqual(list(pack.rglob('*.pyc')),[cached])

if __name__=='__main__': unittest.main()
