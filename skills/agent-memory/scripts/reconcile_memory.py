#!/usr/bin/env python3
"""Review the meaning of local and landed project memory before exchange."""
from pathlib import Path
import sys
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parent))
from agent_memory.semantic_sharing import main
if __name__=='__main__':sys.exit(main())
