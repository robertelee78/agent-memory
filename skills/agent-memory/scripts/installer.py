#!/usr/bin/env python3
"""Install the portable runtime or recover its interrupted activation."""
from pathlib import Path
import sys
if sys.version_info < (3, 11):
    print('am: Python 3.11+ must be available as python3 on PATH to run agent-memory.', file=sys.stderr)
    sys.exit(1)
sys.dont_write_bytecode = True
import runpy
_cache_guard = runpy.run_path(str(Path(__file__).resolve().parent/'source_startup.py'),
                              run_name='am_source_startup')['isolate']()
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.installation import main
if __name__ == '__main__':
    from agent_memory import console
    name = console.installed_name()
    sys.exit(console.main(prog=name) if name else main())
