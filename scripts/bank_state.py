#!/usr/bin/env python3
"""Invoke the current-bank inspector shipped in the portable artifact."""
from pathlib import Path
import runpy
import sys

sys.dont_write_bytecode = True
runpy.run_path(str(Path(__file__).resolve().parents[1] / 'skills/agent-memory/scripts/bank_state.py'), run_name='__main__')
