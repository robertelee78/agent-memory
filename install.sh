#!/usr/bin/env bash
# Complete local checkouts and curl-to-bash use the same transactional installer.
# Keep invocation last: a partial download must not start an incomplete function.
main() (
  set -euo pipefail

  for argument in "$@"; do
    case "$argument" in
      -h|--help)
        cat <<'HELP'
Install agent-memory from a checkout or with curl-to-bash.

Usage: install.sh [options]
  --check, --dry-run        Preview without changing installation targets
  --mode link|copy         Discovery mode (default: link)
  --presets LIST           all, none, or agents,codex,claude,opencode
  --target-dir PATH        Add a custom skills directory; may be repeated
  --bin-dir PATH           Command directory (default: ~/.local/bin)
  --no-guidance            Omit client activation guidance
  --remote URL --ref NAME  Select a managed Git source (default: HTTPS main)
  --development-source PATH  Select a complete local development package

After installation: am update, am uninstall
The agent-memory command remains available for compatibility.
HELP
        exit 0
        ;;
    esac
  done

  command -v python3 >/dev/null 2>&1 || {
    echo 'Python 3.11+ is required to install agent-memory' >&2
    exit 1
  }
  python3 -I -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' || {
    echo 'Python 3.11+ is required to install agent-memory' >&2
    exit 1
  }
  command -v git >/dev/null 2>&1 || {
    echo 'Git is required to install agent-memory' >&2
    exit 1
  }

  script_path="${BASH_SOURCE[0]:-}"
  if [[ -n "$script_path" && -f "$script_path" ]]; then
    script_dir=$(CDPATH='' cd -- "$(dirname -- "$script_path")" && pwd)
    if [[ -f "$script_dir/skills/agent-memory/scripts/installer.py" ]]; then
      exec python3 -I "$script_dir/skills/agent-memory/scripts/installer.py" install "$@"
    fi
  fi

  # Stage only the loader here. The Python installer separately owns the durable
  # managed checkout, locks, receipt, target checks and recovery journal.
  bootstrap_remote='https://github.com/robertelee78/agent-memory.git'
  bootstrap_ref='main'
  # Parse a function's positional arguments so the originals remain intact,
  # including the zero-argument case on Bash 3.2 with nounset enabled.
  bootstrap_options() {
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --remote|--ref|--development-source|--mode|--presets|--target-dir|--bin-dir)
          [[ $# -ge 2 && -n "$2" ]] || { echo "$1 requires a value" >&2; exit 2; }
          case "$1" in
            --remote) bootstrap_remote="$2" ;;
            --ref) bootstrap_ref="$2" ;;
          esac
          shift 2
          ;;
        --remote=*) bootstrap_remote="${1#*=}"; shift ;;
        --ref=*) bootstrap_ref="${1#*=}"; shift ;;
        --development-source=*|--mode=*|--presets=*|--target-dir=*|--bin-dir=*) shift ;;
        --check|--dry-run|--no-guidance) shift ;;
        *) echo "Unknown install option: $1 (use --help)" >&2; exit 2 ;;
      esac
    done
  }
  bootstrap_options "$@"
  [[ -n "$bootstrap_remote" && -n "$bootstrap_ref" && "$bootstrap_ref" != -* ]] || {
    echo 'A nonempty remote and valid source branch are required' >&2
    exit 2
  }

  bootstrap_dir=$(mktemp -d "${TMPDIR:-/tmp}/agent-memory-bootstrap.XXXXXXXX")
  trap 'rm -rf -- "$bootstrap_dir"' EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  GIT_TERMINAL_PROMPT=0 git clone --quiet --depth 1 --single-branch \
    --branch "$bootstrap_ref" -- "$bootstrap_remote" "$bootstrap_dir/source" </dev/null
  # Do not exec here: the EXIT trap must remove scratch after success or failure.
  python3 -I "$bootstrap_dir/source/skills/agent-memory/scripts/installer.py" \
    install "$@" </dev/null
)

main "$@"
