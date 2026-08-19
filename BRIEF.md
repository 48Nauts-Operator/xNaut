# XNAUT-200 — SSH: the interactive half was never finished

Connecting appears to work and then nothing types. Verified:

- `src-tauri/src/ssh.rs:240` binds the authenticated handle to `_ssh_handle` and
  drops it, so `open_shell` and `execute_command` are unreachable.
- `src-tauri/src/commands.rs:521` `write_to_ssh` is a TODO that logs and returns
  Ok, so keystrokes go nowhere and the UI shows no error.
- Nothing emits `ssh-output-<id>`, so there is no output path either.
- The profile editor sends `privateKey` (`src/js/app.js:5433`) while `SshConfig`
  expects `key_path` (`src-tauri/src/ssh.rs:22`), so key auth always falls
  through to "No authentication method provided".

`close_ssh_session` was already fixed under XNAUT-198; do not redo it.

Two acceptable outcomes, your call, argued in REPORT.md:
1. Finish it. A real channel, a reader task emitting `ssh-output-<id>` the way
   `pty.rs` emits `terminal-output:<id>`, a working `write_to_ssh`, and the
   field name fixed on one side.
2. Make the app honest. If a full channel is more than one night, fix the field
   name bug and make the UI say the interactive session is not available rather
   than opening a terminal that silently eats input.

Do not leave it as it is. A feature that looks like it connected and then
swallows every keystroke is worse than one that says it cannot.
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
