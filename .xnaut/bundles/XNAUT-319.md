# XNAUT-319 — The swarm lane: a branch where gates are off and errors are expected

Branch `agent/claude/xnaut-319`, worktree
`/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-319`,
machine `tron.candoo`.

## What changed

A named branch, `swarm`, and a lane that merges to it with no gate. A ticket
is on the lane when it carries the `swarm` **tag** — the owner's to set, and
XNAUT-316's `child_request` already copies a parent's tags, so dividing a lane
ticket fills the lane without anybody re-deciding where the work belongs.

| File | What |
|---|---|
| `src-tauri/src/swarm.rs` | **new.** The whole lane: `lane_of`/`is_swarm`, `route`, `merge`, `publish`, `lane_ticket_for`, `lane_tickets`, `schedule` |
| `src-tauri/src/sweep.rs` | the inline jury condition replaced by one `match crate::swarm::route(..)` |
| `src-tauri/src/jury_signoff.rs` | `start` refuses a lane ticket (`no_jury_reason`) |
| `src-tauri/src/plan_review.rs` | no plan gate on the lane: the plan is written, `approved` returns immediately, nobody is asked |
| `src-tauri/src/throughput.rs` | per-lane numbers: `Lanes {audited, swarm}`, `collect_lanes`, `verifies_in`, `branch_exists`, `error_rate` |
| `src-tauri/src/mobile.rs` | `/api/control/doctor`'s `throughput` now carries both lanes |
| `src-tauri/src/main.rs` | `mod swarm;` |

Against the ticket's four properties:

- **The jury and sign-off do not run.** The sweep routes a lane ticket's green
  record to `swarm::schedule`, never `jury_signoff::schedule`; and
  `jury_signoff::start` refuses a lane ticket outright, so the gate is off in
  both places rather than in one.
- **A worker merges its own green build directly.** A passing verification
  already moves a ticket to `complete` (green IS the approval,
  `sandbox_verify::PASSED_STATUS`). On the lane that same record publishes a
  merge commit onto `swarm`, authored by NautBot, built in a scratch clone.
- **A red build is fixed by the next worker rather than reverted.** No
  integration verification after a lane merge, and no revert path in the
  module at all. The next worker's merge lands on top.
- **Nothing reaches dev without a person asking.** No path in `swarm.rs`
  writes the integration branch; `merge` refuses if the lane and the
  integration branch are ever the same ref; `promote_branch` is never
  consulted, so `uat` is unreachable too.

The lane's plan gate is the one thing the ticket did not name and that had to
be closed anyway: `POST /v1/plan/review` from a lane worktree used to open the
two-reviewer gate, and every failure inside it parks on the owner — the
serialisation the lane exists to remove, arriving through the one door left
open.

## Test results

Both suites run in this worktree on tron.candoo, 2026-09-10.

```
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
test result: ok. 1083 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out; finished in 13.27s

XNAUT_TEST_PORT=4291 npx playwright test
  150 passed (2.7m)
```

XNAUT_TEST_TOTALS={"rust":[{"passed":1083,"failed":0,"ignored":45}],"ui":[150]}

New tests (7):

- `swarm::tests::a_swarm_lane_ticket_never_opens_a_jury_job`
- `swarm::tests::the_signoff_gate_refuses_a_swarm_ticket_through_the_real_start_path`
- `swarm::tests::a_merge_on_swarm_never_touches_dev`
- `swarm::tests::the_lane_has_no_plan_gate_and_asks_nobody`
- `swarm::tests::lane_tickets_reads_the_tag_off_the_board`
- `throughput::tests::the_lanes_numbers_appear_beside_the_audited_lanes`
- `throughput::tests::only_a_finished_verification_inside_the_window_is_a_build_result`

## Mutation evidence

Two mutations, each caught by the test written for the behaviour, the code
restored, the full suite green again.

**1. The lane arm removed from the router** — `swarm.rs::route`, `if is_swarm(t)`
replaced by `if false`:

```
running 1 test
test swarm::tests::a_swarm_lane_ticket_never_opens_a_jury_job ... FAILED

thread 'swarm::tests::a_swarm_lane_ticket_never_opens_a_jury_job' panicked at src/swarm.rs:409:9:
assertion `left == right` failed
  left: Jury
 right: Swarm

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1127 filtered out
```

**2. The lane publishing onto the integration branch** — `swarm.rs::merge`,
`let reference = format!("refs/heads/{branch}")` replaced by
`format!("refs/heads/{integration}")`:

```
running 1 test
test swarm::tests::a_merge_on_swarm_never_touches_dev ... FAILED

thread 'swarm::tests::a_merge_on_swarm_never_touches_dev' panicked at src/swarm.rs:464:9:
assertion `left == right` failed: dev moved
  left: "2f9c3687f9e3bb7031a1211e05e5916268a5b2f7"
 right: "9cec09c21ad39a32419218124fc7ec3fdcabb81e"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1127 filtered out
```

Both restored; `cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut`
back to 1083 passed, 0 failed, 45 ignored.

## How to verify by hand

```bash
cd /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-319

# The two suites
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
npm ci && XNAUT_TEST_PORT=4291 npx playwright test

# Just the lane
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut swarm::
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut throughput::
```

Re-run either mutation by making the one-line edit named above and running the
single test; both should fail exactly as quoted.

To see the lane on a real board: add the tag `swarm` to a ticket, let it
verify green, and watch `swarm::schedule` publish `refs/heads/swarm` while
`refs/heads/dev` does not move. The doctor then shows both lanes:

```bash
curl -s -H "X-Xnaut-Session: $XNAUT_HOOK_TOKEN" \
  http://127.0.0.1:51737/api/control/doctor | jq .throughput
# {"audited": {...,"branch":"dev"}, "swarm": {...,"branch":"swarm","error_rate":…}}
```

## Deliberately not done

- The lane is never merged into dev by anything: "nothing reaches dev without
  a person asking" is the absence of a path, not an approval flow.
- No UI for the lane numbers — they are on `/api/control/doctor` only.
- No sampled audit. That is what the lane's error rate is *evidence for*;
  taking it now would assume the result.
- The `swarm` branch is not pre-created on Forgejo; the lane opens itself at
  the first ticket's work.
