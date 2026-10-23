#!/usr/bin/env python3
"""Search, read and bind agent novelty judgments to current canonical memory."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.comparison import main
if __name__ == '__main__':
    sys.exit(main())
