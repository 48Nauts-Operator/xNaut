#!/bin/sh
# Full application with isolated chat/settings and no automatic worker startup.
# Project files, PM records and Vault documents remain the real project data.
set -eu
cd "$(dirname "$0")/.."
python3 scripts/seed-full-wiki-preview.py
unset XNAUT_WIKI_PREVIEW
XNAUT_FULL_WIKI_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/full-wiki-preview.config.json
