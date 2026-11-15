#!/usr/bin/env bash
# Real-use QA: drive the actual `am` the way a user would, in a temporary
# HOME, and record each command's result and wall time.
#
#   scripts/qa.sh CHECKOUT [RESULTS_FILE]
#
# CHECKOUT is an agent-memory checkout with a built target/release/am.
# Covers: install (development source), init, remember, recall, share and
# refresh between two clones of a bare remote, the session-start hook,
# status, doctor and uninstall. Needs bash 5 (EPOCHREALTIME) and git.
set -euo pipefail

src=$(cd "${1:?usage: qa.sh CHECKOUT [RESULTS_FILE]}" && pwd -P)
results=${2:-/dev/stdout}
work=$(mktemp -d "${TMPDIR:-/tmp}/am-qa.XXXXXX")
trap 'rm -rf "$work"' EXIT

export HOME="$work/home"
unset XDG_STATE_HOME XDG_DATA_HOME XDG_CONFIG_HOME CODEX_HOME CLAUDE_CONFIG_DIR GIT_DIR GIT_WORK_TREE
mkdir -p "$HOME/.claude" "$HOME/.omp/agent"
export PATH="$HOME/.local/bin:$PATH"
git config --global user.name "QA User"
git config --global user.email qa@example.com
git config --global init.defaultBranch main
git config --global pull.rebase false
# An existing user setting the installer must preserve.
printf '{\n  "model": "opus"\n}\n' > "$HOME/.claude/settings.json"

table=""
failures=0
step() { # step LABEL DIR COMMAND...
  local label=$1 dir=$2
  shift 2
  local start=$EPOCHREALTIME status=0
  (cd "$dir" && "$@") > "$work/out" 2>&1 || status=$?
  local ms=$(( (${EPOCHREALTIME/./} - ${start/./}) / 1000 ))
  local verdict=pass
  [ "$status" -eq 0 ] || { verdict="FAIL ($status)"; failures=$((failures + 1)); }
  table+=$(printf '| %s | `%s` | %s ms | %s |' "$label" "$*" "$ms" "$verdict")$'\n'
  printf '== %s (%s ms, %s)\n' "$label" "$ms" "$verdict" >&2
  sed 's/^/   /' "$work/out" >&2
}
expect() { # expect DESCRIPTION COMMAND...
  if "${@:2}" > /dev/null 2>&1; then
    table+="| check | $1 | | pass |"$'\n'
  else
    table+="| check | $1 | | FAIL |"$'\n'
    failures=$((failures + 1))
    printf '!! check failed: %s\n' "$1" >&2
  fi
}

step install "$work" "$src/target/release/am" install --development-source "$src" --presets agents,codex,claude,opencode,omp --omp-agent-dir "$HOME/.omp/agent"
# From here on `am` is the installed command on PATH.
expect "am on PATH is the installed link" test -L "$HOME/.local/bin/am"
expect "Claude SessionStart hook registered" grep -q 'hook session-start --client claude' "$HOME/.claude/settings.json"
expect "existing Claude setting kept" grep -q '"model": "opus"' "$HOME/.claude/settings.json"
expect "Codex SessionStart hook registered" grep -q 'hook session-start --client codex' "$HOME/.codex/hooks.json"
expect "omp extension written" test -f "$HOME/.omp/agent/extensions/agent-memory.ts"
expect "skill linked for Claude" test -f "$HOME/.claude/skills/agent-memory/SKILL.md"

git init -q --bare "$work/remote.git"
git clone -q "$work/remote.git" "$work/a" 2>/dev/null
(cd "$work/a" && echo "# Demo" > README.md && git add README.md && git commit -qm init && git push -q origin main)
git clone -q "$work/remote.git" "$work/b"
git clone -q "$work/remote.git" "$work/c"
(cd "$work/c" && echo "local work" > work.txt && git add work.txt && git commit -qm "local work")

step init "$work/a" am init
step remember "$work/a" am remember --kind fix --title "Musl builds need rust-lld" \
  --body "Cross-building the Linux binaries on macOS fails with the platform linker; set CARGO_TARGET_<TRIPLE>_LINKER=rust-lld." \
  --evidence "QA run" --read-when "When cross-building release binaries"
step "remember duplicate" "$work/a" am remember --kind fix --title "Musl builds need rust-lld" --body "same lesson again"
expect "duplicate skipped" test "$(ls "$work/a/memory-bank/topics" | grep -c '\.md$')" -eq 1
step "remember (stdin JSON)" "$work/a" sh -c 'printf "%s" "{\"title\":\"Use git worktrees per session\",\"body\":\"Each agent session works in its own worktree.\",\"kind\":\"decision\",\"evidence\":[\"team agreement\"]}" | am remember'
step recall "$work/a" am recall
step "recall QUERY" "$work/a" am recall musl linker
expect "recall finds the note" sh -c "cd '$work/a' && am recall musl | grep -q rust-lld"
step share "$work/a" am share
expect "only memory-bank/ staged in A" sh -c "cd '$work/a' && test -z \"\$(git diff --cached --name-only | grep -v '^memory-bank/')\""
expect "remote main has the bank" sh -c "git -C '$work/remote.git' ls-tree -r --name-only main | grep -q memory-bank/MEMORY.md"
expect "A's HEAD did not move" sh -c "test \"\$(git -C '$work/a' rev-parse HEAD)\" = \"\$(git -C '$work/b' rev-parse HEAD)\""

step refresh "$work/b" am refresh
expect "B received A's note" sh -c "cd '$work/b' && am recall musl | grep -q rust-lld"
step "git pull after refresh (clone B)" "$work/b" git pull -q
expect "B clean and at origin/main after pull" sh -c "cd '$work/b' && test -z \"\$(git status --porcelain)\" && test \"\$(git rev-parse HEAD)\" = \"\$(git rev-parse origin/main)\""
step "remember (clone B)" "$work/b" am remember --kind correction --title "Prefer short commits" --body "The user wants one logical change per commit." --evidence "user correction"
step "share (clone B)" "$work/b" am share
step "refresh (clone A)" "$work/a" am refresh
expect "A received B's note" sh -c "cd '$work/a' && am recall commits | grep -q 'one logical change'"
expect "A's MEMORY.md links all three notes" test "$(grep -c '](<topics/' "$work/a/memory-bank/MEMORY.md")" -eq 3
step "share (nothing new)" "$work/a" am share
step "git pull after share and refresh (clone A)" "$work/a" git pull -q
expect "A clean and at origin/main after pull" sh -c "cd '$work/a' && test -z \"\$(git status --porcelain)\" && test \"\$(git rev-parse HEAD)\" = \"\$(git rev-parse origin/main)\""
step "refresh (diverged clone C)" "$work/c" am refresh
step "commit staged memory (clone C)" "$work/c" git commit -qm "Shared memory"
step "git pull --no-edit (clone C, merge)" "$work/c" git pull -q --no-edit
expect "C merged cleanly and keeps its own work" sh -c "cd '$work/c' && test -z \"\$(git status --porcelain)\" && test -f work.txt && am recall commits | grep -q 'one logical change'"

step "hook claude" "$work/a" sh -c "printf '{\"cwd\":\"$work/a\",\"hook_event_name\":\"SessionStart\"}' | am hook session-start --client claude"
expect "hook emits additionalContext" sh -c "printf '{\"cwd\":\"$work/a\"}' | am hook session-start --client claude | grep -q additionalContext"
step "hook omp" "$work/a" am hook session-start --client omp
step "hook outside a repo" "$work" am hook session-start --client codex

step status "$work/a" am status
step doctor "$work/a" am doctor
step "uninstall" "$work" am uninstall
expect "commands removed" test ! -e "$HOME/.local/bin/am"
expect "Claude settings restored" sh -c "! grep -q agent-memory '$HOME/.claude/settings.json' && grep -q '\"model\": \"opus\"' '$HOME/.claude/settings.json'"
expect "codex hooks.json removed (was absent)" test ! -e "$HOME/.codex/hooks.json"
expect "omp extension removed" test ! -e "$HOME/.omp/agent/extensions/agent-memory.ts"
expect "banks untouched" test -f "$work/a/memory-bank/MEMORY.md"

{
  printf '| Step | Command | Time | Result |\n| --- | --- | --- | --- |\n'
  printf '%s' "$table"
  printf '\n%s failure(s) on %s %s\n' "$failures" "$(uname -s)" "$(uname -m)"
} > "$results"
exit $((failures > 0))
