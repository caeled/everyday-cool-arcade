#!/usr/bin/env bash
# Assemble dist/ for the Everyday Cool Arcade from a checkout of the
# Everyday Cool collection.
#
#   tools/sync-dist.sh [path-to-collection]
#
# The collection path defaults to ../everyday-cool (a sibling checkout).
# dist/ is rebuilt from scratch every run:
#   dist/index.html        <- interface/index.html (the arcade shelf)
#   dist/games/<slug>/     <- the game file, its lesson file, and any
#                             support files, per tools/games.manifest
# Game and lesson files are copied as siblings so the relative links
# between them (including the in-game lesson pop-ups) work unchanged.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COLLECTION="$(cd "${1:-$REPO_ROOT/../everyday-cool}" && pwd)"
PROJECTS="$COLLECTION/projects"
MANIFEST="$REPO_ROOT/tools/games.manifest"
DIST="$REPO_ROOT/dist"

if [ ! -d "$PROJECTS" ]; then
  echo "error: no projects/ directory under $COLLECTION" >&2
  exit 1
fi

rm -rf "$DIST"
mkdir -p "$DIST/games"
cp "$REPO_ROOT/interface/index.html" "$DIST/index.html"

games=0
lessons=0
while IFS='|' read -r slug project game lesson extras; do
  case "$slug" in ''|\#*) continue ;; esac
  dest="$DIST/games/$slug"
  mkdir -p "$dest"
  cp "$PROJECTS/$project/$game" "$dest/$game"
  games=$((games + 1))
  if [ "$lesson" != "-" ]; then
    cp "$PROJECTS/$project/$lesson" "$dest/$lesson"
    lessons=$((lessons + 1))
  fi
  if [ "$extras" != "-" ]; then
    IFS=',' read -ra extra_files <<< "$extras"
    for f in "${extra_files[@]}"; do
      cp "$PROJECTS/$project/$f" "$dest/$f"
    done
  fi
  if [ "$lesson" != "-" ]; then
    echo "  $slug: $game + $lesson"
  else
    echo "  $slug: $game (no lesson yet)"
  fi
done < "$MANIFEST"

echo "sync-dist: $games games, $lessons lessons -> $DIST"
