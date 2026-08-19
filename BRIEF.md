# XNAUT-193 — Every MCP tool answers in the same shape

The `xnaut_*` MCP tools in `src-tauri/src/agent_hooks.rs` each return their own
ad hoc JSON, so an agent has to infer what happened and what to do next.

Adopt one response contract for all of them:

    status        success | warning | error
    summary       one line, written for the model
    next_actions  what the agent can usefully do now
    artifacts     paths or ids it can act on

Do it once in the result helper rather than per tool. Keep the existing payload
under a `data` key or alongside; nothing that already consumes these tools may
break, and the ACL test (`cargo test --bin xnaut audit::`) must stay green.

The idea is borrowed from ECC (github.com/affaan-m/ecc, MIT),
`skills/agent-harness-construction/SKILL.md`. Credit it in the file header, in
the form used by `gate_score.rs` and `veto.rs`, and say where we departed.

Prove it with a test that asserts the shape for a representative tool, and one
that fails if any tool returns a bare value instead of the envelope.
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
