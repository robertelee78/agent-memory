#!/usr/bin/env python3
"""Inspect the portable authority model and triage reviewed assertions."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.authority import main

if __name__ == '__main__':
    sys.exit(main())
