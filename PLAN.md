# XNAUT-106 — root the file/diff/git panes at the build slice

## What I found (verified, not assumed)

- `window.xnautRightPaneSetRoot(path)` exists (`src/js/right-pane.js:802`) and
  fans the new root out to every mounted view. Callers today: sidebar worktree
  click, tasks-mode-glue, app.js on task open. **No build-stage caller.**
- The build stage's slice tabs are rendered in
  `src/js/project-management-panel.js:2939` and the click handler at `:2940`
  only flips `activeTab` and repaints the terminal. Each unit carries `wt`
  (its worktree path, set in `launchSlice`) and `status`.
- `slice_changes` / `slice_file_diff` (`src-tauri/src/slice_diff.rs`) already
  compute the merge-base diff for a worktree — the correct answer for "what has
  this agent changed since it forked", including uncommitted work.
- `src/js/right-pane-buildfiles.js` is a complete UI for that data — and it is
  **dead code**: it registers the key `buildfiles`, which is not in
  `VIEW_ORDER` (`right-pane.js:49`), so no slot and no tab button exist for it.
  `mountActiveView` returns early on a missing slot, silently.
- The panes have no write path of their own, but the Files pane routes clicks
  to three surfaces that do: the markdown pane (`markdown-pane.js:119`
  `write_file`), the built-in editor (`app.js:8175` `write_file`), and
  `xnautOpenInEditor` (`tasks-mode-glue.js:186`), which spawns `$EDITOR` in a
  PTY inside the directory the agent is writing to.

## What I will build

### 1. Selecting a slice tab re-roots Files/Search/Git (ticket item 1)

In `project-management-panel.js`, the slice-tab click handler and the
build-start path call a single helper that does
`window.xnautRightPaneSetRoot(unit.wt, origin)` where `origin` is
`{ kind: 'slice', slice, project, wt, live }`. Grep-checked: `xnautRightPaneSetRoot`
is assigned as a **function** and called as one, so the shape matches.

`setRoot` gains an optional second argument. Every existing one-argument caller
keeps working and clears the banner, which is the correct behaviour for a
project switch.

### 2. A live diff for the slice (ticket item 2)

Revive `buildfiles` as a real view: add `{ key: 'buildfiles', title: 'Slice diff' }`
to `VIEW_ORDER` with an icon, and give it a `setRoot(root)` so it follows the
pane root instead of only its own picker. Rooted at a slice worktree it shows
`slice_changes` for that worktree (merge base → working tree), each file
expanding to `slice_file_diff`. No new backend command; no new tab beyond the
one this view was written for and never got.

### 3. Provenance (ticket item 3)

`right-pane.js` renders a `.rpane-origin` band under the tab bar whenever the
root came from a slice: slice name, project, and the worktree path in mono.
Cleared when `setRoot` is called with no origin. With several slices open you
can never read one slice's tree believing it is another's, because the band is
always on screen above the tree, the search results and the git list.

### 4. Read-only while the agent is writing (ticket item 4)

Both halves, because a mark with no enforcement is decoration:

- The band turns amber and reads `read-only — @<slice> is writing here` while
  that slice's status is `running`.
- A new `window.xnautLiveSliceFor(path)` answers which **running** slice owns a
  path (by worktree prefix, longest match, reading `window.xnautBuild.queue`).
  Three call sites refuse and toast instead of writing:
  `markdown-pane.js` save, `app.js` editor save, and `xnautOpenInEditor`
  (which is the worst of the three: it puts a human's `vi` in the agent's
  worktree). Viewing is never blocked — only saving.

## Tests

Playwright (`tests/slice-panes.spec.mjs`), against the existing
`tests/static-server.mjs` stub:

1. clicking slice tab 2 roots the right pane at that slice's worktree, and
   clicking tab 1 roots it back — asserted on the Files pane's own root label,
   not on the call;
2. the origin band names the slice and its worktree path, and a plain project
   `setRoot` clears it;
3. a running slice's band is marked read-only and a stopped slice's is not;
4. `xnautLiveSliceFor` returns the slice for a path inside a running worktree,
   `null` for a finished one, and `null` for a path outside;
5. a save into a live slice worktree is refused (markdown pane) and the file is
   not written — asserted on `window.__xnautInvokes` containing no `write_file`;
6. the Slice diff view mounts, calls `slice_changes` for the rooted worktree,
   and lists the changed files with their +/- counts.

Each of the six is mutation-checked: revert the production line and the test
must fail.

Rust: no backend change, so no new Rust test. Both suites run in full.

## Deliberately not done

- No editing inside the panes. The ticket's item 4 asks for read-only, so
  adding an edit path would be the opposite of the ask.
- No diff annotation. The ticket names XNAUT-91's notes pane as the natural
  home for "why did you do this"; that is a separate ticket and the diff pane
  already has a hunk-note renderer of its own.
- No re-root on every repaint — only on an explicit slice-tab click and on
  build start. A background repaint stealing the root the user chose is the
  behaviour this ticket is trying to fix, not repeat.
