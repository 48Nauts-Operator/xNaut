#!/usr/bin/env bash
# Local UI preview; separate identity, settings, history, no background dispatch.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/seed-full-wiki-preview.py --journal-v4
XNAUT_FULL_WIKI_PREVIEW=1 XNAUT_JOURNAL_PREVIEW=1 XNAUT_JOURNAL_V4_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/journal-v4-preview.config.json
