#!/usr/bin/env python3
"""Prepare and land memory independently of feature code, following GitHub rules."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.memory_publication import main
if __name__ == '__main__':
    sys.exit(main())
