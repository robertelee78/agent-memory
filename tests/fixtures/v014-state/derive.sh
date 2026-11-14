#!/bin/sh
# Derive the repository's Python-era (v0.0.14) state fixtures from the genuine
# captures kept outside the repository (Main-only data; see its README.md).
#
# Usage: tests/fixtures/v014-state/derive.sh CAPTURES_DIR
#   CAPTURES_DIR is .../research/rust-cutover/v014-state/captures
#
# Requires: POSIX sh, tar, gzip, jq, sed, find, cmp. Executes nothing from the capture.
# For each scenario it keeps ONLY lifecycle state files (receipt, journal,
# owner, lock, the two command wrappers, client hook configuration and
# activation Markdown) with the capture's world root replaced by @WORLD@, and
# records, instead of any pack bytes:
#   modes.txt  "<octal mode> <relative path>" for every kept file
#   links.txt  "<relative link path> -> <target with @WORLD@>" for owned links
#   trees.txt  "<kind> <relative path>" for owned copy/source directories,
#              which the tests materialize as opaque bytes and whose receipt
#              identities they recompute (design "Python-era adoption").
#   identities.json  genuine captured identities of files unchanged by relocation
# The pack, the managed source clone and every .py file stay outside the repository.
set -eu
captures=${1:?usage: derive.sh CAPTURES_DIR}
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/am-v014-derive.XXXXXXXX")
trap 'rm -rf "$scratch"' EXIT
umask 077
for scenario in all-copy all-link claude-opencode-copy codex-copy-pending-source; do
  capture="$captures/$scenario"
  root=$(jq -r '.world_root' "$capture/manifest.json")
  mkdir -p "$scratch/$scenario"
  tar -xzf "$capture/world.tar.gz" -C "$scratch/$scenario"
  world="$scratch/$scenario/world"
  out="$here/$scenario"
  rm -rf "$out"
  mkdir -p "$out"
  : > "$out/modes.txt"
  : > "$out/links.txt"
  : > "$out/trees.txt"
  printf '{}\n' > "$out/identities.json"
  for relative in \
      state/agent-memory/install/receipt.json \
      state/agent-memory/install/journal.json \
      state/agent-memory/install/owner.json \
      state/agent-memory/install/lock \
      home/.local/bin/am \
      home/.local/bin/agent-memory \
      claude/settings.json \
      claude/CLAUDE.md \
      codex/hooks.json \
      codex/AGENTS.md \
      config/opencode/AGENTS.md; do
    if [ -f "$world/$relative" ] && [ ! -L "$world/$relative" ]; then
      mkdir -p "$out/$(dirname "$relative")"
      sed "s#$root#@WORLD@#g" "$world/$relative" > "$out/$relative"
      mode=$(jq -r --arg p "$relative" '.entries[] | select(.path == $p) | .mode' "$capture/manifest.json")
      printf '%s %s\n' "$mode" "$relative" >> "$out/modes.txt"
      # Preserve the genuine Python identity whenever relocation changed no bytes.
      if cmp -s "$world/$relative" "$out/$relative"; then
        jq -S --arg path "$root/$relative" --arg relative "$relative" \
          --slurpfile identities "$out/identities.json" \
          '$identities[0] + ([.targets[] | select(.path == $path) |
            {($relative): .identity}] | add // {})' \
          "$world/state/agent-memory/install/receipt.json" > "$scratch/identities.json"
        mv "$scratch/identities.json" "$out/identities.json"
      fi
    fi
  done
  receipt="$world/state/agent-memory/install/receipt.json"
  jq -r '.targets[] | select(.identity.kind == "link") | .path + " -> " + .identity.target' "$receipt" \
    | sed "s#$root/##; s#$root#@WORLD@#g" >> "$out/links.txt"
  jq -r '.targets[] | select(.identity.kind == "directory") | .kind + " " + .path' "$receipt" \
    | sed "s#$root/##" >> "$out/trees.txt"
done
if find "$here" -name '*.py' | grep -q .; then
  echo 'derive.sh: a .py file reached the fixtures' >&2
  exit 1
fi
