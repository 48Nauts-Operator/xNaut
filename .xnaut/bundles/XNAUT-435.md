# XNAUT-435 — the project workspace has a front door

Branch `agent/claude/xnaut-435`, worktree
`xnaut-worktrees/agent-claude-xnaut-435`.

Design document:
`~/.xnaut-vault/work/xnaut/Development/features/2026-09-22_XNAUT-435.md`
(`work:xnaut/Development/features/2026-09-22_XNAUT-435.md`).

## What changed

The workspace was reachable only by clicking a file in the file system. No rail
icon, no menu entry, no command opened it, and since XNAUT-342 removed the
project dropdown there was no way to change project from inside it either — the
sidebar tree was the selector, and the Sessions list replaces that tree.

Five changes, one way in and one way across:

1. **A Projects entry in the sidebar rail** (`src/js/sidebar.js`), first of six
   icons. Pressing it unfolds a collapsed sidebar and a folded Projects section,
   then opens the workspace for the project in scope, falling back to the first
   project in the list. Fresh launch → rail → project row = a project's Code tab
   in two clicks.
2. **One project switcher in the workspace, on every tab**
   (`src/js/workspace.js`, `.wsp-projbar`). It is the control the Delivery tab
   already drew at the top of its left column, lifted into `src/js/project-select.js`
   so there is one component and one list. Choosing a project switches in place
   and keeps the current tab.
3. **Delivery's own select is off inside the workspace** — `hideProjectSelect`,
   passed by `surfaceFactory`. Standalone Delivery still draws it.
4. **The More menu's "Projects" is now "Manage projects"**, the name the gear
   menu already gives the same `navigate('pm')` call, so the rail's Projects icon
   is unambiguous to a reader and to `axui press -x`.
5. **The GUI walk presses it**: `scripts/gui-smoke.sh` opens its surface walk
   with `"Projects|Code|"`, which only passes if a workspace with its Code tab is
   on screen after the press.

### Files

```
src/index.html
src/js/project-select.js          (new)
src/js/workspace.js
src/js/sidebar.js
src/js/delivery-panel.js
scripts/gui-smoke.sh
scripts/smoked-controls.mjs
tests/project-workspace.spec.mjs
tests/sidebar-project-tree.spec.mjs
tests/named-controls-unique.spec.mjs
tests/delivery-lifecycle.spec.mjs
.xnaut/bundles/XNAUT-435.md       (new)
```

## Suites

Both run in this worktree on `tron.candoo`, 2026-09-22.

```
cargo test --manifest-path src-tauri/Cargo.toml
  test result: ok. 1250 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out

XNAUT_TEST_PORT=4291 npx playwright test
  242 passed (2.9m)
```

`npm install` was needed first: this worktree had no `node_modules`.

XNAUT_TEST_TOTALS={"rust":[{"passed":1250,"failed":0,"ignored":45}],"ui":[242]}

## Verify by hand

```bash
cd src-tauri && cargo tauri dev
```

1. **The front door.** In the sidebar rail, the first icon is Projects (two
   stacked cards). Press it: a workspace opens on the Code tab, rooted on the
   project in scope, or on the first project when nothing is in scope. Collapse
   the sidebar to its 52px strip and fold the Projects header first — the press
   unfolds both on its way in.
2. **The switcher.** Between the tab strip and the body, at the head of the left
   column, there is a `PROJECT` label over a dropdown listing every project, the
   open one selected. It is there on Code, Work, Delivery, NAUT-Flow, Vault and
   Memory.
3. **Switching keeps the tab.** Open the Work tab, pick another project: the
   header name changes, the Work tab is still the active tab. Go to Code: the
   tree is the new project's checkout, and nothing from the old one is open.
4. **One select, not two.** On the Delivery tab inside the workspace there is no
   second dropdown above its runs list. Open Delivery on its own (sidebar rail →
   More surfaces → Delivery) and its own select is there, unchanged.
5. **Cmd+P** from anywhere in the workspace focuses the switcher. Arrows and
   typing then pick a project. It focuses rather than opens: a native `<select>`
   has no programmatic open.
6. **Names.** The More menu reads "Manage projects", not "Projects".

## Not finished

Nothing in scope is outstanding. Three things are deliberately out of scope and
are argued in the design document's "Deliberately not done": the header name
stays a label (the 19:53 dropdown-on-`.wsp-name` idea was replaced by André's
20:00 note), the options carry no ticket count and no type-to-filter box (same
replaced design), and "Open workspace" in the app menu and the command palette
is not shipped — the rail entry, the switcher and the GUI walk are the front
door the ticket's acceptance asks for.

`scripts/gui-smoke.sh` cannot be run from this session: the walk needs a
logged-in GUI session with Accessibility and Screen Recording granted to the
bundle, and neither can be granted over ssh. The line added to it is verified by
`tests/named-controls-unique.spec.mjs`, which asserts that "Projects" names
exactly one control in the app — the condition under which `axui press -x`
presses it rather than refusing.
