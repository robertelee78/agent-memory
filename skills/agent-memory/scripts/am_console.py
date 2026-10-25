#!/usr/bin/env python3
"""Public bootstrap presentation; governed operations remain in the helpers."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.console import main
if __name__ == '__main__':
    sys.exit(main(bootstrap=True))
