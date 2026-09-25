# XNAUT-342 — the old Projects panel has no entry point left

Branch `agent/claude/xnaut-342`, on tron.candoo, 2026-09-25.

André, testing 1.27.1: *"Why is the old project view still there, the workspace
was supposed to replace that."*

## What changed

The fold itself landed earlier (42d6c05, already on dev): the nine tabs became
the workspace's. What was left was the panel it folded OUT of — still reachable
beside the workspace, with its own project list, its own project dropdown and
its own nine-tab nav. Two projects' worth of navigation for one project.

That panel is gone. `project-management-panel.js` is now a surface OF one
project: it takes the project as an argument, requires it, and renders none of
the chrome that used to pick one.

### Entry points removed

| Was | Now |
|---|---|
| top bar `btn-projects` → `xnautSidebarNavigate('pm')` | same button, repointed to `xnautOpenWorkspace` |
| sidebar More menu → `{ key: 'pm', label: 'Projects' }` | gone; the tree below it is the project list |
| sidebar gear menu → "Manage projects" | gone; managing one project happens in its workspace |
| `window.xnautAttachProjectManagementTab` | gone; nothing gives the panel a tab |
| `xnautSidebarNavigate('pm')` | gone; the key warns "unknown sidebar nav key" |
| `window.xnautShowProject` | gone; the New project form opens the workspace directly |
| GUI walk step "Open Projects (tasks & plan)" | "Open project workspace", marker `More about this project` |

The button was **repointed rather than deleted**, which is one deliberate
departure from the checklist of 2026-09-22. XNAUT-435 built the rail icon that
was to be the workspace's front door and its merge was REVERTED off dev
(`79a68c7`), so deleting this button would have left a release whose workspace
is reachable only by clicking a dynamic sidebar row — no named control, and
nothing for the GUI walk's retargeted step to press. It opens the project in
scope, else the first the control repository knows, else says where a project is
made.

### Surfaces removed, and where each one lives

Following André's checklist of 2026-09-22 ("Overview gives us nothing, Docs =
Vault, the rest is in the 3-dot menu"):

| Panel section | Where it is now |
|---|---|
| Overview · Active work table | the Work tab (the same list, unfiltered and untruncated) |
| Overview · Current stage band | the NAUT-Flow tab |
| Overview · Primary artifact band | the Vault tab |
| Overview · Contributors band | Project details, three-dot menu |
| Overview · health card (git numbers) | the workspace header, read live (XNAUT-341) |
| Overview · health card (budget, rate, flow) | Project details |
| Overview · health card (**Started**) | Project details — **added by this ticket** |
| Docs | the Vault tab; it was `xnautCreateVaultPane` under a second name |
| Delivery (placeholder) | the Delivery tab, a panel the workspace mounts itself |
| New project button + form | the sidebar's plus (`right-pane-newproject.js`) |
| New ticket button | the Work tab's, which it already was |

Asking for a section that no longer exists no longer falls through to Overview:
it names itself in the pane and on the console, because a fall-through is how a
stale call site keeps working and stays wrong.

### Two jumps that were broken by the fold and are fixed here

Both called globals the fold had removed, and an undefined global in JS is a
silent no-op rather than a crash (CLAUDE.md):

- The Observatory's "last project · continue" card called
  `xnautAttachProjectManagementTab({ section: 'nautflow', flowStage })`. It opens
  the workspace's NAUT-Flow tab at the remembered stage — which needed
  `flowStage` plumbed through `workspace.js` (`pendingFlowStage`), including on
  re-open of an already-open workspace, where `ticket` was being dropped too.
- The New project form called `xnautAttachProjectManagementTab()` and then
  `xnautShowProject(key)` 120ms later. It opens that project's workspace on the
  Work tab, with no second step and no guessed delay.

## The files

- `src/index.html` — `btn-projects` relabelled "Open project workspace".
- `src/js/app.js` — its handler opens the workspace; picks the project in scope,
  else the first known, else says where one is made.
- `src/js/sidebar.js` — the More menu's `pm` entry and the gear menu's
  "Manage projects" removed.
- `src/js/tasks-mode-glue.js` — `xnautAttachProjectManagementTab` and the `pm`
  nav key removed.
- `src/js/project-management-panel.js` — **4603 → 4339 lines.** Project
  required; the dropdown, project rail, Focus button, section nav, New project
  button and dialog, Project details dialog, `xnautShowProject`,
  `selectProject`, `renderProjectFilters`, `renderActiveWork`,
  `activeWorkState`, `projectKeySeed`, `mountProjectDocs`, `fillProjectFacts`,
  `ago` and the Overview, Docs and Delivery sections all removed, with the CSS
  only they used. `Started` added to Project details.
- `src/js/observatory-panel.js`, `src/js/right-pane-newproject.js`,
  `src/js/workspace.js` — the two jumps above.
- `scripts/gui-smoke.sh`, `scripts/smoked-controls.mjs`,
  `scripts/chat-contracts.mjs` — the walk step retargeted; a new contract
  asserting, as an absence, that nothing opens the old panel.
- Ten specs repointed from a standalone mount to `{ project, section }`;
  `tests/project-overview-honest.spec.mjs` rehomed onto the surfaces that now
  read the stage; `tests/project-workspace.spec.mjs`'s three standalone-nav
  tests rewritten against the workspace's own tab strip;
  `tests/workspace-entry-points.spec.mjs` is new — five cases covering the
  removal and all three replacement routes.

## Totals

- `cargo test --manifest-path src-tauri/Cargo.toml`: **1312 passed, 0 failed,
  45 ignored.** Identical to the baseline measured on this branch before any
  edit (this change is frontend only).
- `XNAUT_TEST_PORT=4291 npx playwright test`: **276 passed, 0 failed.**
  Baseline on dev was 271; the five are `workspace-entry-points.spec.mjs`.
  `npx playwright install chromium` was needed first — the browser was absent
  from this worktree's cache, which reads as 260+ failures and is not a code
  failure.

XNAUT_TEST_TOTALS={"rust":[{"passed":1312,"failed":0,"ignored":45}],"ui":[276]}

## Verifying by hand

```bash
cd src-tauri && cargo tauri dev
```

1. The top bar's project icon is titled **Project workspace**. Press it: the
   workspace opens on a project's Code tab, with the three-dot button in its
   header. No project list, no project dropdown, no nine-tab nav anywhere.
2. Open the sidebar's More button (the `⋯` in the rail). **Projects is not
   listed.** Agent Space, Skills, Plugins, Tasks, Delivery, Memory, Vault are.
3. The gear beside the Projects header offers only what is about that list —
   pinning, hidden projects, Refresh. No "Manage projects".
4. In DevTools, `window.xnautSidebarNavigate('pm')` logs
   `[tasks-mode] unknown sidebar nav key: pm` and opens nothing.
   `window.xnautAttachProjectManagementTab` and `window.xnautShowProject` are
   both `undefined`.
5. Three-dot → **Project details**: Started is there, beside Budget, Rate, Flow
   and Owner. A project the control repo has no `created_at` for says
   "Not recorded" rather than showing a date.
6. Observatory → the yellow "Last project · continue" card (needs a NAUT-Flow
   stage to have been opened once, which writes `xnaut-nf-last`). It opens the
   workspace's NAUT-Flow tab standing on the stage it names, not on the
   project's current stage.
7. Sidebar plus → New project. Fill a name and a path, Create: it lands in the
   new project's workspace on the Work tab.

## What is deliberately not done

- **The in-workspace project switcher (XNAUT-435).** Its merge was reverted off
  dev, so the workspace still has no dropdown of its own. Switching project
  means the sidebar tree. This is a real gap and not a blocking one: the
  Sessions view is a TOGGLE on the same rail icon, so the tree is always one
  click away — it does not hide the selector permanently, which is what the
  checklist of 2026-09-22 was guarding against.
- **The richer New project form.** The panel's dialog asked for owner, client,
  contact, budget, rate and flow type; the sidebar's plus asks for name, path
  and remote. Every dropped field is editable in the project's Settings tab,
  which is where they were edited afterwards anyway. Named here because it is a
  narrowing, decided by the checklist's "verify both, then drop", not an
  oversight.
- **`tests/control-inventory.json`** still records the old button label. It is a
  generated snapshot (`tests/enumerate-controls.mjs`) feeding the release
  checklist, not an asserted fixture; regenerating it needs a live enumeration
  run and belongs to that checklist.
- **`scripts/preflight.mjs` is red on this branch and was red before it**:
  `scripts/chat-contracts.mjs` reads `src/js/agents-panel.js`, which does not
  exist on dev. The contract edits here are correct; the script cannot run until
  that unrelated stale read is fixed. It is in neither required suite.
Nothing else. In particular the duplicated `ago()` XNAUT-341's handback named as
deliberately left — verbatim in `workspace.js` and in this panel — is resolved
rather than left: with the health card gone, no markup in the panel carries a
`[data-fact]`, so `fillProjectFacts` and its `ago()` had no reader at all and
went with the card. One copy now, in `workspace.js`.
