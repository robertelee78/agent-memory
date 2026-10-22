#!/usr/bin/env python3
"""Public portable render/validate helper; requires no installed runtime."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.bank_contract import main

if __name__ == "__main__":
    sys.exit(main())
