#!/bin/sh
# Bootstrap uses the same portable installer, including recovery before a wrapper exists.
set -eu
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
exec python3 -I "$script_dir/skills/agent-memory/scripts/installer.py" install "$@"
