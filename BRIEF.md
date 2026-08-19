# XNAUT-199 — Terminal Triggers: one path, not two

There are TWO trigger implementations and the product ships both half-wired.

1. `src/js/app.js:6075` `checkTriggers()`, called from the terminal output
   handler at `app.js:2899`. This is the live one. Notify only. It supports a
   `keyword` type by splitting on commas, which the Rust side does not have.
2. `src-tauri/src/triggers.rs::process_output`, complete and correct, handles
   Notify, RunCommand and AiAssist, and has NO CALLER anywhere. It is
   `#[allow(dead_code)]`.

Nothing in the frontend listens for `trigger-notification`, `trigger-command`
or `trigger-ai-assist`. The trigger UI offers three actions and delivers one.

YOUR JOB IS THE DECISION FIRST, then the deletion. Pick one path and remove the
other. Both are defensible: Rust can do all three actions and sees output the
frontend never renders; JS works today and has keyword matching.

If you choose Rust, note two hazards I hit when I tried it and reverted:
- The PTY flush loop (`src-tauri/src/pty.rs`, the 16ms flusher) is the loop that
  caused an emit-flood livelock once. Guard on "are there any enabled triggers"
  before doing regex work, and run the scan in its own task.
- Adding the Rust path without deleting the JS one double-notifies.

Whichever you choose, the three actions in the UI must all work or the UI must
stop offering the ones that do not.
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
