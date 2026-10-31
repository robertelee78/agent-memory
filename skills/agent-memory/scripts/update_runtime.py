#!/usr/bin/env python3
"""Check or activate the recorded managed runtime channel."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
import runpy
_cache_guard = runpy.run_path(str(Path(__file__).resolve().parent/'source_startup.py'),
                              run_name='am_source_startup')['isolate']()
sys.path.insert(0,str(Path(__file__).resolve().parent))
from agent_memory.updating import main
if __name__=='__main__':
    sys.exit(main())
