#!/bin/sh
# Resolve a supported interpreter even when an older system Python precedes it.
old_ifs=$IFS
IFS=:
set -f
for dir in $PATH; do
  [ -n "$dir" ] || dir=.
  candidate=$dir/python3
  if [ -x "$candidate" ] && "$candidate" -I -c 'import sys; sys.exit(sys.version_info < (3, 11))' >/dev/null 2>&1; then
    IFS=$old_ifs
    exec "$candidate" -I -B "$@"
  fi
done
printf '%s\n' 'am: Python 3.11+ must be available as python3 on PATH.' >&2
exit 1
