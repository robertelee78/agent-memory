#!/usr/bin/env python3
"""Inspect runtime and bank health without installing, repairing, or deleting."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.diagnostics import main
if __name__ == '__main__':
    sys.exit(main())
