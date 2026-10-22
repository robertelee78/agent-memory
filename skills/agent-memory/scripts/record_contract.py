#!/usr/bin/env python3
"""Validate typed Markdown records without admitting or modifying memory."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.records import main

if __name__ == '__main__':
    sys.exit(main())
