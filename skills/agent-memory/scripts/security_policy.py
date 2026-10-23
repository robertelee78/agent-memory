#!/usr/bin/env python3
"""Evaluate bounded project security policy without executing source text."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.security import main

if __name__ == '__main__':
    sys.exit(main())
