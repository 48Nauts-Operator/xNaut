#!/usr/bin/env bash
# Local approved-design preview; separate settings/history and no auto-dispatch.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/seed-full-wiki-preview.py --journal-final
XNAUT_FULL_WIKI_PREVIEW=1 XNAUT_JOURNAL_PREVIEW=1 XNAUT_JOURNAL_V4_PREVIEW=1 XNAUT_JOURNAL_FINAL_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/journal-final-preview.config.json
preview_bundle="${CARGO_TARGET_DIR:-src-tauri/target}/debug/bundle/macos/xNAUT Journal Test.app"
if [ "$(uname -s)" = Darwin ]; then
  codesign --force --deep --sign - --identifier com.nautcode.xnaut.journal-final-preview --entitlements src-tauri/Entitlements.plist "$preview_bundle"
  codesign --verify --deep --strict "$preview_bundle"
fi
