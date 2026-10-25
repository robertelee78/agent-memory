#!/usr/bin/env python3
"""Install the portable runtime or recover its interrupted activation."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_memory.installation import main
if __name__ == '__main__':
    from agent_memory import console
    name = console.installed_name()
    sys.exit(console.main(prog=name) if name else main())
