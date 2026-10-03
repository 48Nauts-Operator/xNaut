#!/bin/sh
# XNAUT-455: build the same project Wiki in an isolated native preview shell.
# The compiled flag bypasses normal worker/scheduler/session startup; the
# distinct bundle identifier keeps this separate from the installed app.
# Document edits still use the real project Vault, as stated in the window.
set -eu
cd "$(dirname "$0")/.."
XNAUT_WIKI_PREVIEW=1 cargo tauri build --debug --bundles app --config src-tauri/wiki-preview.config.json
