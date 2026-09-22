# XNAUT-379 — Monaco in the Code tab

## What changed

- Every text file in the Code tab now has an **Edit** toggle. The existing
  highlighted viewer remains the default and Monaco is fetched only after the
  first Edit click.
- Monaco 0.56.0, its editor worker, stylesheet, inline codicon font, and MIT
  licence are generated into `src/js/vendor/` by `npm run build:monaco`. No CDN
  or other network asset is needed.
- Monaco takes its foreground, background, selection, accent, font family,
  font size, and line height from the app's resolved theme tokens. Rust files
  open with the Rust language mode.
- **Save** first opens Monaco's diff editor with the disk version on the left
  and the edit on the right. Only **Write the file** invokes `code_edit_save`.
- The backend refuses that write while the worktree has a live writer lease,
  naming the lease holder, run, and ticket and directing the human to the
  existing takeover path. The lookup is shared with the run/ticket attribution
  path instead of duplicating live-run rules.
- A successful write refreshes `git_uncommitted_files` and marks the matching
  tree row dirty. **Open in editor** remains beside Edit as the escape hatch.
- `scripts/preflight.mjs` now verifies that Monaco is bundled, remains lazy,
  carries its licence, embeds its font, and contains no remote asset URL.

## Files

- `package.json`, `package-lock.json`
- `frontend/monaco-editor-entry.js`, `frontend/monaco-worker-entry.js`
- `scripts/build-monaco.mjs`, `scripts/preflight.mjs`
- `src/js/vendor/monaco.bundle.js`, `src/js/vendor/monaco.bundle.css`
- `src/js/vendor/monaco.worker.js`, `src/js/vendor/monaco.LICENSE.txt`
- `src/js/workspace.js`
- `src-tauri/src/commands.rs`, `src-tauri/src/run_control.rs`
- `src-tauri/src/swarm.rs`, `src-tauri/src/main.rs`
- `src-tauri/permissions/default.toml`
- `src-tauri/gen/schemas/acl-manifests.json`
- `tests/project-workspace.spec.mjs`

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml`: 1296 passed, 0 failed,
  45 ignored.
- `XNAUT_TEST_PORT=4291 npx playwright test`: 252 passed, 0 failed in 3.5m.
- `npm run build`: regenerated both Monaco and Loops bundles successfully.
- `node scripts/preflight.mjs --fast`: all three Monaco checks passed. The
  repository preflight still reports two unrelated existing failures in chat
  contracts and the missing `xnautCreatePmPanel` pane factory.
- `cargo tauri build --bundles app,dmg`: produced `xNAUT.app`,
  `xNAUT_1.27.0_aarch64.dmg`, and the updater archive. The command then exited
  1 because no `TAURI_SIGNING_PRIVATE_KEY` is present for updater signing.
- The DMG was attached read-only with `hdiutil attach -readonly -nobrowse`;
  `strings -a xNAUT.app/Contents/MacOS/xnaut` found the bundled Monaco editor,
  worker, stylesheet, and licence paths in both the direct app and the copy
  inside the DMG.

XNAUT_TEST_TOTALS={"rust":[{"passed":1296,"failed":0,"ignored":45}],"ui":[252]}

## Manual review

1. Open a project and a `.rs` file in Code. Confirm the viewer appears without
   a Monaco resource request, then click **Edit** and confirm first Edit is
   below 500 ms and uses the viewer's font and app theme.
2. Change a line and click **Save**. Confirm a side-by-side diff appears and
   disk remains unchanged until **Write the file** is pressed. Confirm the tree
   row then shows dirty state and Commit remains a separate action.
3. With a live agent holding that worktree's writer lease, repeat the write.
   Confirm the diff remains open and the refusal names the holder, run, and
   ticket. Use only the existing takeover path to proceed.
4. On a clean machine, install the generated DMG, disable networking, and
   repeat step 1. This launch was not performed in this run because it would
   start a second xNAUT instance against the owner's live port and lease state;
   the pending owner approval could not be read while port 51737 was stalled.

## Deliberately later

LSP bridging, command-palette/keybinding parity, and Monaco in the Vault note
editor remain separate follow-on work.
