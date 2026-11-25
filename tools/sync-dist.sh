#!/usr/bin/env bash
# Assemble dist/ for the Everyday Cool Arcade from a checkout of the
# Everyday Cool collection.
#
#   tools/sync-dist.sh [path-to-collection]
#
# The collection path defaults to ../everyday-cool (a sibling checkout).
# dist/ is rebuilt from scratch every run:
#   dist/index.html        <- interface/index.html (the arcade shelf)
#   dist/about.html        <- interface/about.html
#   dist/documents.html    <- GENERATED from tools/games.manifest: the
#                             Game Documents page the app's Help menu
#                             opens — one entry per game, linking its
#                             lesson. Never hand-edit the generated
#                             page; add the game to the manifest.
#   dist/games/<slug>/     <- the game file, its lesson file, and any
#                             support files, per tools/games.manifest
#   dist/emulators/        <- the Emulator Box overlay: the nine Nerd
#                             Heaven console pages and their lessons,
#                             copied FLAT (the pages cross-link siblings
#                             by bare filename), plus their racks
#                             (javatari/, jsnes/, emulatorjs-snes/,
#                             emulatorjs-dos/, apple2ts/,
#                             emulatorjs-mame/, emulatorjs-7800/,
#                             emulatorjs-5200/) copied whole.
#                             dist/emulators/index.html is a stub that
#                             bounces the consoles' room-index link back
#                             to the arcade shelf.
#   src-tauri/resources/baton-box/  <- the Baton Box server bundle the
#                             app's Start-the-Baton-Box control runs:
#                             refreshed from a sibling ../baton-box
#                             checkout (override with BATON_BOX_DIR).
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

# Escape a string for use as HTML text content.
html_escape() {
  printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'
}

rm -rf "$DIST"
mkdir -p "$DIST/games"
cp "$REPO_ROOT/interface/index.html" "$DIST/index.html"
cp "$REPO_ROOT/interface/about.html" "$DIST/about.html"
cp "$REPO_ROOT/interface/arcade-robots.webp" "$DIST/arcade-robots.webp"

DOC_ROWS="$(mktemp)"
EMU_ROWS="$(mktemp)"
trap 'rm -f "$DOC_ROWS" "$EMU_ROWS"' EXIT

games=0
lessons=0
while IFS='|' read -r slug project game lesson extras title; do
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
    printf '      <li><span class="doc-title">%s</span> <a class="doc-link" href="games/%s/%s">📖 Read the lesson</a></li>\n' \
      "$(html_escape "$title")" "$slug" "$lesson" >> "$DOC_ROWS"
  else
    echo "  $slug: $game (no lesson yet)"
    printf '      <li><span class="doc-title">%s</span> <span class="no-lesson">📖 Lesson in the works</span></li>\n' \
      "$(html_escape "$title")" >> "$DOC_ROWS"
  fi
done < "$MANIFEST"

# ---------------------------------------------------------------------------
# The Emulator Box overlay. The nine consoles are Nerd Heaven pages, not
# manifest games: each page expects its lesson, its sibling consoles, and
# its rack folder beside it under bare relative names, so the whole set
# is copied FLAT into dist/emulators/ and cross-links work unchanged.
# The one link that cannot resolve there is the consoles' room-index
# link (index.html); the stub written below bounces it to the shelf.
EMU_SRC="$PROJECTS/nerd-heaven-tech-support"
EMU="$DIST/emulators"
mkdir -p "$EMU"
emu_count=0
while IFS='|' read -r emu_page emu_lesson emu_title; do
  case "$emu_page" in ''|\#*) continue ;; esac
  cp "$EMU_SRC/$emu_page" "$EMU/$emu_page"
  cp "$EMU_SRC/$emu_lesson" "$EMU/$emu_lesson"
  printf '      <li><span class="doc-title">%s</span> <a class="doc-link" href="emulators/%s">📖 Read the lesson</a></li>\n' \
    "$(html_escape "$emu_title")" "$emu_lesson" >> "$EMU_ROWS"
  emu_count=$((emu_count + 1))
  echo "  emulators: $emu_page + $emu_lesson"
done <<'EMU_LIST'
the-6502-bench.html|the-6502-bench-lesson.html|The 6502 Bench
the-2600-console.html|the-2600-console-lesson.html|The 2600 Console
the-nes-console.html|the-nes-console-lesson.html|The NES Console
the-snes-console.html|the-snes-console-lesson.html|The SNES Console
the-dos-console.html|the-dos-console-lesson.html|The DOS Console
the-apple-iie-console.html|the-apple-iie-console-lesson.html|The Apple IIe Console
the-mame-console.html|the-mame-console-lesson.html|The MAME Console
the-7800-console.html|the-7800-console-lesson.html|The 7800 Console
the-5200-console.html|the-5200-console-lesson.html|The 5200 Console
EMU_LIST
for rack in javatari jsnes emulatorjs-snes emulatorjs-dos apple2ts emulatorjs-mame emulatorjs-7800 emulatorjs-5200; do
  cp -r "$EMU_SRC/$rack" "$EMU/$rack"
done
cat > "$EMU/index.html" <<'EMU_STUB'
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>The Emulator Box — back to the shelf</title>
<meta http-equiv="refresh" content="0; url=../index.html">
</head>
<body>
<p>The consoles live on the shelf — <a href="../index.html">back to the Arcade shelf</a>.</p>
</body>
</html>
EMU_STUB
echo "  emulators: $emu_count consoles, racks javatari jsnes emulatorjs-snes emulatorjs-dos apple2ts emulatorjs-mame emulatorjs-7800 emulatorjs-5200 -> $EMU"

# The Game Documents page: interface styling, generated entirely from
# the manifest rows collected above (plus the counts from this run).
{
  cat <<'PAGE_HEAD'
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Game Documents — Everyday Cool Arcade</title>
<style>
  :root {
    --cream: #f8f6ef;
    --paper: #fffdf7;
    --ink: #232733;
    --ink-soft: #4c5265;
    --orange: #eb694b;
    --orange-dark: #c94f33;
    --line: #e3ddcd;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    background: var(--cream);
    color: var(--ink);
    font-family: "Segoe UI", system-ui, -apple-system, sans-serif;
    line-height: 1.5;
  }
  .wrap { max-width: 1180px; margin: 0 auto; padding: 0 24px 64px; }

  header.hero { padding: 56px 0 8px; text-align: left; }
  .eyebrow {
    letter-spacing: 0.22em;
    font-size: 12px;
    font-weight: 700;
    color: var(--orange-dark);
    text-transform: uppercase;
    margin: 0 0 10px;
  }
  h1 {
    font-family: Georgia, "Times New Roman", serif;
    font-size: clamp(40px, 6vw, 64px);
    line-height: 1.05;
    margin: 0 0 14px;
    font-weight: 700;
  }
  h1 em { color: var(--orange); font-style: italic; }
  .lead {
    font-size: 19px;
    color: var(--ink-soft);
    max-width: 46em;
    margin: 0 0 18px;
  }
  .counts {
    display: inline-block;
    background: var(--ink);
    color: var(--cream);
    border-radius: 999px;
    padding: 6px 16px;
    font-size: 14px;
    font-weight: 600;
    margin-bottom: 10px;
  }
  .tip { font-size: 14px; color: var(--ink-soft); max-width: 46em; }
  .tip a { color: var(--orange-dark); font-weight: 600; }

  ul.doc-list { list-style: none; margin: 26px 0 0; padding: 0; }
  ul.doc-list li {
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 14px 18px;
    margin-bottom: 10px;
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    box-shadow: 0 1px 3px rgba(35, 39, 51, 0.06);
  }
  .doc-title {
    font-family: Georgia, "Times New Roman", serif;
    font-size: 18px;
    font-weight: 700;
    flex: 1;
  }
  a.doc-link {
    color: var(--ink);
    text-decoration: none;
    font-weight: 600;
    padding: 7px 14px;
    border: 1.5px solid var(--ink);
    border-radius: 9px;
    font-size: 14.5px;
  }
  a.doc-link:hover { background: var(--ink); color: var(--cream); }
  .no-lesson {
    font-size: 13px;
    color: var(--ink-soft);
    border: 1.5px dashed var(--line);
    padding: 7px 12px;
    border-radius: 9px;
  }

  footer {
    margin-top: 64px;
    border-top: 1px solid var(--line);
    padding-top: 20px;
    color: var(--ink-soft);
    font-size: 14.5px;
  }
</style>
</head>
<body>
<div class="wrap">

  <header class="hero">
    <p class="eyebrow">Everyday Cool Arcade</p>
    <h1>Game <em>Documents</em></h1>
    <p class="lead">Every game keeps a back door: a lesson page that explains the physics — or the
      method — the game runs on, shows the math with worked examples, and tells how the game itself
      was built, in plain language. This is the shelf of all of them, one per game.</p>
PAGE_HEAD
  printf '    <span class="counts">%s games · %s lessons</span>\n' "$games" "$lessons"
  cat <<'PAGE_MID'
    <p class="tip"><a href="index.html">← Back to the shelf</a></p>
  </header>

  <section>
    <ul class="doc-list">
PAGE_MID
  cat "$DOC_ROWS"
  cat <<'PAGE_EMU_HEAD'
    </ul>
  </section>

  <section>
    <p class="eyebrow">Nerd Heaven</p>
    <h1 style="font-size: clamp(28px, 4vw, 40px);">The <em>Emulator Box</em></h1>
    <p class="lead">The nine consoles keep their back doors too: each lesson is the audit of the
      machine it ships — what was measured, what bends, and what to bring yourself. The consoles
      play from the shelf's Emulator Box section.</p>
    <ul class="doc-list">
PAGE_EMU_HEAD
  cat "$EMU_ROWS"
  cat <<'PAGE_TAIL'
    </ul>
  </section>

  <footer>
    <p>Part of Everyday Cool — the full workshops live at caeled.github.io/everyday-cool ·
      Free to explore. Built with AI assistance. Curated by a human.</p>
  </footer>

</div>
</body>
</html>
PAGE_TAIL
} > "$DIST/documents.html"

# ---------------------------------------------------------------------------
# The Baton Box server bundle (src-tauri/resources/baton-box/): what the
# app's Start-the-Baton-Box control runs. Refreshed from a sibling
# checkout of the standalone box (override with BATON_BOX_DIR). The
# server file is copied byte-for-byte; runtime state (sessions.json,
# __pycache__) never rides into the bundle.
BATON_SRC="${BATON_BOX_DIR:-$REPO_ROOT/../baton-box}"
BATON_DEST="$REPO_ROOT/src-tauri/resources/baton-box"
if [ -f "$BATON_SRC/baton_box.py" ]; then
  rm -rf "$BATON_DEST"
  mkdir -p "$BATON_DEST"
  cp "$BATON_SRC/baton_box.py" "$BATON_DEST/baton_box.py"
  for extra in start-baton-box.bat README.md CONTRACT.md; do
    if [ -f "$BATON_SRC/$extra" ]; then
      cp "$BATON_SRC/$extra" "$BATON_DEST/$extra"
    fi
  done
  cp -r "$BATON_SRC/games" "$BATON_DEST/games"
  find "$BATON_DEST" -type d -name "__pycache__" -prune -exec rm -rf {} +
  find "$BATON_DEST" -name "sessions.json" -delete
  echo "  baton-box: server + games refreshed from $BATON_SRC -> $BATON_DEST"
else
  echo "  warning: no Baton Box checkout at $BATON_SRC — bundled resources left as they were" >&2
fi

echo "sync-dist: $games games, $lessons lessons, $emu_count consoles -> $DIST"
