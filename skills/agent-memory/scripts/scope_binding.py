#!/usr/bin/env python3
"""Public shared scope resolver and binding operation before runtime installation."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.scope_binding import main

if __name__ == "__main__":
    sys.exit(main())
