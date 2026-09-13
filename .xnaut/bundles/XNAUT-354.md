# XNAUT-354 test bundle

Author: @claude
Date: 2026-09-13
Session: 2921f969-bde8-4b5b-85e2-e9041a8ff65d
Branch: agent/claude/xnaut-354

## Change

Two ways to put agents on a batch of tickets became one. The Multi-Agent
Manager pane planned a swarm in its own chat and fired `loom_run` directly —
no run registry, no jury, no ledger, its own model dropdown and its own
max-parallel box. NautBot started runs through `dispatch.rs`, which has all of
that. Same intent; the older one outside every control.

NautBot gains two tools, in `agent_tools.rs` beside `dispatch_ticket` and
guarded the same way (orchestrator only):

- `swarm_plan(project, tickets?)` builds a validated plan from
  `swarm_plan::build`. Real PM tickets only, one worktree per run, capped by
  `@nautbot`'s profile. Every ticket it leaves out is named with a reason
  rather than dropped. It starts nothing.
- `swarm_dispatch(plan_id)` hands every run to `dispatch::pm_ticket_dispatch`.

A plan is CONSUMED by the first yes (`swarm_plan::take`), so the card and the
chat cannot both start the same batch. A plan with one runnable ticket is
never offered — `swarm_plan::offer` returns `Single`, no plan travels to the
UI, and no card can be raised — which is the "a single-ticket request gets no
swarm question" criterion, decided before anything could reach a surface.

The plan reaches the owner as a card in NautBot's thread (event
`swarm-plan-proposed`, rendered by `agent-space.js`, confirmed with the
`swarm_plan_dispatch` command). Max parallel moved from
`settings.loops.max_parallel_runs` to `AgentProfile.max_parallel`: starting a
batch is the orchestrator's act, and a global setting could not say whose
limit it was. The executor model stays per-OWNER, where dispatch already reads
it, instead of one dropdown overriding every run in a batch.

`multiagent-pane.js`, `window.xnautSwarm`, the right-pane rail entry, the
Observatory's "Initialize Multi-Agent" button and the phone bridge's
`/api/manager*` routes (with `mobile_manager_publish` and
`AppState.mobile_manager`) are gone. The Observatory's band reads
`run_registry_list` and groups dispatched runs by project, so a swarm survives
a webview reload and is visible however it was started.

The NautFlow Build stage's SANDBOX runtime borrowed that engine and is not a
swarm over tickets — its slices are synthetic spec slices with no PM ticket for
`dispatch.rs` to read, so they could not route through the new tools even in
principle. Asked which way to take it, André chose "keep it: move the loom
launcher into the Build stage's own file", so it is `build-sandbox.js`,
publishing `window.xnautBuild`.

### Two pre-existing failures fixed on the way

Neither is XNAUT-354's doing; both blocked this ticket's "zero failures" bar.

1. `jury_signoff::tests::a_project_without_an_approval_toml_refuses_instead_of_running_our_build`
   failed on the branch base. `jury_runtime::policy` falls back to
   `~/.config/xnaut`, so a fixture that deliberately removes a project's
   `approval.toml` found the DEVELOPER'S OWN and refused with "policy changed
   after sign-off" instead. It passed only on a machine that had never run
   xNAUT. It gets the same thread-local test seam the vault and the inbox
   already have (`jury_runtime::use_test_config`).
2. Twenty-eight Playwright tests failed on the branch base. Nineteen clicked
   `getByText('Agent Space')`, a label the rail fold (XNAUT-337/342) moved
   behind the "More surfaces" menu; eight more in `actions-rows.spec.mjs`
   shared that helper, and `headless-profile.spec.mjs` pinned a
   `headlessAgentCommand` call shape that XNAUT-355 changed. The specs now
   open the More menu first — the same pattern seven already-passing specs
   use — and the pin names the `handle` argument XNAUT-355 added. Agent Space
   itself was verified reachable by hand before any spec was touched: this was
   a stale selector, not a product regression.

Files:

- src-tauri/src/swarm_plan.rs (new)
- src-tauri/src/agent_tools.rs
- src-tauri/src/agent_profiles.rs
- src-tauri/src/dispatch.rs
- src-tauri/src/run_control.rs
- src-tauri/src/jury_runtime.rs
- src-tauri/src/jury_signoff.rs
- src-tauri/src/settings.rs
- src-tauri/src/state.rs
- src-tauri/src/mobile.rs
- src-tauri/src/main.rs
- src-tauri/src/composer.rs
- src-tauri/permissions/default.toml
- src-tauri/gen/schemas/acl-manifests.json
- src/js/build-sandbox.js (new)
- src/js/multiagent-pane.js (deleted)
- src/js/agent-space.js
- src/js/observatory-panel.js
- src/js/right-pane.js
- src/js/project-management-panel.js
- src/js/buildrun-pane.js
- src/js/right-pane-buildlog.js
- src/js/right-pane-buildfiles.js
- src/js/sidebar.js
- src/index.html
- tests/nautbot-swarm.spec.mjs (new)
- tests/scale-30-agents.spec.mjs
- tests/headless-profile.spec.mjs
- tests/agent-space.spec.mjs
- tests/actions-rows.spec.mjs
- tests/canvas.spec.mjs
- tests/exe-computer.spec.mjs
- tests/harness-switch.spec.mjs
- tests/static-server.mjs
- .xnaut/bundles/XNAUT-354.md

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml`: exit 0; 1187 passed, 0
  failed, 45 ignored, 0 filtered. Six of them are new, in `swarm_plan::tests`.
- `XNAUT_TEST_PORT=4391 npx playwright test`: exit 0; 218 passed, 0 failed,
  3.0m. Three are new, in `tests/nautbot-swarm.spec.mjs`.
- `npx eslint` on every JavaScript file this ticket touched: exit 0. The repo
  as a whole has 57 pre-existing `npm run lint` errors in files this ticket
  does not touch (including a duplicate `agent` key in `right-pane.js` that
  predates it).
- `cargo clippy --manifest-path src-tauri/Cargo.toml --bin xnaut`: no warning
  in `swarm_plan.rs`, `dispatch.rs` or `jury_runtime.rs`. 54 warnings elsewhere
  are pre-existing.
- `rustfmt --edition 2021 src-tauri/src/swarm_plan.rs` applied; the repository
  as a whole is not rustfmt-clean (1255 `cargo fmt --check` diffs at the branch
  base, in files this ticket does not touch), so it was not reformatted.

### A note on the port, and on a run that had to be thrown away

`XNAUT_TEST_PORT=4391`, not 4291. Playwright's `webServer` sets
`reuseExistingServer: true`, and worktree `agent-claude-xnaut-356` held a
server on 4291 for most of this session. The first full UI run here attached to
it and therefore tested ANOTHER WORKTREE'S FRONTEND: it reported 32 failures,
including three for features whose code was never loaded. That result is
discarded, and it is exactly the hazard `playwright.config.mjs` documents above
`const PORT`. Every number quoted here comes from a run on a port checked free
with `lsof -nP -iTCP:<port> -sTCP:LISTEN` first.

### Baseline, so the deltas are checkable

Measured on the branch base (31c581b) in this worktree, on its own free port:

| Suite | Base | Now |
| --- | ---: | ---: |
| `cargo test` | 1186 passed, 1 failed | 1187 passed, 0 failed |
| `npx playwright test` | 187 passed, 28 failed | 218 passed, 0 failed |

## How to verify by hand

1. `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut swarm_plan`
   — the six rules, without an app: only real tickets, every skip named, the
   cap bounding and naming what it dropped, one worktree per run, one runnable
   ticket is not a swarm, and a plan consumed by the first yes.
2. Pick a free port, then
   `XNAUT_TEST_PORT=<port> npx playwright test tests/nautbot-swarm.spec.mjs`
   — the card appears, dispatches NOTHING until the button is pressed, then
   sends the plan id; the Observatory band reads the registry; nothing reaches
   the retired pane.
3. In the app: ask NautBot "work on all open tickets for XNAUT". A plan card
   should list only tickets the PM has, name every one it skipped, and start
   nothing until you press Dispatch. Ask it to work one named ticket and no
   swarm question should appear at all.
4. `grep -rn "xnautSwarm\|multiagent-pane" src/` — one hit, the sentence in
   `build-sandbox.js`'s header explaining where the engine came from.

## Deliberately not done

- The Build stage's sandbox runtime still fires `loom_run` outside the
  registry, the jury and the ledger. It is a smaller surface than before (one
  caller, no chat, no model dropdown of its own) but it is the same engine, and
  rebuilding it on `dispatch.rs` needs synthetic slices to become something
  dispatch can read. That is a ticket, not a footnote.
- `tests/control-inventory.json` still lists the retired pane's controls. It is
  a generated artifact (`tests/enumerate-controls.mjs`), not asserted by any
  spec, and regenerating it is a separate run.

XNAUT_TEST_TOTALS={"rust":[{"passed":1187,"failed":0,"ignored":45}],"ui":[218]}
