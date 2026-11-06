#!/usr/bin/env python3
from pathlib import Path
import runpy
import sys

sys.dont_write_bytecode = True
_cache_guard = runpy.run_path(str(Path(__file__).resolve().parent / 'source_startup.py'),
                             run_name='am_source_startup')['isolate']()
sys.path.insert(0, str(Path(__file__).resolve().parent))

from agent_memory.omp_boundary import main

if __name__ == '__main__':
    raise SystemExit(main())
