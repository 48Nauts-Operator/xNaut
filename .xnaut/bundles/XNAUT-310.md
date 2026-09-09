# XNAUT-310 — the stale-ticket triage notice fires every pass instead of once per set

Branch `agent/claude/xnaut-310`, worktree
`/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-310`,
machine `tron.candoo`, 2026-09-09.

`XNAUT_TEST_TOTALS={"rust":[{"passed":1042,"failed":0,"ignored":45}],"ui":[133]}`

## What was wrong

XNAUT-306 gave the sweep a notice listing unowned `ready`/`in_progress` tickets
too old to auto-assign. It came back three times on 2026-09-09 — 12:27, 13:28,
14:11 — with identical content.

The dedup it shipped with was per TICKET, not per notice, and it had an eraser
next to it. `tick` ended with:

```rust
if !plan.iter().any(|a| matches!(a, Action::NotifyStaleUnowned { .. })) {
    announced.stale_unowned.clear();
}
```

So any pass that planned no stale notice forgot every ticket already reported,
and the next pass that planned one treated the whole list as new again. That
gap is not rare. The stale list is filtered by `fleet.allows`, and the fleet is
rebuilt from the board on every tick:

```rust
let projects = crate::project_management::list_projects(&repo).unwrap_or_default();
let fleet: HashSet<String> = projects.iter().filter(|p| p.fleet).map(|p| p.key.clone()).collect();
```

One unreadable read, or a project's `fleet` flag off for a tick, empties the
list, clears the memory, and the owner is told the same thing again.

## What it does now

The notice is remembered as the SET of ticket ids that was actually delivered,
and it is news exactly when that set changes.

```rust
/// The ticket ids of a stale list, as a set: what the notice is ABOUT, with
/// the titles and the ordering dropped because neither changes what it says.
fn ids_of(tickets: &[(String, String)]) -> std::collections::BTreeSet<String> {
    tickets.iter().map(|(id, _)| id.clone()).collect()
}

fn stale_unowned_is_news(&self, tickets: &[(String, String)]) -> bool {
    !tickets.is_empty() && ids_of(tickets) != self.stale_unowned
}

/// Called only after the inbox write succeeds, so a failed delivery is
/// retried next pass.
fn stale_unowned_delivered(&mut self, tickets: &[(String, String)]) {
    self.stale_unowned = ids_of(tickets);
}
```

The `clear()` in `tick` is gone. An empty list posts nothing **and erases
nothing** — that second half is the actual fix, because forgetting on a gap is
what re-posted the notice.

Two consequences worth naming:

- The notice now carries the **whole** current list rather than the newcomers.
  Under the old incremental rule, three stale tickets followed by a fourth
  produced a notice naming only the fourth, which reads as if the other three
  had been dealt with. It is a standing state, so it is stated in full.
- The set comparison is over ids only. A retitled ticket is not a new notice.

Field type changed `HashSet<String>` → `BTreeSet<String>`; set equality is
what the rule is expressed in, and `BTreeSet` makes the comparison ordered and
the value cheap to reason about in a test.

## Files

- `src-tauri/src/sweep.rs` — the only production file touched (110 insertions,
  28 deletions, tests included). Nothing in the jury, the registry or the
  sweep's scheduling was changed; `TICK`, `TRIAGE_EVERY_MS` and
  `spawn_sweep_task` are untouched.

## Tests

`stale_notify_is_once_per_standing_ticket_and_retries_failed_delivery` asserted
the old incremental semantics and was replaced by three tests, one per
behaviour the ticket names:

| test | covers |
|---|---|
| `stale_notify_is_once_per_set_and_says_it_again_when_the_set_changes` | same set twice posts once (in any order); a ticket added posts again; a ticket removed posts again; a retitle is not a change |
| `an_empty_stale_set_is_never_posted_and_never_forgets` | an empty set never posts, before or after a delivery, and does not erase the memory |
| `a_failed_stale_delivery_is_retried_on_the_next_pass` | the memory records DELIVERY, not the attempt |

All pure over the id sets — no store, no `AppHandle`.

### Rust

```
$ cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
test result: ok. 1042 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out; finished in 8.83s
```

### UI

```
$ XNAUT_TEST_PORT=4291 npx playwright test
  133 passed (2.7m)
```

(The worktree had no `node_modules`; `npm install` was run first. It is
gitignored and not part of the change.)

### Lint

`cargo clippy --bin xnaut --all-targets` was already failing across the tree
before this change and still is; the count went **77 → 76** (this change
removes one, adds none). `cargo fmt --check` reports 1006 pre-existing diffs
repo-wide, none introduced here — the file's dense style is deliberate and
matches its neighbours.

## Mutation evidence

Each mutation was applied to the restored-good file, run, then reverted.

**M1 — drop the set comparison** (`stale_unowned_is_news` returns
`!tickets.is_empty()`), i.e. the every-pass bug, reintroduced:

```
test sweep::tests::stale_notify_is_once_per_set_and_says_it_again_when_the_set_changes ... FAILED
panicked at src/sweep.rs:2308: assertion failed: !announced.stale_unowned_is_news(&set)
test result: FAILED. 0 passed; 1 failed
```

**M2 — drop the emptiness guard** (`ids_of(tickets) != self.stale_unowned`
alone), so an empty list posts a notice listing nothing:

```
test sweep::tests::an_empty_stale_set_is_never_posted_and_never_forgets ... FAILED
panicked at src/sweep.rs:2343: assertion failed: !announced.stale_unowned_is_news(&[])
test result: FAILED. 10 passed; 1 failed
```

**M3 — forget on an empty list**, the old `tick`-level `clear()` moved inline:

```rust
fn stale_unowned_is_news(&mut self, tickets: &[(String, String)]) -> bool {
    if tickets.is_empty() { self.stale_unowned.clear(); return false; }
    ids_of(tickets) != self.stale_unowned
}
```

```
test sweep::tests::an_empty_stale_set_is_never_posted_and_never_forgets ... FAILED
panicked at src/sweep.rs:2347: assertion failed: !announced.stale_unowned_is_news(&set)
test result: FAILED. 10 passed; 1 failed
```

M3 is the one that matters: it is the shipped defect, and the test that catches
it is the one asserting the memory survives an empty pass.

**Restored, green:**

```
$ cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
test result: ok. 1042 passed; 0 failed; 45 ignored
```

One further mutation was tried and is recorded because it did NOT fail:
rewriting `stale_unowned_delivered` as `retain(...)` + `extend(...)` leaves
every test green. It is an equivalent mutant — `delivered` is only ever reached
with the exact list just posted, so pruning-then-extending and assignment
cannot differ. Worth knowing that the method has no reachable behaviour of its
own beyond "store this list".

## Verifying it by hand

The rule is pure, so the fastest check is the unit tests:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut stale
```

In the running app: with two stale unowned tickets on the board, the notice
"Stale unowned tickets need a person's decision" appears once and does not
return on later sweeps (every 180s, `ledger` kind `sweep_stale_unowned`). Give
one of them an owner and the next pass posts once more, listing the one that
remains. Give the last one an owner and nothing is posted at all.

## Deliberately not done

- **Nothing is persisted.** The memory lives in `Announced`, so an app restart
  re-posts the current set once. That matches every other condition the sweep
  remembers (`read_only`, `no_repo`, `triage`), and the ticket says "no store
  needed".
- **No time-based re-nag.** A set that never changes is said exactly once for
  the life of the process. If a standing list should be repeated after, say, a
  week, that is a new decision and a new ticket.
- The underlying reason `list_projects` can come back empty for a tick was not
  investigated; this change makes the notice immune to it rather than fixing
  it.
