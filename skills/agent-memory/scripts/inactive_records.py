#!/usr/bin/env python3
"""Inspect safe inactive representations and invalidate stale derived input."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.inactive import main

if __name__ == '__main__':
    sys.exit(main())
