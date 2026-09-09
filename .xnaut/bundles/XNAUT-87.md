# XNAUT-87 — the project overview reports state instead of inventing it

Branch `agent/claude/xnaut-87`, worktree
`/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-87`.

## What changed

The overview's stage was fabricated at the renderer and stamped at the backend,
so both halves are fixed. The rule applied throughout: **never display a value
that had to be invented.**

### 1. A project is allowed to have NO stage (`src/js/project-management-panel.js`)

The bug was one expression, repeated at every call site:

```js
const stage = stages.some((i) => i[0] === project.stage) ? project.stage : stages[0][0];
```

A missing or unrecognised stage silently became stage 1. It is invisible to
review because the fabricated value is a well-formed stage key — the page looks
right and is wrong. Replaced by one seam that can answer "no":

```js
function currentStageOf(project) {
  const stages = stagesFor(project);
  const index = stages.findIndex((item) => item[0] === project.stage);
  return index < 0 ? null : { stages, index, stage: stages[index], key: stages[index][0] };
}
```

Everything that showed a stage now asks it first and renders nothing on `null`:

- **Hero badge** — via a new `projectHero()`, so all nine project sections get
  the same answer rather than each re-deriving it.
- **Current stage band** (new) — stage name, its phase, and `n of N`, all read
  from the stage table. Rendered only when the project genuinely has a stage.
- **Primary artifact** — this band IS the current stage's Vault document. With
  no stage there is no such document, and the old code named one anyway; the
  Open button then offered to create it.
- **Contributors** — was `project.owner || 'Unassigned'` under a "Stage
  ownership" heading that never reflected a stage. An unowned project now shows
  no Contributors band.
- **Artifacts tab** — "Open current document" is replaced by a sentence saying
  the project is not in NAUT-Flow.
- **NAUT-Flow rail** — `currentIndex` is `-1`, not `0`, for a project with no
  stage: nothing is marked done or current, and the count reads `Not started`
  instead of `1 / 15`. Opening the tab is how you *look* at the flow; promoting
  or skipping is how you enter it.

Also removed, because it backed panels that no longer exist: the `.pmw-gate-*`
and `.pmw-readiness` / `.pmw-ticket-lock` CSS (the hardcoded `Quality gate 0/3`
checkboxes and the readiness bar), the dead locals `ticketsReady`, `planIndex`
and `controlConnected`, and `flowPhases()`, whose only caller was one of them.

### 2. The stage stops being stamped on imports (`src-tauri/src/project_management.rs`)

The renderer was only half the mechanism. `default_project_stage()` put `"idea"`
on every project — `import_task_projects`, both legacy migrations, and the
`#[serde(default)]` on the field itself — so *no project could have no stage*
and the frontend fix would have been latent. `pm_project_update` then filtered
an empty requested stage away as "no change", so a project could enter a flow
and never leave.

```rust
// Empty means the project is not in NautFlow, which is a legitimate and
// common state rather than a missing value (XNAUT-87).
#[serde(default, deserialize_with = "null_as_default")]
pub stage: String,
```

`null_as_default` because `stage` is a `String` and `#[serde(default)]` alone
does not cover an explicit `null` — the trap CLAUDE.md records as having once
blanked the whole Projects board.

The three-way decision in `pm_project_update` was tangled in one `if let` chain
where only two branches worked. Lifted into a testable seam:

```rust
fn next_project_stage(
    current: &str, requested: Option<&str>, flow_type: &str, flow_changed: bool,
) -> Result<String, String> {
    let rebased = if flow_changed && !current.is_empty() {
        if flow_type == "incident" { "intake".to_string() } else { default_project_stage() }
    } else { current.to_string() };
    match requested.map(str::trim) {
        None => Ok(rebased),
        Some("") => Ok(String::new()),   // leaving NautFlow, previously unreachable
        Some(stage) => validate_choice(stage, "project stage", stage_keys(flow_type)),
    }
}
```

Note the `!current.is_empty()` guard on the re-base: changing an unstaged
project's flow type says which track it *would* run, not that it is running one.
Without it, the Settings form would have pulled projects into a flow — the same
fabrication arriving through a different door.

**Deliberately unchanged:** `pm_project_create`. Creating a project through the
New Project form *is* opting into NAUT-Flow, so that path still starts at
`idea` / `intake`.

## Files changed

- `src/js/project-management-panel.js`
- `src-tauri/src/project_management.rs`
- `tests/project-overview-honest.spec.mjs` (new, 11 tests)

## Test results

Both from my own runs in this worktree, on 2026-09-09.

```
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
test result: ok. 1048 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out; finished in 8.50s
```

```
npx playwright test          # see the port note below
144 passed (2.7m)
```

Baselines before this work: Rust 1040 passed / 0 failed / 45 ignored, UI 133.
The deltas are the 8 new Rust tests and the 11 new UI tests.

XNAUT_TEST_TOTALS={"rust":[{"passed":1048,"failed":0,"ignored":45}],"ui":[144]}

### Port note — read this before reproducing

The dispatch note says `XNAUT_TEST_PORT=4291`. **That port was in use by another
worktree** (`xnaut-88`) while this ticket was being worked, and
`playwright.config.mjs` sets `reuseExistingServer: true`, so a run on 4291
silently attached to *that* worktree's frontend and tested their `src/` instead
of mine. It showed up as 8 of my 11 tests failing against code I could see was
correct; `curl http://127.0.0.1:4291/js/project-management-panel.js` returned a
file with none of my changes in it. This is exactly the failure the config's own
comment warns about.

Every number above was therefore produced on **port 4381**, verified free first:

```
XNAUT_TEST_PORT=4381 npx playwright test
```

Use 4291 only after checking nothing else holds it:
`lsof -nP -iTCP:4291 -sTCP:LISTEN`.

## Mutation evidence

Four mutations, each reverting one behaviour this ticket added, each restored
and re-proved green afterwards.

**1. JS — restore the stage-1 default.** In `currentStageOf`, put back the
`Math.max(0, …)` that made an unknown stage into stage 1:

```
7 failed, 4 passed
  a project with no stage field at all shows no stage at all
  a project with an empty stage shows no stage at all
  a project with a stage no flow defines shows no stage at all
  a project with a stage belonging to another flow shows no stage at all
  the Artifacts tab offers no current document when there is no stage
  the NAUT-Flow rail says "Not started" instead of putting an unstaged project on stage 1
  the NAUT-Flow rail shows the real position once the project is in a flow
```

Restored (`diff` against the pre-mutation copy reported no difference) →
`11 passed (8.5s)`.

**2. Rust — restore `#[serde(default = "default_project_stage")]` on the field:**

```
test result: FAILED. 33 passed; 2 failed
  project_management::tests::a_manifest_without_a_stage_is_in_no_flow
  project_management::tests::a_null_stage_is_read_as_no_flow_rather_than_refused
```

**3 + 4. Rust — re-stamp the import, and swallow the flow exit.** `stage:
default_project_stage()` back in `import_task_projects`, and `Some("") =>
Ok(rebased)` in `next_project_stage`:

```
test result: FAILED. 31 passed; 4 failed
  project_management::tests::a_manifest_without_a_stage_is_in_no_flow
  project_management::tests::a_null_stage_is_read_as_no_flow_rather_than_refused
  project_management::tests::an_explicit_empty_stage_takes_a_project_out_of_the_flow
  project_management::tests::imported_and_migrated_projects_carry_no_stage
```

Restored (`diff` clean) → `1048 passed; 0 failed; 45 ignored`.

## Verifying by hand

1. `cd src-tauri && cargo tauri dev` (a JS-only change ships stale from a
   release build; use dev for UI work).
2. Open **Projects** and click a project that was **imported** rather than
   created through the New Project form — one whose `project.json` under
   `~/.xnaut-control/projects/<KEY>/` has `"stage": ""`, or no `stage` key.
   - The hero shows **no stage chip**.
   - There is **no** "Current stage" band and **no** "Primary artifact" band.
   - The **Artifacts** tab says the project is not in NAUT-Flow instead of
     offering "Open current document".
   - The **NAUT-Flow** tab's rail header reads **Not started**, and no stage in
     the rail is ticked or marked current.
3. In that project's **NAUT-Flow** tab, press **Approve & promote**. The project
   now has a real stage: the hero chip, the Current stage band and the Primary
   artifact band all appear, and the rail count becomes `2 / 15`.
4. Open a project that already has a stage (e.g. `"stage": "build"`) and confirm
   nothing regressed: chip reads `build`, the Current stage band reads
   "Build · 12 of 15 · Deliver", the artifact row points at that stage's Vault
   document.
5. Clear a project's owner in **Settings** and save: the **Contributors** band
   disappears rather than reading "Unassigned".
6. Nowhere on the page is there a "Quality gate · 0/3" or a "Ticket readiness"
   bar.

## Not done, deliberately

- **No migration of existing `project.json` files.** Projects already on disk
  carry `"stage": "idea"` from before this change and will keep showing that
  stage; the fix stops new imports acquiring one. Rewriting existing records is
  a data migration with its own blast radius and was not asked for. Clearing the
  field by hand, or a flow reset in the UI, both work today.
- **`pm_project_create` still stamps a starting stage.** Creating a project
  through the form is opting in; that is the intended behaviour, not an
  oversight.
- **No UI affordance for "leave NAUT-Flow".** The backend now accepts an empty
  stage over `pm_project_update`, so leaving is representable and reachable from
  a caller, but no button in the panel sends it. Adding one is a design decision
  beyond this ticket.
- **`Connected systems` still lists only the source repository** — that is
  already the one row backed by real config, and the Vault / Control repo /
  Engram rows the ticket complained about had been removed before this work
  started.
- **Clippy** reports 4 pre-existing errors in `project_management.rs` and 47
  across the crate. Unchanged by this work: the count in this file was 4 before
  and 4 after (checked by stashing). Not touched — out of scope.

## Scope note

The ticket named `project-management-panel.js` and the dispatch limited
production changes to the files a ticket names. The backend half was raised with
the owner through the Mesh inbox before it was written
(`in-9ab18df4-8209-4933-a084-1e41b23859ca`), because a frontend-only fix would
have been correct and permanently latent. The owner chose
**"Also make the stage optional in project_management.rs"**.
