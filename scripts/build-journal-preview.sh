#!/bin/sh
# Isolated native Journal test build; production and earlier previews stay open.
set -eu
cd "$(dirname "$0")/.."
python3 scripts/seed-full-wiki-preview.py --journal
unset XNAUT_WIKI_PREVIEW
XNAUT_FULL_WIKI_PREVIEW=1 XNAUT_JOURNAL_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/journal-preview.config.json
