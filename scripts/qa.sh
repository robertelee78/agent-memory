#!/usr/bin/env bash
# Real-use QA: drive the actual `am` the way a user would, in a temporary
# HOME, and record each command's result and wall time.
#
#   scripts/qa.sh CHECKOUT [RESULTS_FILE]
#
# CHECKOUT is an agent-memory checkout with a built target/release/am.
# Covers: install (development source, default presets with omp present
# beside a user's own omp extension) over a linked Claude settings file and
# interrupted-install leftovers, init, remember, recall, share and refresh
# between clones of a bare remote, plain git pull after refresh and share
# with git set to merge and to rebase on pull (also pull.ff=only, an
# uncommitted code edit, a branch without upstream), refusals that protect data
# (staged-only memory, links inside the bank, an odd bank path), subcommand
# help, an existing bank without every context file, the session-start hook,
# status, doctor, uninstall, the takeover of a small offline v0.0.14 install,
# install on a fresh machine and again after `am uninstall` (the lean one and
# the v0.0.14 one), omp installed after agent-memory and a relative
# PI_CODING_AGENT_DIR. Needs bash 5
# (EPOCHREALTIME) and git.
set -euo pipefail
[ -n "${EPOCHREALTIME:-}" ] || { echo "qa.sh needs bash 5 or newer (macOS: brew install bash)" >&2; exit 2; }

src=$(cd "${1:?usage: qa.sh CHECKOUT [RESULTS_FILE]}" && pwd -P)
results=${2:-/dev/stdout}
work=$(mktemp -d "${TMPDIR:-/tmp}/am-qa.XXXXXX")
work=$(cd "$work" && pwd -P)
trap 'rm -rf "$work"' EXIT

export HOME="$work/home"
# Inherited omp locations would point default omp detection outside the
# temporary HOME; the scenarios that need them set them explicitly.
unset XDG_STATE_HOME XDG_DATA_HOME XDG_CONFIG_HOME CODEX_HOME CLAUDE_CONFIG_DIR GIT_DIR GIT_WORK_TREE PI_CODING_AGENT_DIR PI_CONFIG_DIR
mkdir -p "$HOME/.claude" "$HOME/.omp/agent/extensions" "$HOME/dotfiles"
# omp is on this machine, with an extension of the user's own.
printf '// my own omp extension\n' > "$HOME/.omp/agent/extensions/vox.ts"
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
step "install --dry-run" "$work" "$src/target/release/am" install --dry-run --development-source "$src"
expect "dry-run deletes nothing" sh -c "test -e '$stale' && test -e '$lookalike' && test -e '$state/leftovers'"
step "install --dry-run --presets without omp" "$work" "$src/target/release/am" install --dry-run --development-source "$src" --presets agents,codex,claude,opencode
expect "presets without omp leave omp out" sh -c "! grep -q '\.omp' '$work/out'"
step install "$work" "$src/target/release/am" install --development-source "$src"
# From here on `am` is the installed command on PATH.
expect "am on PATH is the installed link" test -L "$HOME/.local/bin/am"
expect "Claude SessionStart hook registered" grep -q 'hook session-start --client claude' "$HOME/.claude/settings.json"
expect "linked Claude settings stay a link" test -L "$HOME/.claude/settings.json"
expect "existing Claude setting kept" grep -q '"model": "opus"' "$HOME/dotfiles/claude-settings.json"
expect "Codex SessionStart hook registered" grep -q 'hook session-start --client codex' "$HOME/.codex/hooks.json"
expect "omp extension written" test -f "$HOME/.omp/agent/extensions/agent-memory.ts"
expect "the user's own omp extension untouched" grep -q 'my own omp extension' "$HOME/.omp/agent/extensions/vox.ts"
expect "omp skill and activation block set up" sh -c "test -f '$HOME/.omp/agent/skills/agent-memory/SKILL.md' && grep -q 'agent-memory:activation' '$HOME/.omp/agent/AGENTS.md'"
expect "status lists omp with Claude and Codex" sh -c "am status | grep 'Session-start hooks' | grep 'claude ok' | grep 'codex ok' | grep -q 'omp ok'"
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
expect "remote main has the bank" sh -c "git -C '$work/remote.git' ls-tree -r --name-only main | grep -q memory-bank/MEMORY.md"
expect "A's branch moved to the memory-only share commit: nothing staged" sh -c "cd '$work/a' && test -z \"\$(git status --porcelain)\" && test \"\$(git rev-parse HEAD)\" = \"\$(git rev-parse origin/main)\""
expect "A's code is untouched (only memory-bank/ differs from before)" sh -c "test -z \"\$(git -C '$work/a' diff --name-only \"\$(git -C '$work/b' rev-parse HEAD)\" HEAD | grep -v '^memory-bank/')\""

step refresh "$work/b" am refresh
expect "B moved over the memory-only commit: clean right after refresh" sh -c "cd '$work/b' && test -z \"\$(git status --porcelain)\" && grep -q 'moved to the shared memory commit' '$work/out'"
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
expect "C got one commit that changes only memory-bank/" sh -c "cd '$work/c' && test \"\$(git log -1 --format=%s)\" = 'Shared memory' && test -z \"\$(git diff --name-only HEAD~1 HEAD | grep -v '^memory-bank/')\" && test -z \"\$(git status --porcelain)\""
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

# Plain `git pull` after refresh and share, with git set to rebase (clone R)
# and to merge (clone M) on pull; T is a teammate. Only memory between HEAD
# and the shared commit: the branch moves there. Code too: R gets one
# memory-only commit that its rebasing pull drops; M keeps the memory staged
# for its fast-forward pull.
git init -q --bare "$work/pull.git"
git clone -q "$work/pull.git" "$work/t" 2>/dev/null
(cd "$work/t" && echo code > code.txt && git add code.txt && git commit -qm init && git push -q origin main && am init > /dev/null && am share > /dev/null)
git clone -q "$work/pull.git" "$work/r"
git -C "$work/r" config pull.rebase true
teammate_note() { (cd "$work/t" && am remember --kind fact --title "$1" --body "$1, from the teammate." > /dev/null && am share > /dev/null); }
teammate_code() { (cd "$work/t" && git pull -q --ff-only && echo "$1" >> code.txt && git commit -qam "$1" && git push -q origin main); }
at_origin() { (cd "$1" && test -z "$(git status --porcelain)" && test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)"); }
teammate_note "Rebase one"
step "refresh (rebase pull, memory-only behind)" "$work/r" am refresh
expect "R moved over the memory-only commit, nothing staged" at_origin "$work/r"
step "git pull (rebase) after refresh" "$work/r" git pull -q
(cd "$work/r" && am remember --kind fact --title "Rebase mine" --body "R's own note." > /dev/null)
step "share (rebase pull)" "$work/r" am share
expect "R clean at the shared commit after share" at_origin "$work/r"
step "git pull (rebase) after share" "$work/r" git pull -q
teammate_code "teammate code 1"; teammate_note "Rebase two"; teammate_note "Rebase three"
step "refresh (rebase pull, behind code and memory)" "$work/r" am refresh
expect "refresh made one memory-only commit and says so" sh -c "grep -q 'committed on main as one commit that changes only memory-bank/' '$work/out' && cd '$work/r' && test -z \"\$(git status --porcelain)\" && test -z \"\$(git diff --name-only HEAD~1 HEAD | grep -v '^memory-bank/')\""
step "git pull (rebase) after refresh, behind" "$work/r" git pull -q
expect "the pull dropped that commit: R clean at origin/main" at_origin "$work/r"
expect "R has the teammate's code and notes" sh -c "cd '$work/r' && grep -q 'teammate code 1' code.txt && am recall 'Rebase three' | grep -q 'from the teammate'"
teammate_code "teammate code 2"; teammate_note "Rebase four"
(cd "$work/r" && am remember --kind fact --title "Rebase five" --body "R's note while behind." > /dev/null)
step "share (rebase pull, behind code)" "$work/r" am share
step "git pull (rebase) after share, behind" "$work/r" git pull -q
expect "R clean at origin/main after share and pull" at_origin "$work/r"
teammate_code "teammate code 3"; teammate_note "Rebase six"
step "refresh (rebase pull, behind again)" "$work/r" am refresh
teammate_note "Rebase seven"
step "refresh again (more memory shared since)" "$work/r" am refresh
expect "still one memory-only commit ahead" test "$(git -C "$work/r" rev-list --count origin/main..HEAD)" -eq 1
step "git pull (rebase) after refreshing twice" "$work/r" git pull -q
expect "R clean at origin/main with the later note" sh -c "cd '$work/r' && test -z \"\$(git status --porcelain)\" && am recall 'Rebase seven' | grep -q 'from the teammate'"
(cd "$work/r" && printf '\nUnshared local edit.\n' >> memory-bank/activeContext.md)
teammate_code "teammate code 4"; teammate_note "Rebase eight"
step "refresh with unshared edits (rebase pull)" "$work/r" am refresh
expect "refresh says the uncommitted edits stop a rebasing pull" grep -q 'You have uncommitted changes (memory edits not shared yet: am share shares them); git will ask you to commit or stash them before a rebasing pull.' "$work/out"
expect "the unshared edit is kept" grep -q 'Unshared local edit' "$work/r/memory-bank/activeContext.md"
step "am share, then git pull (rebase)" "$work/r" sh -c 'am share > /dev/null && git pull -q'
expect "R clean at origin/main, the edit shared" sh -c "cd '$work/r' && test -z \"\$(git status --porcelain)\" && git show origin/main:memory-bank/activeContext.md | grep -q 'Unshared local edit'"
git clone -q "$work/pull.git" "$work/m"
teammate_code "teammate code 5"; teammate_note "Merge one"
step "refresh (merge pull, behind code and memory)" "$work/m" am refresh
expect "M keeps the memory staged and says git pull works" grep -q 'The shared memory is staged. `git pull` works as usual.' "$work/out"
step "git pull (merge) after refresh, behind" "$work/m" git pull -q
expect "M clean at origin/main" at_origin "$work/m"
teammate_code "teammate code 6"
(cd "$work/m" && am remember --kind fact --title "Merge mine" --body "M's own note." > /dev/null)
step "share (merge pull, behind code)" "$work/m" am share
step "git pull (merge) after share, behind" "$work/m" git pull -q
expect "M clean at origin/main after share and pull" at_origin "$work/m"
# pull.ff=only with a rebasing pull: a plain `git pull` refuses the memory-only
# commit, so am says to use `git pull --rebase`; with a merging pull the
# memory stays staged and the fast-forward takes it.
git clone -q "$work/pull.git" "$work/f"
git -C "$work/f" config pull.rebase true
git -C "$work/f" config pull.ff only
teammate_code "teammate code 7"; teammate_note "FF one"
step "refresh (rebase pull, pull.ff=only, behind)" "$work/f" am refresh
expect "refresh says to pull with git pull --rebase" grep -q 'Pull with `git pull --rebase`: with pull.ff=only a plain `git pull` refuses' "$work/out"
refuse "plain git pull (rebase, pull.ff=only) refuses, as said" "$work/f" git pull -q
step "git pull --rebase (pull.ff=only)" "$work/f" git pull -q --rebase
expect "F clean at origin/main" at_origin "$work/f"
git -C "$work/f" config pull.rebase false
teammate_code "teammate code 8"; teammate_note "FF two"
step "refresh (merge pull, pull.ff=only, behind)" "$work/f" am refresh
expect "F keeps the memory staged and says git pull works" grep -q 'The shared memory is staged. `git pull` works as usual.' "$work/out"
step "git pull (merge, pull.ff=only)" "$work/f" git pull -q
expect "F clean at origin/main (merge, pull.ff=only)" at_origin "$work/f"
# pull.ff=only judges the branch's own upstream, not the shared branch: a
# feature branch that diverged from origin/feature (merge pull) gets a memory
# commit and the `git pull --no-ff` it needs; one still level with its upstream
# (rebase pull) keeps working with a plain `git pull` after the memory commit.
git clone -q "$work/pull.git" "$work/g"
git -C "$work/g" config pull.rebase false
git -C "$work/g" config pull.ff only
(cd "$work/g" && git checkout -q -b feature && echo g1 > g1.txt && git add g1.txt && git commit -qm g1 && git push -q -u origin feature 2>/dev/null)
git clone -q "$work/pull.git" "$work/g2"
(cd "$work/g2" && git checkout -q feature && echo g2 > g2.txt && git add g2.txt && git commit -qm g2 && git push -q origin feature)
(cd "$work/g" && echo g3 > g3.txt && git add g3.txt && git commit -qm g3 && git fetch -q)
teammate_note "Feature one"
step "refresh (feature diverged from its upstream, merge pull, pull.ff=only)" "$work/g" am refresh
expect "refresh says to pull with git pull --no-ff" grep -q 'Pull with `git pull --no-ff`: with pull.ff=only a plain `git pull` refuses, as this branch and its upstream have both moved.' "$work/out"
refuse "plain git pull (diverged feature, pull.ff=only) refuses, as said" "$work/g" git pull -q
step "git pull --no-ff (diverged feature, pull.ff=only)" "$work/g" git pull -q --no-ff --no-edit
expect "G clean, with its upstream merged and the note" sh -c "cd '$work/g' && test -z \"\$(git status --porcelain)\" && git merge-base --is-ancestor origin/feature HEAD && am recall 'Feature one' | grep -q 'from the teammate'"
git clone -q "$work/pull.git" "$work/h"
git -C "$work/h" config pull.rebase true
git -C "$work/h" config pull.ff only
(cd "$work/h" && git checkout -q -b feature-h && echo h > h.txt && git add h.txt && git commit -qm h && git push -q -u origin feature-h 2>/dev/null)
teammate_note "Feature two"
step "refresh (feature level with its upstream, rebase pull, pull.ff=only)" "$work/h" am refresh
expect "refresh made the memory commit and says git pull works" sh -c "grep -q 'committed on feature-h' '$work/out' && grep -q '\`git pull\` works as usual.' '$work/out'"
step "git pull (feature ahead of its upstream, rebase, pull.ff=only)" "$work/h" git pull -q
expect "H clean" sh -c "cd '$work/h' && test -z \"\$(git status --porcelain)\""
# A code edit stops a rebasing pull whatever am does; am says so.
(cd "$work/r" && git pull -q && echo v1 > mine.txt && git add mine.txt && git commit -qm mine && git push -q origin main && echo "my code edit" >> mine.txt)
teammate_code "teammate code 9"; teammate_note "Code edit one"
step "refresh with an uncommitted code edit (rebase pull)" "$work/r" am refresh
expect "refresh says git will ask to commit or stash" grep -q 'You have uncommitted changes; git will ask you to commit or stash them before a rebasing pull.' "$work/out"
refuse "plain git pull with the code edit (rebase) refuses, as said" "$work/r" git pull -q
step "git stash, git pull, git stash pop (rebase)" "$work/r" sh -c 'git stash -q && git pull -q && git stash pop -q'
expect "R has origin/main and keeps the code edit" sh -c "cd '$work/r' && test \"\$(git rev-parse HEAD)\" = \"\$(git rev-parse origin/main)\" && grep -q 'my code edit' mine.txt && test -z \"\$(git diff --cached --name-only)\""
(cd "$work/r" && git checkout -q mine.txt)
# A branch without an upstream: nothing to pull, and am says so.
(cd "$work/r" && git checkout -q -b local-only && echo local > local.txt && git add local.txt && git commit -qm local)
teammate_note "No upstream one"
step "refresh on a branch without upstream" "$work/r" am refresh
expect "refresh says there is no upstream to pull" grep -q 'This branch has no upstream branch, so there is nothing for `git pull` to pull.' "$work/out"

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
expect "the user's own omp extension kept" grep -q 'my own omp extension' "$HOME/.omp/agent/extensions/vox.ts"
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
expect "takeover: no omp on this machine, so no omp folders" test ! -e "$HOME/.omp"
step "uninstall after takeover" "$work" "$src/target/release/am" uninstall
expect "takeover: CLAUDE.md keeps only the user's text" sh -c "grep -q 'My notes' '$C/CLAUDE.md' && ! grep -q agent-memory '$C/CLAUDE.md'"
expect "takeover: settings.json keeps only the user's setting" sh -c "! grep -q -e session-start -e python3.sh '$C/settings.json' && grep -q '\"model\": \"opus\"' '$C/settings.json'"

# --------------------------------------------------------------- fresh machine, reinstall, omp later
# A machine that never had agent-memory (and no omp), the same machine after
# `am uninstall`, then omp installed later, beside an extension of its own.
export HOME="$work/fresh-home"
mkdir -p "$HOME"
export PATH="$HOME/.local/bin:$PATH"
step "install on a machine that never had agent-memory" "$work" "$src/target/release/am" install --development-source "$src"
expect "no omp on the machine: no omp folders created" test ! -e "$HOME/.omp"
step "status after a fresh install" "$work" am status
step "uninstall (fresh machine)" "$work" am uninstall
step "install again after am uninstall" "$work" "$src/target/release/am" install --development-source "$src"
step "doctor after installing again" "$work" am doctor
mkdir -p "$HOME/.omp/agent/extensions"
printf '// my own omp extension\n' > "$HOME/.omp/agent/extensions/vox.ts"
step "status once omp is installed later" "$work" am status
expect "status says omp is not set up yet and to run am update" grep -q 'omp is installed .* but agent-memory is not set up for it yet. Run `am update`.' "$work/out"
step "update sets up omp" "$work" am update
expect "omp extension written by update" test -f "$HOME/.omp/agent/extensions/agent-memory.ts"
expect "the user's own omp extension untouched by update" grep -q 'my own omp extension' "$HOME/.omp/agent/extensions/vox.ts"
step "status lists omp after update" "$work" am status
expect "status lists omp with Claude and Codex after update" sh -c "grep 'Session-start hooks' '$work/out' | grep 'claude ok' | grep 'codex ok' | grep -q 'omp ok'"
step "update again (nothing to do)" "$work" am update
mkdir -p "$HOME/pi-agent"
step "install --dry-run with PI_CODING_AGENT_DIR" "$work" env PI_CODING_AGENT_DIR="$HOME/pi-agent" am install --dry-run
expect "PI_CODING_AGENT_DIR is the omp agent directory" grep -q "$HOME/pi-agent/extensions/agent-memory.ts" "$work/out"
# A relative PI_CODING_AGENT_DIR is resolved against the current directory,
# as omp does; it never blocks an install that leaves omp out.
mkdir -p "$HOME/project" "$HOME/shared-agent"
step "install --dry-run with a relative PI_CODING_AGENT_DIR" "$HOME/project" env PI_CODING_AGENT_DIR=../shared-agent am install --dry-run
expect "../shared-agent resolves like omp does" grep -q "$HOME/shared-agent/extensions/agent-memory.ts" "$work/out"
step "install --dry-run --presets without omp, relative PI_CODING_AGENT_DIR" "$HOME/project" env PI_CODING_AGENT_DIR=../../../missing-agent am install --dry-run --presets agents,codex,claude,opencode
step "uninstall (omp set up)" "$work" am uninstall
expect "uninstall removed only agent-memory's omp files" sh -c "test ! -e '$HOME/.omp/agent/extensions/agent-memory.ts' && test ! -e '$HOME/.omp/agent/skills/agent-memory' && test ! -e '$HOME/.omp/agent/AGENTS.md' && grep -q 'my own omp extension' '$HOME/.omp/agent/extensions/vox.ts'"

# --------------------------------------------------------------- after the v0.0.14 am uninstall
# A small offline capture of what the v0.0.14 `am uninstall` leaves: its
# files removed, the user's own text and settings restored, and the receipt
# kept, marked "uninstalled": true (identities null for removed targets).
export HOME="$work/uninstalled-home"
export PATH="$HOME/.local/bin:$PATH"
O="$HOME/.local/share/agent-memory/source"
mkdir -p "$HOME/.claude" "$HOME/.codex" "$HOME/.agents/skills" "$HOME/.local/state/agent-memory/install"
printf '# My notes\n' > "$HOME/.claude/CLAUDE.md"
printf '{\n  "model": "opus"\n}\n' > "$HOME/.claude/settings.json"
printf '{"hooks": {}}\n' > "$HOME/.codex/hooks.json"
file_id() { printf '{"hash": "%s", "kind": "file", "mode": 420}' "$(sha "$1")"; }
link_id='{"kind": "link", "target": "'"$O"'/skills/agent-memory"}'
cat > "$HOME/.local/state/agent-memory/install/receipt.json" <<EOF
{
  "schema": "agent-memory/install-receipt-v1",
  "uninstalled": true, "uninstalled_at": "2026-11-15T21:38:32.205435+00:00",
  "installed_at": "2026-11-15T21:38:32.205435+00:00", "installation_id": "95e00cfe89db487a93fd694e43b0292c",
  "bin_dir": "$HOME/.local/bin", "mode": "link", "presets": "all", "no_guidance": false, "custom_targets": [],
  "created_directories": ["$HOME/.local/share/agent-memory"],
  "source": {"kind": "release", "path": "$O", "pack": "$O/skills/agent-memory", "version": "0.0.14"},
  "targets": [
    {"kind": "link", "path": "$HOME/.agents/skills/agent-memory", "identity": null, "pre_change_identity": $link_id},
    {"kind": "link", "path": "$HOME/.codex/skills/agent-memory", "identity": null, "pre_change_identity": $link_id},
    {"kind": "hook-config", "path": "$HOME/.codex/hooks.json", "identity": $(file_id "$HOME/.codex/hooks.json"), "pre_change_identity": $(file_id "$HOME/.codex/hooks.json")},
    {"kind": "link", "path": "$HOME/.claude/skills/agent-memory", "identity": null, "pre_change_identity": $link_id},
    {"kind": "activation", "path": "$HOME/.claude/CLAUDE.md", "identity": $(file_id "$HOME/.claude/CLAUDE.md"), "pre_change_identity": $(file_id "$HOME/.claude/CLAUDE.md")},
    {"kind": "hook-config", "path": "$HOME/.claude/settings.json", "identity": $(file_id "$HOME/.claude/settings.json"), "pre_change_identity": $(file_id "$HOME/.claude/settings.json")},
    {"kind": "wrapper", "path": "$HOME/.local/bin/am", "identity": null, "pre_change_identity": {"hash": "da0971d21d81d6ae03cd4dbd6e44489f54ebf044f0d71c7c7363a0cb196bdbb5", "kind": "file", "mode": 493}},
    {"kind": "wrapper", "path": "$HOME/.local/bin/agent-memory", "identity": null, "pre_change_identity": {"hash": "da0971d21d81d6ae03cd4dbd6e44489f54ebf044f0d71c7c7363a0cb196bdbb5", "kind": "file", "mode": 493}},
    {"kind": "source", "path": "$O", "identity": null, "pre_change_identity": {"kind": "directory"}}
  ]
}
EOF
# A file at an install location that agent-memory did not put there.
mkdir -p "$HOME/.agents/skills/agent-memory"
printf "the user's own skill\n" > "$HOME/.agents/skills/agent-memory/SKILL.md"
refuse "install over a file agent-memory did not put there" "$work" "$src/target/release/am" install --development-source "$src"
expect "the user's file is kept" grep -q "the user's own skill" "$HOME/.agents/skills/agent-memory/SKILL.md"
mv "$HOME/.agents/skills/agent-memory" "$HOME/my-skill"
step "install after the v0.0.14 am uninstall" "$work" "$src/target/release/am" install --development-source "$src"
expect "a fresh receipt replaced the uninstalled one" sh -c "grep -q 'install-receipt-v4' '$HOME/.local/state/agent-memory/install/receipt.json' && ! grep -q '\"uninstalled\"' '$HOME/.local/state/agent-memory/install/receipt.json'"
expect "the user's text and settings kept" sh -c "grep -q 'My notes' '$HOME/.claude/CLAUDE.md' && grep -q '\"model\": \"opus\"' '$HOME/.claude/settings.json'"
step "doctor after installing over the v0.0.14 uninstall" "$work" am doctor

{
  printf '| Step | Command | Time | Result |\n| --- | --- | --- | --- |\n'
  printf '%s' "$table"
  printf '\n%s failure(s) on %s %s\n' "$failures" "$(uname -s)" "$(uname -m)"
} > "$results"
exit $((failures > 0))
