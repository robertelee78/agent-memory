#!/usr/bin/env python3
"""Remove proven runtime ownership using the external receipt."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.uninstallation import main
if __name__ == '__main__':
    sys.exit(main())
