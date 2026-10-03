# XNAUT-439 — a compile is work: the stall detector now counts CPU, ticket edits and verify evidence

An agent fifteen minutes into `cargo test` on a cold worktree was marked
`failed` with `stalled: alive but capture, hooks and commits show no progress
beyond the window`. It was working. The detector was measuring the wrong things.

## What was wrong

`run_control::verdict` decided progress from two signals:

```rust
let progressing = grew || (!proof.commit.is_empty() && proof.commit != run.last_commit);
```

A long compile moves neither. It writes nothing to the capture file, fires no
hook, and makes no commit until it finishes. So run `01M354JH4E9TVBGJ36H7SNJC5G`
(XNAUT-86, claude) read `failed` in the Observatory and on the board while its
agent compiled and then wrote the results into the ticket body. The registry
deferred the ticket return — `process/session/capture has not proved the writer
stopped` — so the ticket survived, but the run did not. The same window failed
XNAUT-379's first run on 2026-09-14.

## What changed

Progress is now six signals, not two. The three new ones are the three places a
compile IS visible, exactly as the ticket specifies:

| Reading | Source | Moves during |
|---|---|---|
| `cpu_ms` | accumulated CPU of the run's **child** processes | cargo, rustc, node, playwright |
| `ticket_revision` | the run's own ticket record in the control repo | the agent writing its progress |
| `verify_log_bytes` | bytes under the worktree's `.xnaut/` tree | the bundle and verify evidence being written |

### One definition of progress, read by both callers

`verdict` and `reconcile_in` each spelled the condition out separately. At six
terms two copies drift, and a reconciler that disagrees with the verdict is how
a run gets failed and kept in the same pass. Now there is one:

```rust
pub fn progressed(&self, run: &RunManifest) -> bool {
    self.capture_bytes > run.capture_bytes
        || (!self.commit.is_empty() && self.commit != run.last_commit)
        || self.cpu_ms > run.cpu_ms
        || self.ticket_revision > run.ticket_revision
        || self.verify_log_bytes > run.verify_log_bytes
}
```

### CPU is accumulated, children-only, and a high-water mark

Three decisions, each load-bearing:

**Accumulated, not instantaneous.** A `%cpu` sample taken between two rustc
invocations reads zero and proves nothing. A total only climbs for a process
doing work, so two readings a sweep apart answer the question without having to
catch the compile in the act.

**Children only, never the run's own pid.** A polling CLI always accrues a
little CPU. Counting it would make every run look busy forever and this detector
would never fire again — an abandoned run would hold its ticket for good. That
is a worse bug than the one being fixed, so it has its own test.

**A high-water mark, not the latest reading.** A subtree's total *falls* when a
child exits — rustc finishes and its time leaves the process table. Storing the
latest reading would let the next idle tick beat the lowered mark and read as
fresh progress, forever. `run.cpu_ms.max(proof.cpu_ms)` makes a fall silence.

### Sampled once per sweep, not once per run

`reconcile_in` observes every non-done run: **121 of the 211 manifests** in this
machine's registry, 109 of them already failed. Sampling inside `observe_in`
would fork `ps` 121 times and reload settings 121 times per pass — the same
per-call work that put 116 git processes at 786% CPU on the control repo in
XNAUT-432. So:

- `Machine::sample()` takes one process table and one control-repo path per pass;
  `observe_with` reads from it. `observe_in` keeps its signature for single-run
  callers and tests.
- The three readings are skipped for a terminal run, whose verdict is `Keep` and
  never recomputed. That skip is most of the saving.
- `project_management::repo_path_now()` resolves the path **without**
  `configured_repo`'s `inspect`, which runs several git commands and counts every
  ticket file in every project.

### Also fixed: one stale UI test

`tests/workspace-entry-points.spec.mjs` filled only name and path on the new
project form and asserted the Create button was enabled. `f5a440e` had made the
repository url a third required field (a project must HAVE a repository for
results to be delivered through); `repository-transfer.spec.mjs` was written for
that rule, this test was left behind. It now fills the url. Pre-existing on the
base commit — verified by stashing the Rust changes and reproducing it — and
unrelated to the detector, but it had to be green to ship this.

## Files

| File | What |
|---|---|
| `src-tauri/src/run_control.rs` | the three readings, `progressed`, `Machine`, `observe_with`, 11 tests |
| `src-tauri/src/sweep.rs` | sample the machine once per pass, at both call sites |
| `src-tauri/src/project_management.rs` | `repo_path_now()` — the path without the git-heavy inspection |
| `tests/workspace-entry-points.spec.mjs` | fill the now-required repository url |
| `src-tauri/gen/schemas/acl-manifests.json` | build regeneration, committed separately |

## Deliberately not done

- **No attribution of a ticket edit to the run's own agent.** The control repo
  records no actor on a ticket edit — the `ticket.updated` event carries type,
  ticket, project, at, revision and title, and no writer. The reading is scoped
  to the run's own ticket, which is the closest the record allows, and it errs
  toward alive: the direction that does not kill a working agent.
- **A descendant spinning uselessly still masks a stall.** A process burning CPU
  is doing *something*, and this detector cannot tell useful work from a runaway
  loop. Accepted and documented rather than guessed at.
- **`PROGRESS_WINDOW_MS` is unchanged at 15 minutes.** The window was never the
  bug; what it measured was.
- **No `cargo fmt`.** 100+ files in this tree are not rustfmt-clean, including
  every file I touched, so the repo does not enforce it. Reformatting only my
  lines would make them inconsistent with their neighbours; reformatting the
  tree would bury this diff. The new code matches the surrounding style.

## How to verify by hand

```bash
# Both suites, from the worktree root.
cargo test --manifest-path src-tauri/Cargo.toml
npm ci                                    # a fresh worktree has no node_modules
XNAUT_TEST_PORT=4291 npx playwright test

# The regression itself, and the inverse bug it must not cause:
cargo test --manifest-path src-tauri/Cargo.toml a_compile_is_progress
cargo test --manifest-path src-tauri/Cargo.toml a_child_exiting_lowers

# The real plumbing, against a real `ps` and a real directory — this is the test
# that would have caught the whole class, since every pure test above would pass
# just as happily if `ps` took different flags on this platform:
cargo test --manifest-path src-tauri/Cargo.toml observe_reads_real_child_cpu
```

## Totals

Rust `cargo test --manifest-path src-tauri/Cargo.toml`: 1529 passed, 0 failed,
54 ignored (baseline on the base commit was 1518 passed; the 11 new tests are
mine, and no test was skipped or ignored to get here).

UI `XNAUT_TEST_PORT=4291 npx playwright test`: 399 passed, 0 failed.

XNAUT_TEST_TOTALS={"rust":[{"passed":1529,"failed":0,"ignored":54}],"ui":[399]}
