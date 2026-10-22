#!/bin/sh
# One public entrypoint; works before agent-memory has been installed.
set -eu
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec python3 -I "$SCRIPT_DIR/test_gate.py" "$@"
