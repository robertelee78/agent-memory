#!/usr/bin/env python3
"""Public portable entry point for safe deletion markers and rebuild guards."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.tombstones import main

if __name__ == "__main__":
    sys.exit(main())
