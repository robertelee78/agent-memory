#!/usr/bin/env python3
"""Import only freshly reviewed landed memory, preserving feature code and local learning."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.memory_refresh import main
if __name__ == '__main__':
    sys.exit(main())
