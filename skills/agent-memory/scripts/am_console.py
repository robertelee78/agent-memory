#!/usr/bin/env python3
"""Public bootstrap presentation; governed operations remain in the helpers."""
from pathlib import Path
import sys
if sys.version_info < (3, 11):
    print('am: Python 3.11+ must be available as python3 on PATH to run agent-memory.', file=sys.stderr)
    sys.exit(1)
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.console import main
if __name__ == '__main__':
    sys.exit(main(bootstrap=True))
