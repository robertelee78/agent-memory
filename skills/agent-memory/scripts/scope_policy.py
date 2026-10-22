#!/usr/bin/env python3
"""Evaluate the portable shared-repository scope policy."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.scopes import main

if __name__ == '__main__':
    sys.exit(main())
