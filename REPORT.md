# XNAUT-192: Plan Canvas

A plan an agent hands over renders in the Plan pane, André clicks the block he
means and attaches a numbered note to it, and Approve or Request changes
answers the agent that is blocked on the reply.

Branch `agent/xnaut-192`. Committed locally, not pushed, `main` untouched.

## What I changed

**New: `src-tauri/src/plan_review.rs`** (+ routes in `agent_hooks.rs`)

- `POST /v1/plan/review` writes the plan to `<project>/PLAN.md`, clears the
  previous round's notes for that file, files an inbox `approve` item carrying
  `context.plan_project` / `context.plan_file`, emits `plan-review` so the pane
  opens on it, then blocks on `inbox::wait_for_answer`. It answers
  `{id, decision, plan_path, notes: [{n, lines, quote, text}]}`.
- `GET /v1/plan/review/:id` gives the same verdict shape, for an agent re-issuing
  the wait after a long-poll expires. A plan left on screen overnight is the
  normal case; `MAX_WAIT_MS` is 300s.
- Both routes are registered on the parking router next to `/v1/inbox/*`, not
  the 5s one.
- `decision_from_status` maps `approved -> approved`, `denied ->
  changes_requested`, anything else -> `pending`.

**Changed: `src/js/plan-pane.js`**

- A third mode, Review, alongside Preview and Edit, and the default. The plan is
  split into blocks (`splitBlocks`) and each rendered into its own
  `.plan-block[data-start][data-end]`.
- Clicking a block opens a composer; saving writes a `notes.rs` annotation with
  `newRange = [block.start, block.end]`, `source: "user"`, against the plan
  file's relative path. Notes render back on their block, numbered, with a
  remove button.
- A review bar with Approve plan / Request changes, wired to `inbox_decide`
  with `approved` / `denied`. It appears from the `plan-review` event, or from
  an already-open `approve` item found via `inbox_list` when the tab is opened
  by hand.
- A `plan-review` event lands in the pane already showing that plan; only if
  none is open does it attach a new tab.

**Changed: `src-tauri/src/foundation.rs`**. Teaches `/v1/plan/review` next to
the inbox routes, VERSION v1 -> v2 per that file's own rule. A route no agent
is told about is a route no agent calls.

**Changed: `src-tauri/src/inbox.rs`**. `wait_for_answer` is now `pub(crate)`.
No behaviour change.

## Design decisions, and why

**Anchors are `notes.rs` annotations, unchanged.** `Annotation.newRange` is an
inclusive 1-indexed line range, which is exactly what a block of markdown is. A
plan note and a diff note are the same record; the diff pane needed no change.

**A fence is one block.** `splitBlocks` swallows everything from an opening
``` to its closer, so a note on a SQL statement or a mermaid diagram anchors to
the whole thing rather than to a stray backtick line.

**Notes are numbered down the plan, not by click order.** Both sides sort by
start line (`numberNotes` in the pane, `collect_notes` in Rust). If they
disagreed, "see note 2" would name two different notes across the handover.

**`denied` becomes `changes_requested`.** The inbox vocabulary is
approved/denied because it was built for tool calls. A plan sent back with
notes is not rejected work, and an agent acting on "denied" would abandon
rather than revise.

**A new round clears the last round's notes.** They were already delivered with
that round's verdict; kept, they would anchor to lines the revision moved and
point the agent at the wrong paragraph. Notes that drift because André edited
the plan under them are shown separately rather than hidden, because they are
still going to the agent.

**Nothing from ECC's transport was ported.** Credit is in the header of
`plan_review.rs` and `plan-pane.js` in the `gate_score.rs` form: ECC
(github.com/affaan-m/ecc, MIT), `docs/design/plan-canvas.md`, crediting
lavish-axi by @kunchenguid. Their loopback server, DNS-rebinding guard,
bundled markdown renderer and pinned Mermaid CDN all exist so a CLI can borrow
a browser. xNAUT is the browser. Both departures are recorded in the file
headers and in the vault doc.

## What I proved

| Check | Result |
|---|---|
| `cargo test --bin xnaut` | 505 pass, 0 fail (9 new: 8 `plan_review::tests`, 1 `foundation::tests`) |
| `cargo clippy --bin xnaut` | 23 warnings, all pre-existing, none in any file I touched |
| `npx playwright test` | 96 pass, 0 fail (5 new in `tests/plan-canvas.spec.mjs`) |
| `node scripts/mutation-check.cjs` | 20/20 caught, including 2 new XNAUT-192 entries |
| `node scripts/mutation-check.cjs --all` | 33/35 caught; the third XNAUT-192 entry caught, the two BASELINE lines explained below |

The Playwright spec drives the real pane against the Tauri stub with a
notes store that actually remembers writes, because every part of this is a
seam: which lines a click anchors to, which command the note is written with,
and which word the verdict sends. It asserts:

- the four blocks of a plan with a fenced block resolve to `[[1,1],[3,3],[5,7],[9,9]]`,
  and a note on the fence is written with `newRange: [5,7]` to `PLAN.md`;
- two notes clicked bottom-up render numbered 1 and 2 top-down;
- Request changes invokes `inbox_decide({id, decision: 'denied'})` and Approve
  invokes `'approved'`, and the buttons go away with the question;
- a `plan-review` event is adopted by the open pane rather than stacking a tab;
- a note whose lines the plan lost is still shown.

Mutations added to `scripts/mutation-check.cjs`, each confirmed red:

- `XNAUT-192 a fenced block in the plan splits into pieces`. Breaks the fence
  branch of `splitBlocks`; caught by "a click anchors a note to the plan lines
  that block came from".
- `XNAUT-192 Request changes answers the agent with approved`. Caught by
  "Approve and Request changes answer the waiting agent".
- `XNAUT-192 an unanswered plan reads as approved`. Flips `decision_from_status`'s
  fallback; caught by `cargo test --bin xnaut plan_review::tests` (slow, `--all`).

## What I could not do, and why

**The HTTP round trip is not proved end to end.** The two ends are: the pure
verdict and note-collection functions (Rust tests) and the whole pane including
the exact invokes it makes (Playwright). The middle, `create_and_announce` ->
`wait_for_answer` -> `settle`, is not covered by a test, because `inbox.rs`
resolves its store from `dirs::config_dir()/xnaut/inbox` with no injection
point, and that is André's live Mesh inbox. Writing test items into it was out
of bounds for this run, and making the store path injectable is a change to
`inbox.rs` beyond this ticket. The parts of that path that are not new are the
ones the veto's ask tier already exercises daily.

**The PM ticket was not updated.** This run was forbidden from editing
`~/.xnaut-control`. XNAUT-192's `documentation` field should point at
`work:xnaut/Development/features/2026-08-19_Plan-Canvas.md`, and its status
moved.

## The trap that cost the most

Anything else on port 4173 poisons a mutation run. `playwright.config.mjs` sets
`reuseExistingServer: true`, so the mutated copy's tests get served whatever is
already listening.

It bit twice, two different ways. First a `node tests/static-server.mjs` I had
left running served the UNMUTATED original, and the run reported my two new
entries AND two long-standing ones as SURVIVED, which reads as "these checks
are decoration". They are not; the port was. Then I ran `npx playwright test`
alongside `mutation-check --all`, the two runs raced for the same port, and the
same two entries came back BASELINE ("already fails unmutated"). Both are
artifacts. With the port to itself the run is 20/20, and `--all` catches the
Rust entry.

Rule: nothing else may touch 4173 while a mutation run is going, including
another agent's. A concurrent run on another ticket hit the same trap from the
other side (its suite passed 91/91 while actually exercising THIS worktree's
`src/`) and is fixing `scripts/mutation-check.cjs` to hand its children a
separate port. I did not make that change here, so the two do not land as
conflicting edits to one file.

## Design doc

`~/.xnaut-vault/work/xnaut/Development/features/2026-08-19_Plan-Canvas.md`
