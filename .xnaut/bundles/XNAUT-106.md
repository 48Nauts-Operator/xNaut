# XNAUT-106 test bundle

Author: @claude
Date: 2026-09-20
Branch: agent/claude/xnaut-106

## Change

You can now see the codebase an agent is working in. Selecting a build slice
tab roots the right pane's Files, Search, Git and Slice diff views at THAT
slice's worktree; a provenance band names which worktree is on screen; a slice
that is still running is marked read-only and that mark is enforced at the
three places a human could otherwise write into the agent's directory.

Four pieces, one per item in the ticket:

1. **Re-root on slice select.** `project-management-panel.js` gained
   `showSliceInPanes(unit)`, called from the slice-tab click handler and from
   the end of the build-start path. The plumbing it calls
   (`window.xnautRightPaneSetRoot`) already existed and the build stage simply
   never called it. Deliberately NOT called from `renderTabs`, which runs on
   every status poll: re-rooting there would yank the pane away from whoever
   had navigated elsewhere.

2. **A live diff for the slice.** `right-pane-buildfiles.js` was a complete,
   correct UI for `slice_changes` / `slice_file_diff` — and it was dead code.
   It registered the key `buildfiles`, which was not in `right-pane.js`'s
   `VIEW_ORDER`, so it got no slot and no tab button, and `mountActiveView`
   returned early in silence. It is now in `VIEW_ORDER` as "Slice diff", takes
   its initial root from `mount(container, root)`, and follows the pane root
   through a real `setRoot` instead of the empty one it shipped with.
   Two latent bugs in it were fixed while reviving it: `destroy()` is called by
   the host with NO arguments, so the cleanup hung on `container.__bfCleanup`
   never ran and the 15s git poll leaked; and the picker now includes the
   rooted directory when it is not one of the queue's slices.

3. **Provenance.** `right-pane.js`'s `setRoot(path)` takes an optional second
   argument describing where the root came from, and renders a band under the
   tab bar: badge, slice name with its branch, and the worktree path in mono.
   Every pre-existing caller passes one argument and therefore clears the band,
   which is the correct reading of a project switch. `window.xnautRightPaneOrigin()`
   exposes it.

4. **Read-only while the agent writes.** New `src/js/slice-scope.js` answers
   which RUNNING slice owns a path, from `window.xnautBuild.queue` — the same
   list the slice tabs and the Build run pane read, with nothing cached, so a
   slice that finished a second ago stops being read-only a second ago.
   `window.xnautSliceWriteBlocked(path)` refuses and toasts, and is wired into
   all three write paths: the markdown pane's save, the built-in editor's save,
   and `xnautOpenInEditor` — the worst of them, which puts a human's `$EDITOR`
   inside the directory the agent is committing from, where there is no save
   hook left to refuse. Viewing is never blocked; that is the point of the
   ticket.

`publishBuildToSwarm` now carries `branch` on each queue entry, because the
band and the guard both name the slice to a human and "which branch" is half of
"which slice".

Files:

- src/js/right-pane.js
- src/js/right-pane-buildfiles.js
- src/js/slice-scope.js (new)
- src/js/project-management-panel.js
- src/js/markdown-pane.js
- src/js/app.js
- src/js/tasks-mode-glue.js
- src/index.html
- tests/slice-panes.spec.mjs (new)
- tests/static-server.mjs
- PLAN.md
- .xnaut/bundles/XNAUT-106.md

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml`: exit 0; **1250 passed, 0
  failed, 45 ignored**, 0 filtered. Identical to the baseline taken before any
  edit — there is no backend change in this ticket.
- `XNAUT_TEST_PORT=4291 npx playwright test`: exit 0; **244 passed, 0 failed**,
  2.9m. 236 before, plus the eight new ones.
- `node --check` on all seven edited JS files: exit 0.
- `npx eslint` on the three most-changed files: clean apart from one
  pre-existing `no-dupe-keys` on `ICONS.agent` (`right-pane.js:26` and `:42`),
  which predates this branch and is left alone — the two definitions differ, so
  deleting either changes which icon renders and that is not this ticket.

### Mutation checks

Every one of the eight tests was checked by reverting its production line and
confirming the test fails:

| Reverted | Test that failed |
| --- | --- |
| `showSliceInPanes(units()[activeTab])` removed from the tab click | selecting a build slice roots the panes… |
| `setRoot` stores `origin = null` | the provenance band names the worktree… |
| the band never sets `data-live` | a running slice is marked read-only… |
| longest-prefix → first match | a path is attributed… by longest prefix |
| `startsWith(wt + '/')` → `startsWith(wt)` | a path is attributed… by longest prefix |
| guard removed from `xnautOpenInEditor` | a save into a worktree an agent is writing… |
| Slice diff's `setRoot(root)` → `setRoot() {}` | the Slice diff view shows what the rooted worktree changed… |
| the picker drops the rooted dir when it is not a slice | with no build running, Slice diff reads the project root… |
| the unattributable-build `console.warn` removed | a build that publishes no worktrees permits saves, and says why |

Two of these were found BY the mutation check rather than confirmed by it, and
both made the tests better:

- the first longest-prefix mutation SURVIVED, because the sibling case
  (`engine` vs `engine-docs`) is decided by the `/` boundary test, not by
  longest-prefix. The test now also covers a worktree nested inside another,
  which is the only case where longest-prefix actually decides.
- the first `setRoot` mutation on the Slice diff view SURVIVED, because the
  view mounts fresh on first tab click and reads its root from `mount`. The
  test now switches slice while the view is already on screen, which is where
  `setRoot` is the only thing that can be right.

### The plan review, and what it changed

The `/v1/plan/review` endpoint reported only `pending` with a gate note,
"missing, stale, late or invalid reviewer identity". The ticket record shows
what that hid: **one of the two reviewers did return a verdict.** `codex`
exited 71 and produced nothing — that is the identity failure, and it is what
held the gate. `claude` returned `changes_requested` at 0.74 confidence with
seven reasons. Neither of its two BLOCKERs is about the design: both are
submission metadata this run never supplied, because the plan was posted as
prose over HTTP with no `spend_estimate` and no declared paths. Its two minor
points are substantive, and both are now fixed with a mutation-checked test:

- *"Adding `{ key: 'buildfiles' }` to VIEW_ORDER adds a visible tab for every
  user, not only during a build. State what that tab shows when the root is a
  plain project rather than a slice worktree."* — it shows the project's own
  changes against its closest fork, which is what `slice_changes` measures for
  any worktree. Asserted by *with no build running, Slice diff reads the
  project root rather than erroring*, which also asserts `window.__xnautErrors`
  is empty.
- *"A refusal that silently becomes a permit would defeat ticket item 4."* — a
  build marked `active` that publishes no worktrees is now a logged, explained
  permit rather than a silent one. Permitting stays correct: with no worktree
  list every path is equally unattributable, so refusing would block every save
  in the app rather than protect one worktree. Asserted by *a build that
  publishes no worktrees permits saves, and says why*, including that it does
  not warn twice.

This is recorded because the reviewer's verdict was not visible on the surface
this run used to ask for it, and the next agent to hit the same reviewer
failure should know to read the ticket's `jury_reviews` rather than conclude no
review happened.

## Manual verification

1. `cd src-tauri && cargo tauri dev` from this worktree.
2. Open a project → NautFlow → Build, and start a build (or open a project that
   already has a build in `window.xnautBuild.queue`).
3. Click the second slice tab. The right pane's Files tree re-roots at that
   slice's worktree, and the band above it reads
   `BUILD SLICE · <title> · nautloom/<slug>` with the worktree path under it.
4. Click the Slice diff icon. It lists what that slice changed since it forked
   — committed AND uncommitted, measured from the merge base, so an agent that
   has already committed twelve times does not read as an agent doing nothing.
   Click a file for its diff. Switch slice tabs: the list follows.
5. While that slice's status is `running`, the band is amber and reads
   `READ-ONLY · agent writing`. Click a `.md` file in the tree and try to save
   it: the save is refused with a toast naming the slice. Right-click → open in
   your editor is refused the same way. When the slice finishes, both work again.

## What is deliberately not done

- **No editing inside the panes.** The ticket's item 4 asks for read-only, so
  an edit path would be the opposite of the ask.
- **No diff annotation.** The ticket names XNAUT-91's notes pane as the natural
  home for "why did you do this". `diff-pane.js` already has a hunk-note
  renderer; joining the two is that ticket, not this one.
- **No re-root on repaint**, argued above.
- **The pre-existing `ICONS.agent` duplicate is left in place**, argued above.

## Not verified

The plan was posted for blind review
(`in-55d5eee0-e708-4510-a614-bcf757997274`) and never got a decision: it
answers `pending` with note 1 "missing, stale, late or invalid reviewer
identity", a failure the control repo records as already remembered three times
on XNAUT-106 and XNAUT-379. `pending` is not approval and was not treated as
approval. The plan as submitted is in `PLAN.md`.

No mechanism was ported from another project.

XNAUT_TEST_TOTALS={"rust":[{"passed":1250,"failed":0,"ignored":45}],"ui":[244]}
