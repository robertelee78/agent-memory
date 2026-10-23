#!/usr/bin/env python3
"""Checkout entrypoint for source-bound memory refresh."""
from pathlib import Path
import runpy
import sys
sys.dont_write_bytecode = True
runpy.run_path(str(Path(__file__).resolve().parents[1]/'skills/agent-memory/scripts/refresh_memory.py'), run_name='__main__')
