# XNAUT-202 — Surfaces that claim more than the code does

Six small honesty bugs found while documenting features. Each is verified; fix
what is cheap, and where a fix is not cheap, make the UI stop claiming it.

1. The Collaborators tab writes `collab:<handle>` onto a profile and its help
   text says "Enforced at dispatch" (`src/js/agent-space.js:1826`). No Rust file
   reads it. Either enforce it or stop saying it is enforced.
2. The plugin panel footer says "Chat turns do not use plugins"
   (`src/js/plugins-panel.js:174`). `mcp_client::open_for` plus
   `agent_tools.rs:681` made that untrue.
3. The plugin library calls it attestation; what exists is `verify()`
   (`src-tauri/src/plugins.rs:1075`), a proof the server starts and stays up.
   Nothing signs anything. Name it what it is.
4. `ledger_recent` is registered and ACL-allowed with no frontend caller, so the
   decision ledger has no reader. Either build the small pane or say why not.
   Note that an allow is never recorded, only refused, asked and conflict.
5. `worklog_summary` is fetched at `src/js/app.js:1215` and its result unused;
   the preview at `:1234` renders `session.generate_summary`, a Rust METHOD name
   that is not a serialised field, so the fallback one-liner always wins.
6. `codex_spend` is registered (`main.rs:366`) and never called, so the USD
   estimate exists only in the backend and never on the footer.

Each one that you fix needs a check that fails without it. For the ones you do
not fix, the UI text must change in the same commit so the app stops overclaiming.
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
