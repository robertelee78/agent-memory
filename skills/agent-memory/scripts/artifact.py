#!/usr/bin/env python3
"""Inspect the complete client-neutral portable artifact."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.artifact import main

if __name__ == '__main__':
    sys.exit(main())
