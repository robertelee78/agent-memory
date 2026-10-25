#!/usr/bin/env python3
"""Explicit source-bound bank migration and recovery; never a runtime update."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.migration import main

if __name__ == '__main__':
    sys.exit(main())
