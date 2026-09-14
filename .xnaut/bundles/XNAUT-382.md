# XNAUT-382: issue intake, where an issue becomes an inbox ticket linked both ways

Branch `agent/claude/xnaut-382`, worktree
`/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-382`,
off `bf55e17`.

## What changed

On the morning of 2026-09-14 GitHub #75 was copied into XNAUT-363 by hand: read
the issue, retype the body, invent a type, file it. Nothing ever told #75's
author where it went. All of that is now one module riding the sweep.

Per project that has intake switched on, once a tick:

1. List the source's issues newer than the stored cursor.
2. Keep the ones the project's **trigger** wants: every issue, or only those
   carrying a label (`xnaut` by default, per project).
3. File one **inbox** ticket each: type from the labels, the reporter's body
   plus a provenance line, `source_id` naming the issue, **no owner and nothing
   dispatched**. An issue is a request, not an assignment.
4. Comment `Tracked as XNAUT-N in xNAUT.` on the issue, once.
5. Mirror each tracked ticket's status back as an `xnaut:<status>` label.

**Never twice, by two guards doing different jobs.** The *cursor* is a
high-water mark stored beside the agent registry; it bounds what a tick
considers and is what makes a quiet tick cheap (one list call, no writes).
`source_id` is the correctness guard: an issue that already has a ticket never
gets a second one, whatever the cursor says. Losing the state file therefore
costs a re-list, not a duplicate board. The cursor advances past issues the
trigger *rejected* too, or an unlabelled issue is reconsidered every three
minutes forever.

**One trait, three dialects.** `IssueSource` is `origin` / `display_name` /
`list_since` / `comment` / `mirror_status`. `ForgeSource` covers Forgejo, GitHub
and GitLab through `forges.rs`; `LinearSource` is GraphQL over `api.linear.app`.
Jira needs a fourth impl and nothing else. `BoxFuture` rather than `async fn` in
the trait, because the tick holds `Box<dyn IssueSource>`.

**Deciding is pure, talking is not.** Everything that can be wrong about intake
(trigger, both guards, type, body, where the cursor ends up) is in `plan()`,
which takes no network, no clock and no app handle, so it is a table test.

Two dialect traps that are silent when wrong, both handled and both tested:

- **Gitea/Forgejo takes label IDS**, not names, and creates nothing; a name it
  does not know is dropped with a 200. `set_issue_labels` resolves each name and
  creates the missing one first. GitHub takes names and creates them itself.
- **Linear answers 200 with an `errors` array.** A status-only check reads a
  refusal as success and then finds no issues, forever. Its personal API key
  also goes in `Authorization` **raw**; a `Bearer` prefix is a 400 that reads
  like a bad key.

**A bug fixed on the way.** `forges::api_base` was "unless the URL says
`api.github.com`, use `api.github.com`", which sent every GitHub **Enterprise**
request to github.com, which is the wrong company's API. It now rewrites only the two
spellings of the web host, which is also what lets the tests point a `github`
host at a recorded-payload server on localhost.

**Only the fleet machine ticks.** The board is shared and intake WRITES to it,
and its never-twice guard is a `source_id` already on that board. Two sweeps
ticking at the same moment would both read a board with no ticket for the issue
and both file one, because each machine only pushes after it writes. So the
tick is gated on a new `Role::files_issues()`, fleet only, sitting beside
XNAUT-370's `dispatches()` and `verifies()` in the same one-place capability
table. The pane's **Run now** is deliberately NOT gated: a person pressing it on
their own desk is attended, and it is how a workstation pulls an issue in.

**The surface.** Settings → Issue Intake lists every project that has somewhere
to read issues from, with the toggle, the trigger, the label, a Linear team box
and the Linear API key. It says how many issues each project tracks and where
its cursor is, and for a project that is on but idle it says *why*. **Rescan**
is a button rather than a hidden behaviour: the cursor is a high-water mark, so
an issue labelled after it was created sits below it, and the escape hatch has
to be something a person can press.

## Files

| File | What |
|---|---|
| `src-tauri/src/issue_intake.rs` | New, 2365 lines: 1225 of code and 28 tests. The trait, both sources, the pure core, the cursor store, the tick, three commands. |
| `src-tauri/src/forges.rs` | `list_issues_for` / `get_issue_for` / `add_issue_comment_for` (explicit owner); `set_issue_labels` + `ensure_label_id`; `parse_remote` / `host_for_remote`; the Enterprise `api_base` fix. |
| `src-tauri/src/project_management.rs` | `ProjectRecord.issue_intake`; `TicketCreateRequest.source_id`; `set_issue_intake_in`. |
| `src-tauri/src/settings.rs` | `LinearSettings { api_key, endpoint }`. |
| `src-tauri/src/sweep.rs` | `issue_intake_beat` on the tick, gated on the role, with once-only blocked reporting. |
| `src-tauri/src/instance.rs` | `Role::files_issues()`, fleet only, and its row in the capability-table test. |
| `src-tauri/src/main.rs`, `src-tauri/permissions/default.toml` | Module + three commands, registered and granted. |
| `src-tauri/gen/schemas/acl-manifests.json` | Regenerated by the build. |
| `src/js/issue-intake-settings.js`, `src/index.html`, `src/js/app.js` | The pane, its nav entry and its call site. |
| `tests/issue-intake.spec.mjs` | 7 UI tests. |
| `agent_tools.rs`, `core_team.rs`, `subdivide.rs` | `source_id: String::new()` on their `TicketCreateRequest` literals. |

## How to verify by hand

```bash
# The whole Rust suite.
cargo test --manifest-path src-tauri/Cargo.toml

# Intake alone: the pure core, plus both forges and Linear against recorded
# HTTP, plus the end-to-end tick against a real control repo.
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut issue_intake

# The pane.
XNAUT_TEST_PORT=4291 npx playwright test issue-intake
```

In the running app: **Settings → Issue Intake**. Pick a project with a forge
remote, tick *Take issues in*, leave the trigger on *Only issues with a label*,
**Save**, then **Run now**. The pane reports what it filed. On the board the new
ticket is `inbox`, unowned, typed from the issue's labels, with
`source_id: github:<owner>/<repo>#N` and a body ending `From GitHub #N by
<author>`. On the forge the issue carries a `Tracked as …` comment and an
`xnaut:inbox` label. Press **Run now** again: `Filed nothing; 1 already
tracked.` Move the ticket to `in_progress` and press it once more: the label on
the issue becomes `xnaut:in_progress` and the repo's own labels are untouched.

To see the cursor's escape hatch: label an *older* issue and press **Run now**
(nothing happens, because it is below the cursor), then **Rescan** (it arrives).

## Totals

Both from runs in this worktree on 2026-09-14.

- Rust: **1280 passed, 0 failed, 45 ignored**. The 45 are the suite's standing
  `#[ignore]` set, untouched here; nothing in this change is ignored or
  skipped. 30 of the 1280 are new (28 in `issue_intake`, 2 in `forges`), so
  1250 pre-existing tests still pass; `instance.rs`'s capability-table test
  grew three assertions rather than becoming a new test.
- UI: **243 passed, 0 failed**. 7 are new (`tests/issue-intake.spec.mjs`), so
  236 pre-existing tests still pass.

`tests/observatory-sessions.spec.mjs:197` timed out once on the first full UI
run and passed on the re-run and in isolation. It is a 30s click timeout in a
3-minute suite and touches nothing in this change.

`node scripts/hygiene-check.mjs` reports 2 failures: unattributed test fns
(`foundation.rs::the_handback_route_gets_a_real_url`,
`main.rs::print_startup_banner`, `veto.rs::a_call_that_cannot_be_recorded_is_refused`)
and `vault-pane.js:491 'path' is not defined`. None of those four symbols is
touched by this change, and the XNAUT-370 bundle records the identical two
failures one commit earlier. Its "81 commands registered but never invoked"
warning does not name any of the three added here.

XNAUT_TEST_TOTALS={"rust":[{"passed":1280,"failed":0,"ignored":45}],"ui":[243]}
