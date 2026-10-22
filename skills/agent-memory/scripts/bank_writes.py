#!/usr/bin/env python3
"""Public developer-attribution initializer/selector before runtime installation."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.bank_writes import main

if __name__ == "__main__":
    sys.exit(main())
