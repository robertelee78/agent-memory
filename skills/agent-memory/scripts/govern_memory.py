#!/usr/bin/env python3
"""Portable entry point for governed memory lifecycle operations."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.governance import main

if __name__ == '__main__':
    sys.exit(main())
