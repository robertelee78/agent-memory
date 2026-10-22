#!/usr/bin/env python3
"""Checkout entrypoint invokes the same helper shipped inside the portable pack."""
from pathlib import Path
import runpy
import sys

sys.dont_write_bytecode = True
runpy.run_path(str(Path(__file__).resolve().parents[1] / "skills/agent-memory/scripts/bank_contract.py"), run_name="__main__")
