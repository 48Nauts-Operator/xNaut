#!/usr/bin/env bash
# Local UI preview; separate identity, settings, history, no background dispatch.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/seed-full-wiki-preview.py --journal-v4
XNAUT_FULL_WIKI_PREVIEW=1 XNAUT_JOURNAL_PREVIEW=1 XNAUT_JOURNAL_V4_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/journal-v4-preview.config.json
preview_bundle="${CARGO_TARGET_DIR:-src-tauri/target}/debug/bundle/macos/xNAUT UI Preview.app"
if [ "$(uname -s)" = Darwin ]; then
  codesign --force --deep --sign - --identifier com.nautcode.xnaut.journal-v4-preview --entitlements src-tauri/Entitlements.plist "$preview_bundle"
  codesign --verify --deep --strict "$preview_bundle"
fi
