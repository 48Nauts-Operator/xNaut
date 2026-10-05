#!/bin/sh
# XNAUT-462: separate profile and bundle; production and previous previews stay intact.
set -eu
cd "$(dirname "$0")/.."
python3 scripts/seed-full-wiki-preview.py --continuity
unset XNAUT_WIKI_PREVIEW
XNAUT_FULL_WIKI_PREVIEW=1 XNAUT_JOURNAL_PREVIEW=1 XNAUT_CONTINUITY_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/continuity-preview.config.json
