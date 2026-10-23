#!/usr/bin/env python3
"""Run the portable implementation from this checkout."""
from pathlib import Path
import runpy
import sys

sys.dont_write_bytecode = True
runpy.run_path(str(Path(__file__).resolve().parents[1] / 'skills/agent-memory/scripts/govern_memory.py'), run_name='__main__')
