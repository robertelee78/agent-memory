#!/usr/bin/env python3
"""Assemble bounded project evidence through the portable package."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.assembly import main

if __name__ == '__main__':
    sys.exit(main())
