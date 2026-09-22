# XNAUT-380 — sign-off accepts a not-done list the ticket already names

Branch `agent/claude/xnaut-380`, worktree
`/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-380`.

## What changed

The sign-off gate used to send ANY not-done item in a handback to the owner.
On the 2026-09-14 autonomy run (XNAUT-370) that was the single human click in
an otherwise unattended loop — dispatch 08:48, done 09:15, verify green
09:24:45, escalated 09:24:50 with "unfinished work has not been accepted by the
ticket", approved 09:25:21 — and it fired on work the ticket itself listed as
out of scope. The agent was escalated for obeying its ticket.

Now the gate reads both sides. An item the ticket body or a linked design
document already declares out of scope is accepted and named on the record;
an item nothing declares still escalates, and the card names only that item.

- **`src-tauri/src/signoff_scope.rs`** (new). The comparison, pure and unit
  tested. `not_done_items` splits a handback's `not_finished` on newlines,
  bullets and semicolons, and drops clauses that only say what an item waits
  on ("waits on a signing cert" is a dependency, not scope).
  `declared_out_of_scope` reads both forms that are real in this repository: a
  heading (`## Not in scope`, or the shouted `NOT IN SCOPE` a dispatch body
  writes) whose paragraphs run to the next heading, and the inline
  `Deliberately not done: …` sentence every design doc in the vault actually
  uses, wrapped across lines. `accounts_for` matches on naming words —
  lowercased, filler dropped, lightly stemmed — and requires a declaration to
  carry 60% of an item's words, and at least two of them.
- **`src-tauri/src/jury_signoff.rs`**. `evidence_reason` became `evidence`,
  returning both the refusal and the accepted list. `not_done_verdict` holds
  the new rule and keeps the hand-written `Accepted not_finished: <exact>`
  escape hatch, which now overrides even the policy switch. `start` puts the
  accepted items on the job as a note.
- **`src-tauri/src/jury.rs`**. `Policy::escalate_every_not_done` (default
  false) keeps today's always-escalate behaviour for a project that wants a
  person to see each cut of scope. `Job::notes` carries lines the gate wants
  on the record whatever the verdict. The sign-off rubric the reviewers read
  now states the new rule, or both reviewers would refuse what the gate
  accepts.
- **`src-tauri/src/jury_runtime.rs`**. `run_job` appends the notes to the
  reason AFTER the verdict, because the memory entry and the Delivery page
  both read the first line of a reason and that line has to be the decision.
- **`.xnaut/approval.example.toml`**, **`src-tauri/src/main.rs`**: the switch
  documented, the module registered.

The asymmetry is the load-bearing part: containment runs item → declaration
and never the other way, and anything unmatched escalates. Accepting scope
nobody declared is the failure that matters; escalating something already
covered is only the noise we started with.

## Tests

`cargo test --manifest-path src-tauri/Cargo.toml` — 1262 passed, 0 failed,
45 ignored (the ignored set is the pre-existing live-proof tests).
`XNAUT_TEST_PORT=4291 npx playwright test` — 236 passed.

New Rust tests, all green:

- `signoff_scope::tests` — covered, uncovered and mixed lists; `nothing`;
  a wrapped inline declaration read as one; a mere mention of scope declaring
  nothing without its colon; the hand-written acceptance line; a section
  ending at the next heading; plurals and tenses; shared vocabulary alone not
  being enough.
- `jury_signoff::tests::declared_unfinished_work_is_accepted_and_the_rest_still_escalates`
  — the same three cases through the real evidence rule, plus the policy
  switch and the hand-written override.
- `jury_runtime::tests::a_note_is_recorded_after_the_verdict_and_never_in_front_of_it`.

## Verify by hand

```bash
cd /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-380
cargo test --manifest-path src-tauri/Cargo.toml
XNAUT_TEST_PORT=4291 npx playwright test          # needs npm ci first
cargo clippy --manifest-path src-tauri/Cargo.toml --bin xnaut --all-targets
```

The behaviour, without a jury run: a ticket whose body carries

```
NOT IN SCOPE
- the Windows leg, which waits on a signing certificate
```

and a handback with `not_finished: "the Windows leg is untested; waits on a
signing cert"` produces no owner card, and the jury reason ends with
`Declared out of scope by the ticket: the Windows leg is untested`. Add
`the SSH importer is not written` to that handback and the card comes back
naming only the SSH importer.

## Deliberately not done

- No UI for the accepted list. It reaches the Delivery page already, inside
  the jury reason the page renders; a separate field would be a second place
  to keep in step with the first.
- No fuzzy matching of the ticket's ACCEPTANCE section against the diff. This
  ticket is about not-done items, and reading acceptance criteria is the
  reviewers' job.
- The 60% threshold is a constant, not policy. A project can turn the whole
  rule off with `escalate_every_not_done`; tuning how fuzzy the match is would
  be a knob nobody has yet asked for.

XNAUT_TEST_TOTALS={"rust":[{"passed":1262,"failed":0,"ignored":45}],"ui":[236]}
