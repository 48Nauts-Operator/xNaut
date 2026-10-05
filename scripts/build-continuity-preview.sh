#!/bin/sh
# XNAUT-462: separate profile and bundle; production and previous previews stay intact.
set -eu
cd "$(dirname "$0")/.."
python3 scripts/seed-full-wiki-preview.py --continuity
unset XNAUT_WIKI_PREVIEW
XNAUT_FULL_WIKI_PREVIEW=1 XNAUT_JOURNAL_PREVIEW=1 XNAUT_CONTINUITY_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/continuity-preview.config.json
# Debug linker signatures do not seal bundled resources. Sign the finished local
# preview so it opens consistently without changing release signing settings.
preview_target=$(cargo metadata --no-deps --manifest-path src-tauri/Cargo.toml --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
preview_bundle="$preview_target/debug/bundle/macos/xNAUT Continuity Preview.app"
codesign --force --deep --sign - --entitlements src-tauri/Entitlements.plist "$preview_bundle"
codesign --verify --deep --strict "$preview_bundle"
