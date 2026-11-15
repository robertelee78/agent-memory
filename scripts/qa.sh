#!/usr/bin/env bash
# Real-use QA: drive the actual `am` the way a user would, in a temporary
# HOME, and record each command's result and wall time.
#
#   scripts/qa.sh CHECKOUT [RESULTS_FILE]
#
# CHECKOUT is an agent-memory checkout with a built target/release/am.
# Covers: install (development source) over a linked Claude settings file
# and interrupted-install leftovers, init, remember, recall, share and
# refresh between clones of a bare remote, plain git pull, refusals that
# protect data (staged-only memory, links inside the bank, an odd bank
# path), subcommand help, an existing bank without every context file,
# the session-start hook, status, doctor, uninstall, and the takeover of a
# small offline v0.0.14 install. Needs bash 5 (EPOCHREALTIME) and git.
set -euo pipefail
[ -n "${EPOCHREALTIME:-}" ] || { echo "qa.sh needs bash 5 or newer (macOS: brew install bash)" >&2; exit 2; }

src=$(cd "${1:?usage: qa.sh CHECKOUT [RESULTS_FILE]}" && pwd -P)
results=${2:-/dev/stdout}
work=$(mktemp -d "${TMPDIR:-/tmp}/am-qa.XXXXXX")
work=$(cd "$work" && pwd -P)
trap 'rm -rf "$work"' EXIT

export HOME="$work/home"
unset XDG_STATE_HOME XDG_DATA_HOME XDG_CONFIG_HOME CODEX_HOME CLAUDE_CONFIG_DIR GIT_DIR GIT_WORK_TREE
mkdir -p "$HOME/.claude" "$HOME/.omp/agent" "$HOME/dotfiles"
export PATH="$HOME/.local/bin:$PATH"
git config --global user.name "QA User"
git config --global user.email qa@example.com
git config --global init.defaultBranch main
git config --global pull.rebase false
# An existing user setting the installer must preserve, kept in a dotfiles
# repository and linked into place.
printf '{\n  "model": "opus"\n}\n' > "$HOME/dotfiles/claude-settings.json"
ln -s ../dotfiles/claude-settings.json "$HOME/.claude/settings.json"
sha() { if command -v sha256sum > /dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi | cut -d' ' -f1; }
# What a killed install leaves, as its leftovers record proves (each entry
# with the SHA-256 of the bytes am wrote): a CLAUDE.md it created holding
# only the activation block, and an abandoned temporary.
state="$HOME/.local/state/agent-memory/install"
mkdir -p "$state" "$HOME/.codex"
cp "$src/skills/agent-memory/templates/activation-markdown.txt" "$HOME/.claude/CLAUDE.md"
stale="$HOME/.claude/.CLAUDE.md.am-0b6c1d2e-3f40-4a51-8b62-7c83d94ea5f6"
printf 'bytes am wrote\n' > "$stale"
# A recorded temporary path that now holds a user's different file.
replaced="$HOME/.codex/.AGENTS.md.am-4fa05162-7384-4e95-8fa6-b07182930a41"
printf "the user's own text\n" > "$replaced"
# A runtime directory the killed run had moved aside, which install must put
# back first; and a recorded entry that is not am's (a user's file outside any
# target directory), which am must leave alone and doctor must report.
data="$HOME/.local/share/agent-memory"
aside="$data/.runtime.am-old-2d8e3f40-5162-4c73-8d84-9ea5fb607182"
mkdir -p "$aside" && printf 'previous runtime\n' > "$aside/marker"
mkdir -p "$HOME/notes"
foreign="$HOME/notes/.todo.md.am-3e9f4051-6273-4d84-9e95-af6b07182930"
printf 'my todo draft\n' > "$foreign"
{
  printf 'temp\t%s\t%s\n' "$stale" "$(sha "$stale")"
  printf 'created\t%s\t%s\n' "$HOME/.claude/CLAUDE.md" "$(sha "$HOME/.claude/CLAUDE.md")"
  printf 'temp\t%s\t%s\n' "$replaced" "$(printf 'bytes am meant to write\n' | sha)"
  printf 'aside\t%s\t%s\n' "$data/runtime" "$aside"
  printf 'temp\t%s\t%s\n' "$foreign" "$(sha "$foreign")"
} > "$state/leftovers"
# A user's file that merely looks like an am temporary: never am's to delete.
lookalike="$HOME/.claude/.settings.json.am-1c7d2e3f-4051-4b62-9c73-8d94ea5fb607"
printf '{"my": "recovery backup"}\n' > "$lookalike"
# Pre-existing files with no real content: uninstall must keep them.
mkdir -p "$HOME/.codex" "$HOME/.config/opencode"
printf '{"permissions": {"allow": []}}\n' > "$HOME/.codex/hooks.json"
printf '\n' > "$HOME/.config/opencode/AGENTS.md"

table=""
failures=0
run_step() { # run_step WANT LABEL DIR COMMAND... (WANT: ok or fail)
  local want=$1 label=$2 dir=$3
  shift 3
  local start=$EPOCHREALTIME status=0
  (cd "$dir" && "$@") > "$work/out" 2>&1 || status=$?
  local ms=$(( (${EPOCHREALTIME/./} - ${start/./}) / 1000 ))
  local verdict=pass
  if [ "$want" = ok ] && [ "$status" -ne 0 ]; then
    verdict="FAIL ($status)"
  elif [ "$want" = fail ] && [ "$status" -eq 0 ]; then
    verdict="FAIL (should have refused)"
  elif [ "$want" = fail ]; then
    verdict="pass (refused)"
  fi
  [ "${verdict#pass}" != "$verdict" ] || failures=$((failures + 1))
  local shown=${*//$'\n'/\\n}
  table+=$(printf '| %s | `%s` | %s ms | %s |' "$label" "$shown" "$ms" "$verdict")$'\n'
  printf '== %s (%s ms, %s)\n' "$label" "$ms" "$verdict" >&2
  sed 's/^/   /' "$work/out" >&2
}
step() { run_step ok "$@"; }
refuse() { run_step fail "$@"; }
expect() { # expect DESCRIPTION COMMAND...
  if "${@:2}" > /dev/null 2>&1; then
    table+="| check | $1 | | pass |"$'\n'
  else
    table+="| check | $1 | | FAIL |"$'\n'
    failures=$((failures + 1))
    printf '!! check failed: %s\n' "$1" >&2
  fi
}

# --------------------------------------------------------------- install
step "install --dry-run" "$work" "$src/target/release/am" install --dry-run --development-source "$src" --presets agents,codex,claude,opencode,omp --omp-agent-dir "$HOME/.omp/agent"
expect "dry-run deletes nothing" sh -c "test -e '$stale' && test -e '$lookalike' && test -e '$state/leftovers'"
step install "$work" "$src/target/release/am" install --development-source "$src" --presets agents,codex,claude,opencode,omp --omp-agent-dir "$HOME/.omp/agent"
# From here on `am` is the installed command on PATH.
expect "am on PATH is the installed link" test -L "$HOME/.local/bin/am"
expect "Claude SessionStart hook registered" grep -q 'hook session-start --client claude' "$HOME/.claude/settings.json"
expect "linked Claude settings stay a link" test -L "$HOME/.claude/settings.json"
expect "existing Claude setting kept" grep -q '"model": "opus"' "$HOME/dotfiles/claude-settings.json"
expect "Codex SessionStart hook registered" grep -q 'hook session-start --client codex' "$HOME/.codex/hooks.json"
expect "omp extension written" test -f "$HOME/.omp/agent/extensions/agent-memory.ts"
expect "skill linked for Claude" test -f "$HOME/.claude/skills/agent-memory/SKILL.md"
expect "recorded interrupted-install temporary removed" test ! -e "$stale"
expect "look-alike user file survives install" grep -q 'recovery backup' "$lookalike"
expect "doctor mentions the look-alike" sh -c "am doctor | grep -q 'looks like an am temporary'"
expect "moved-aside runtime put back before install" grep -q 'previous runtime' "$data/runtime/marker"
expect "unrecognized recorded entry left alone" grep -q 'my todo draft' "$foreign"
expect "doctor lists the entry am did not act on" sh -c "am doctor | grep -q '.todo.md.am-'"
expect "recorded temp path now holding a user's different file survives" grep -q "the user's own text" "$replaced"
expect "doctor lists the changed recorded temp" sh -c "am doctor | grep -q '.AGENTS.md.am-4fa05162'"
step "help (subcommand)" "$work" am remember --help
expect "am install -h prints its usage" sh -c "am install -h | grep -q 'Usage: am install'"

# --------------------------------------------------------------- memory
git init -q --bare "$work/remote.git"
git clone -q "$work/remote.git" "$work/a" 2>/dev/null
(cd "$work/a" && echo "# Demo" > README.md && git add README.md && git commit -qm init && git push -q origin main)
git clone -q "$work/remote.git" "$work/b"
git clone -q "$work/remote.git" "$work/c"
(cd "$work/c" && echo "local work" > work.txt && git add work.txt && git commit -qm "local work")

step init "$work/a" am init
step "init (existing bank)" "$work/a" am init
expect "init on an existing bank changes nothing" sh -c "cd '$work/a' && am init | grep -q 'already has a memory bank'"
step remember "$work/a" am remember --kind fix --title "Musl builds need rust-lld" \
  --body "Cross-building the Linux binaries on macOS fails with the platform linker; set CARGO_TARGET_<TRIPLE>_LINKER=rust-lld." \
  --evidence "QA run" --read-when "When cross-building release binaries"
step "remember duplicate" "$work/a" am remember --kind fix --title "Musl builds need rust-lld" --body "same lesson again"
expect "duplicate skipped" test "$(ls "$work/a/memory-bank/topics" | grep -c '\.md$')" -eq 1
step "remember (stdin JSON)" "$work/a" sh -c 'printf "%s" "{\"title\":\"Use git worktrees per session\",\"body\":\"Each agent session works in its own worktree.\",\"kind\":\"decision\",\"evidence\":[\"team agreement\"]}" | am remember'
expect "harness is unknown unless given" sh -c "grep -l 'worktrees per session' '$work/a'/memory-bank/topics/*.md | xargs grep -q '\"harness\": \"unknown\"'"
step "remember (internal heading)" "$work/a" am remember --kind procedure --title "Release steps" --body $'Tag first.\n# Then\nRun the release build.'
expect "text before an internal heading kept" sh -c "grep -l 'Release steps' '$work/a'/memory-bank/topics/*.md | xargs grep -q '^Tag first.'"
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
expect "A's MEMORY.md links all four notes" test "$(grep -c '](<topics/' "$work/a/memory-bank/MEMORY.md")" -eq 4
step "share (nothing new)" "$work/a" am share
step "git pull after share and refresh (clone A)" "$work/a" git pull -q
expect "A clean and at origin/main after pull" sh -c "cd '$work/a' && test -z \"\$(git status --porcelain)\" && test \"\$(git rev-parse HEAD)\" = \"\$(git rev-parse origin/main)\""
step "refresh (diverged clone C)" "$work/c" am refresh
step "commit staged memory (clone C)" "$work/c" git commit -qm "Shared memory"
step "git pull --no-edit (clone C, merge)" "$work/c" git pull -q --no-edit
expect "C merged cleanly and keeps its own work" sh -c "cd '$work/c' && test -z \"\$(git status --porcelain)\" && test -f work.txt && am recall commits | grep -q 'one logical change'"

# A teammate edits a MEMORY.md row; B's untouched copy takes the edit.
(cd "$work/a" && sed -i.bak 's/| When cross-building release binaries |/| Before any musl release build |/' memory-bank/MEMORY.md && rm memory-bank/MEMORY.md.bak)
step "share edited route row (clone A)" "$work/a" am share
(cd "$work/b" && am remember --kind fact --title "Local only" --body "B keeps this unshared." > /dev/null)
step "refresh (clone B)" "$work/b" am refresh
expect "B took the teammate's row edit and kept its own row" sh -c "grep -q 'Before any musl release build' '$work/b/memory-bank/MEMORY.md' && grep -q 'Local only' '$work/b/memory-bank/MEMORY.md'"

# Memory the user staged by hand and then edited again exists only in the index.
(cd "$work/b" && printf '# Progress\n\nStaged by hand.\n' > memory-bank/progress.md && git add memory-bank/progress.md && printf '# Progress\n\nEdited again.\n' > memory-bank/progress.md)
(cd "$work/a" && am remember --kind fact --title "Staging test" --body "Forces a refresh in B." > /dev/null && am share > /dev/null)
refuse "refresh with staged-only memory (clone B)" "$work/b" am refresh
expect "staged memory kept in the index" sh -c "cd '$work/b' && git show :memory-bank/progress.md | grep -q 'Staged by hand'"
(cd "$work/b" && git add memory-bank/progress.md)
step "refresh after git add (clone B)" "$work/b" am refresh

# A link inside the bank must not let refresh write into code.
git clone -q "$work/remote.git" "$work/d"
mkdir -p "$work/d/src"
(cd "$work/d" && rm -rf memory-bank/topics && ln -s ../src memory-bank/topics)
(cd "$work/a" && am remember --kind fact --title "Link test" --body "Arrives while D's topics is a link." > /dev/null && am share > /dev/null)
refuse "refresh through a linked topics dir (clone D)" "$work/d" am refresh
expect "nothing written into src/" sh -c "test -z \"\$(ls -A '$work/d/src')\""

# A bank directory named ':' shares only itself, never the code beside it.
git init -q --bare "$work/odd.git"
git clone -q "$work/odd.git" "$work/e" 2>/dev/null
(cd "$work/e" && echo code > code.txt && git add code.txt && git commit -qm code && git push -q origin main && git config agent-memory.bankroot ':' && echo changed > code.txt && echo new > new.txt)
step "init (bank named ':')" "$work/e" am init
step "share (bank named ':')" "$work/e" am share
expect "only the ':' bank was shared" sh -c "test -z \"\$(git -C '$work/odd.git' diff --name-only main~1 main | grep -v '^:/')\""

# A v0.0.14 bank has only some context files; doctor accepts it.
git init -q "$work/legacy"
(cd "$work/legacy" && am init > /dev/null && rm memory-bank/productContext.md memory-bank/systemPatterns.md memory-bank/techContext.md memory-bank/progress.md \
  && grep -v -e productContext -e systemPatterns -e techContext -e 'progress.md' memory-bank/MEMORY.md > m && mv m memory-bank/MEMORY.md)
step "doctor (bank without every context file)" "$work/legacy" am doctor

# --------------------------------------------------------------- hooks, health
step "hook claude" "$work/a" sh -c "printf '{\"cwd\":\"$work/a\",\"hook_event_name\":\"SessionStart\"}' | am hook session-start --client claude"
expect "hook emits additionalContext" sh -c "printf '{\"cwd\":\"$work/a\"}' | am hook session-start --client claude | grep -q additionalContext"
expect "hook asks before saving" sh -c "cd '$work/a' && am hook session-start --client omp | grep -q 'Save this to project memory?'"
step "hook omp" "$work/a" am hook session-start --client omp
step "hook outside a repo" "$work" am hook session-start --client codex

step status "$work/a" am status
step doctor "$work/a" am doctor
step "uninstall" "$work" am uninstall
expect "commands removed" test ! -e "$HOME/.local/bin/am"
expect "Claude settings restored through the link" sh -c "test -L '$HOME/.claude/settings.json' && ! grep -q session-start '$HOME/dotfiles/claude-settings.json' && grep -q '\"model\": \"opus\"' '$HOME/dotfiles/claude-settings.json'"
expect "CLAUDE.md an interrupted install created (recorded) removed" test ! -e "$HOME/.claude/CLAUDE.md"
expect "pre-existing empty-ish codex hooks.json kept" sh -c "grep -q permissions '$HOME/.codex/hooks.json' && ! grep -q session-start '$HOME/.codex/hooks.json'"
expect "pre-existing whitespace-only AGENTS.md kept" test -e "$HOME/.config/opencode/AGENTS.md"
expect "omp extension removed" test ! -e "$HOME/.omp/agent/extensions/agent-memory.ts"
expect "look-alike user file survives uninstall" grep -q 'recovery backup' "$lookalike"
expect "no am temporaries left (the user's look-alikes stay)" sh -c "test -z \"\$(find '$HOME' -name '.*.am-*' ! -path '$lookalike' ! -path '$foreign' ! -path '$replaced')\" && test -e '$foreign' && test -e '$replaced'"
expect "banks untouched" test -f "$work/a/memory-bank/MEMORY.md"

# --------------------------------------------------------------- v0.0.14 takeover
# A small offline capture of a v0.0.14 install's on-disk state: receipt-v1,
# its Python wrappers, skill link, activation block, prompt/Stop hook entries
# and source tree, with ~/.claude a link into a dotfiles checkout.
export HOME="$work/legacy-home"
D="$HOME/.local/share/agent-memory/source"
mkdir -p "$D/skills/agent-memory/hooks" "$HOME/.local/bin" "$HOME/dotfiles/claude/skills" "$HOME/.local/state/agent-memory/install"
ln -s dotfiles/claude "$HOME/.claude"
C="$HOME/dotfiles/claude"
printf 'legacy v0.0.14 pack\n' > "$D/skills/agent-memory/SKILL.md"
printf '#!/bin/sh\n' > "$D/skills/agent-memory/hooks/python3.sh"
chmod 700 "$D"; chmod 755 "$D/skills" "$D/skills/agent-memory" "$D/skills/agent-memory/hooks" "$D/skills/agent-memory/hooks/python3.sh"; chmod 644 "$D/skills/agent-memory/SKILL.md"
P="$D/skills/agent-memory"
ln -s "$P" "$C/skills/agent-memory"
printf '#!/bin/sh\nexec python3 %s/scripts/installer.py "$@"\n' "$P" > "$HOME/.local/bin/am"
cp "$HOME/.local/bin/am" "$HOME/.local/bin/agent-memory"; chmod 755 "$HOME/.local/bin/am" "$HOME/.local/bin/agent-memory"
block='<!-- agent-memory:activation:begin -->
When this repository contains memory-bank/MEMORY.md, or the user asks to initialize, resume, checkpoint, remember, correct or forget project memory, load the agent-memory skill and follow its selected operation.
<!-- agent-memory:activation:end -->'
printf '# My notes\n\n%s\n' "$block" > "$C/CLAUDE.md"
prompt="/bin/sh $P/hooks/python3.sh $P/hooks/turn_hook.py"
stop="/bin/sh $P/hooks/python3.sh $P/hooks/claude_stop.py"
cat > "$C/settings.json" <<EOF
{
  "model": "opus",
  "hooks": {
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "$prompt"}]}],
    "Stop": [{"hooks": [{"asyncRewake": true, "type": "command", "command": "$stop"}]}]
  }
}
EOF
block_hash=$(printf '%s' "$block" | sha)
wrapper_hash=$(sha "$HOME/.local/bin/am")
cat > "$HOME/.local/state/agent-memory/install/receipt.json" <<EOF
{
  "schema": "agent-memory/install-receipt-v1",
  "bin_dir": "$HOME/.local/bin", "mode": "link", "presets": "claude", "no_guidance": false, "custom_targets": [],
  "source": {"kind": "release", "path": "$D", "pack": "$P", "version": "0.0.14"},
  "targets": [
    {"kind": "source", "path": "$D", "identity": {"kind": "directory", "mode": 448, "hash": "b87a7aafa5e7f9cbd399060298246aa688ee7c3dc27ddb27d7e7d70cc88e33f1"}},
    {"kind": "link", "path": "$C/skills/agent-memory", "identity": {"kind": "link", "target": "$P"}},
    {"kind": "activation", "path": "$C/CLAUDE.md", "block_hash": "$block_hash", "originally_absent": false, "identity": {"kind": "file", "mode": 420, "hash": "$(sha "$C/CLAUDE.md")"}},
    {"kind": "hook-config", "path": "$C/settings.json", "originally_absent": false,
     "hook_entry": {"hooks": [{"type": "command", "command": "$prompt"}]},
     "hook_original_shape": {"event_present": false, "hooks_present": false},
     "stop_entry": {"hooks": [{"asyncRewake": true, "type": "command", "command": "$stop"}]},
     "stop_original_shape": {"event_present": false, "hooks_present": false}},
    {"kind": "wrapper", "path": "$HOME/.local/bin/am", "identity": {"kind": "file", "mode": 493, "hash": "$wrapper_hash"}},
    {"kind": "wrapper", "path": "$HOME/.local/bin/agent-memory", "identity": {"kind": "file", "mode": 493, "hash": "$wrapper_hash"}}
  ]
}
EOF
step "takeover of v0.0.14 (linked ~/.claude)" "$work" "$src/target/release/am" install --development-source "$src"
expect "takeover: Python prompt/Stop hooks replaced by SessionStart" sh -c "! grep -q python3.sh '$C/settings.json' && grep -q 'session-start --client claude' '$C/settings.json' && grep -q '\"model\": \"opus\"' '$C/settings.json'"
expect "takeover: wrappers replaced by links, old source removed" sh -c "test -L '$HOME/.local/bin/am' && test ! -e '$D'"
expect "takeover: ~/.claude is still a link" test -L "$HOME/.claude"
step "uninstall after takeover" "$work" "$src/target/release/am" uninstall
expect "takeover: CLAUDE.md keeps only the user's text" sh -c "grep -q 'My notes' '$C/CLAUDE.md' && ! grep -q agent-memory '$C/CLAUDE.md'"
expect "takeover: settings.json keeps only the user's setting" sh -c "! grep -q -e session-start -e python3.sh '$C/settings.json' && grep -q '\"model\": \"opus\"' '$C/settings.json'"

{
  printf '| Step | Command | Time | Result |\n| --- | --- | --- | --- |\n'
  printf '%s' "$table"
  printf '\n%s failure(s) on %s %s\n' "$failures" "$(uname -s)" "$(uname -m)"
} > "$results"
exit $((failures > 0))
