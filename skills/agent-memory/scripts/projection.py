#!/usr/bin/env python3
"""Build or check a disposable source-bound reference projection."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.projections import main

if __name__ == '__main__':
    sys.exit(main())
