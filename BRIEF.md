# XNAUT-192 — Plan Canvas: review a plan in the pane, annotate it, approve or send it back

Today a plan ends as a wall of markdown in a terminal and the feedback loop is
retyping what you want changed. The target: the plan renders properly, André
clicks the part he means and attaches a numbered annotation, and hits Approve or
Request changes. The agent blocks until he answers.

Most of the scaffolding already exists in this codebase. Read these before
designing anything:
- `src/js/plan-pane.js` renders plans.
- `src-tauri/src/notes.rs` carries range-anchored annotations on both sides of a
  diff, from the hunk port. Reuse this rather than inventing an anchor model.
- `src-tauri/src/inbox.rs` `create_and_wait` already lets an agent block on a
  human answer, and `/v1/inbox/wait/:id` is the route it waits on. That is the
  await half, already built and already used by the veto's ask tier.

So the work is wiring those three together plus the verdict: Approve or Request
changes, returned to the waiting agent.

The idea is from ECC (github.com/affaan-m/ecc, MIT), `docs/design/plan-canvas.md`,
which credits lavish-axi by @kunchenguid. Credit both in the file header in the
form used by `gate_score.rs`, and record where we departed. We depart a lot:
they run a loopback web server with a DNS-rebinding guard, ship their own
markdown renderer and pin a Mermaid CDN, all so a CLI can reach a browser.
xNAUT is the browser. Do not port any of that.

Scope discipline: a plan that can be annotated and approved from the pane, with
the agent actually blocking on the answer. Nothing more. If you cannot finish,
land the smallest complete slice and say in REPORT.md exactly where you stopped.
## Rules for this run

You are working alone on one ticket, in your own git worktree, overnight. André is asleep.

- Your worktree is your own. Do not touch other worktrees, and never edit
  `~/.xnaut-control` (the PM repo) or `~/Library/Application Support/xnaut`.
- There is a running xNAUT app. Do not kill it, do not restart it, do not type
  into terminals or agent sessions you did not create.
- Commit locally on your branch. DO NOT push. DO NOT tag. Do not touch `main`.
- Read before you assert. State findings as "I read X:NN, it does Y", never
  "it probably". If you have been wrong twice about a cause, stop and go read.
- Every change needs a check that would FAIL if the change were wrong. Add a
  test, then break the code on purpose and confirm the test goes red, then put
  it back. `scripts/mutation-check.cjs` is the existing harness; add an entry.
- Before you finish: `cd src-tauri && cargo test --bin xnaut` and
  `cargo clippy --bin xnaut` must be clean, and `npx playwright test` must pass.
  If the Playwright browser is missing, `npx playwright install chromium` (a
  full disk keeps purging it, that is not your bug).
- Write `REPORT.md` at the root of your worktree: what you changed, what you
  proved, what you could not do and why. That file is how your work is read.
- No em-dashes anywhere, in code comments, commit messages or the report.
- If the ticket turns out to be wrong about the code, say so in REPORT.md and
  fix what is actually broken rather than what was described.

Comments in this codebase explain WHY, not what. Match that.
