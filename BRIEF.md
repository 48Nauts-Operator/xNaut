# XNAUT-201 — Mobile companion: the terminal mirror is dead on this lineage

Nothing inserts a `MobileTap` into `state.mobile_taps` on `feature/loops-platform`.
It is only read (`src-tauri/src/mobile.rs:880`, `:961`), removed
(`src-tauri/src/pty.rs:344`) and updated (`pty.rs:364`). So `handle_socket`
finds `None` and closes the socket for every session: the phone connects and
sees nothing.

The insert exists at `pty.rs:206` on the `feature/mobile-companion` branch,
which is NOT an ancestor of HEAD. Look at it with:
`git show feature/mobile-companion:src-tauri/src/pty.rs`

The unit tests pass because they construct `MobileTap` directly, which is why
nothing caught this. Whatever you do, add a test that would fail if the insert
were missing again, exercising the path from session creation rather than
building the struct by hand.

Cherry-pick the insert rather than merging that branch; it carries unrelated
work. Check what else the mirror needs on this lineage before declaring it
fixed, and say in REPORT.md whether the phone actually receives output or only
gets a socket that stays open.

Second, smaller finding, worth recording but only fix it if the first part
lands with time to spare: the served phone app (`src/mobile.html`, compiled in
with `include_str!`) implements 3 of the 15 bridge routes, and scales the
terminal with a CSS transform instead of sending the `resize` op the bridge
already supports.
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
