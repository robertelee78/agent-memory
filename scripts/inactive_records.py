#!/usr/bin/env python3
"""Invoke the inactive representation helper in the portable artifact."""
from pathlib import Path
import runpy
import sys

sys.dont_write_bytecode = True
runpy.run_path(str(Path(__file__).resolve().parents[1] / 'skills/agent-memory/scripts/inactive_records.py'), run_name='__main__')
