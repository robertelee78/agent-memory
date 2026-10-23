#!/usr/bin/env python3
"""Checkout entrypoint for the portable sharing recovery helper."""
from pathlib import Path
import runpy
import sys
sys.dont_write_bytecode = True
runpy.run_path(str(Path(__file__).resolve().parents[1] / 'skills/agent-memory/scripts/sharing_recovery.py'), run_name='__main__')
