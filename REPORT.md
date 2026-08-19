---
Author: Claude Opus 5 (agent/xnaut-201)
Last modified: 2026-08-19 09:35 CEST
---

# XNAUT-201: the terminal mirror on `feature/loops-platform`

## Verdict

The phone now receives real output, not just a socket that stays open.

That is proved in-process end to end through the production functions
(registration → tee → subscriber → replay), not inferred. The one hop I could
not drive from a test is named below, with what guards it instead.

## What was actually broken

The ticket was right. I read the branch before changing anything.

- `state.mobile_taps` was only ever read, removed and mutated. No code path
  inserted one. (Line numbers here are the pre-change ones the ticket cites:
  `mobile.rs:880`, `pty.rs:344`, `pty.rs:364`. Everything below is post-change.)
- `handle_socket` therefore hit its `None` arm for every session and sent
  `Message::Close` immediately. Every attach, every session.
- The insert exists at `pty.rs:206` on `feature/mobile-companion`, which is not
  an ancestor of HEAD. `git diff feature/mobile-companion HEAD -- src/mobile.html`
  is empty and `src-tauri/src/mobile.rs` on HEAD is a strict superset (+345
  lines), so the missing insert was the whole of the divergence. Cherry-picked
  by hand, as instructed; the branch was not merged.

Two things the ticket did not mention, found by tracing the rest of the path:

1. **`create_command_session` never got a tap either**, on this lineage or the
   reference one. It matters more here: `app.js:3415` and `app.js:3475` launch
   every agent tab through `create_command_session` (zsh -lc, and
   `zellij attach --create`), and `pty.rs:628` registers those in the agent
   registry. So the sessions the phone most wants to watch, the ones the session
   list labels `agent`, were exactly the ones with no tap. Fixing only
   `create_pty_session` would have left the mirror dead for agents while looking
   fixed for a plain shell.
2. **A desktop resize never reached the tap.** `pty::resize_pty` resized the PTY
   and stopped there; only the phone's own resize op updated `tap.cols/rows`
   (`mobile.rs`, old `resize_session`). The tap's dims are what a freshly
   attached phone builds its xterm from, so after any desktop window resize the
   phone would render the mirror at the width the session was born with, and
   every wrapped line would land wrong.

## What I changed

`src-tauri/src/pty.rs`

- **`register_session` (`pty.rs:68`)**. New. Takes the spawned PTY, pulls the
  reader and writer, builds the `PtySession`, and inserts into **both**
  `pty_sessions` and `mobile_taps`. Both create paths now go through it
  (`pty.rs:258`, `pty.rs:613`), so a session cannot exist without a tap. That
  is the fix, and putting it in one place is what stops finding #1 above from
  recurring the next time someone adds a third create path.
- **`tee_output` (`pty.rs:406`)**. The per-read mirror block lifted out of
  `spawn_pty_reader` unchanged (scrollback tail + `tap.push`), so it can be
  called from a test. The reader now calls it at `pty.rs:384`.
- **`resize_session` (`pty.rs:453`)**. One resize implementation, which also
  records the new dims on the tap. `resize_pty` is a one-line delegate.

`src-tauri/src/mobile.rs`

- **`attach_tap` (`mobile.rs:880`)**. The tap lookup `handle_socket` used to do
  inline, extracted verbatim. `handle_socket` now reads as
  "no tap, close the socket" in one `let else`.
- `resize_session` keeps its range check (dims off the wire need one; the
  desktop's own do not) and delegates the resize to `pty::resize_session`. That
  removed the duplicated resize body the old comment admitted to.

`scripts/mutation-check.cjs`: four entries, below, plus two changes to the
harness itself and one to `playwright.config.mjs`. Those are explained under
"The harness could not be trusted while I ran"; without them I could not produce
honest evidence tonight.

Nothing else was touched. No frontend change, no ACL change, no new dependency.

## What I proved

New test: `pty::tests::a_registered_session_can_be_mirrored_to_the_phone`
(`pty.rs:681`). It opens a real PTY, spawns a real child, and then runs the
production chain:

1. `register_session(...)`, the same call both create paths make.
2. `mobile::attach_tap(...)`, the exact lookup `handle_socket` does. Returns
   the ring, a live subscription and the dims.
3. `tee_output(...)`, the exact call the PTY reader makes per read.
4. the subscriber receives `hello phone`, and a **second** `attach_tap` replays
   the same bytes, at the dims a desktop resize moved to in between.

It builds no `MobileTap` by hand, which is precisely why the old tests
(`mobile.rs:1064`, `mobile.rs:1073`) stayed green through the bug.

Mutation entries added to `scripts/mutation-check.cjs`, each breaking the code
on purpose and requiring the check to go red:

| mutation | check |
|---|---|
| `XNAUT-201 a created session gets no mobile tap` (delete the `mobile_taps` insert) | `cargo test --bin xnaut pty::tests` |
| `XNAUT-201 a desktop resize stops reaching the mobile tap` | `cargo test --bin xnaut pty::tests` |
| `XNAUT-201 the PTY reader stops teeing reads` (delete the call site) | `cargo test --bin xnaut the_pty_reader_still_tees` |
| `XNAUT-201 the PTY tee stops feeding the mobile tap` (drop `tap.push`) | `cargo test --bin xnaut pty::tests` |

Results:

```
caught     XNAUT-201 a created session gets no mobile tap
           test pty::tests::a_registered_session_can_be_mirrored_to_the_phone ... FAILED
caught     XNAUT-201 a desktop resize stops reaching the mobile tap
           test pty::tests::a_registered_session_can_be_mirrored_to_the_phone ... FAILED
caught     XNAUT-201 the PTY reader stops teeing reads
           test audit::acl_tests::the_pty_reader_still_tees_every_read_to_the_mobile_tap ... FAILED
caught     XNAUT-201 the PTY tee stops feeding the mobile tap
           test pty::tests::a_registered_session_can_be_mirrored_to_the_phone ... FAILED

4/4 mutations caught
```

Each of those is the harness applying the break, watching the check go red, and
putting the code back. The green run without the break is the 498-test line
below.

Gate commands, all from this worktree:

- `cd src-tauri && cargo test --bin xnaut`: 498 passed, 0 failed, 20 ignored.
- `cd src-tauri && cargo clippy --bin xnaut`: 23 warnings, byte-for-byte the
  same list as before my changes; none in `pty.rs` or `mobile.rs`.
- `PW_PORT=4291 npx playwright test`: 91 passed, 0 failed.

## The harness could not be trusted while I ran

Worth reading before the numbers above are believed, because I nearly filed a
false finding off it.

My first full `mutation-check --all` reported two entries SURVIVED
(`a context menu ignores the interface zoom again`,
`terminals stop cancelling the interface zoom`). Both are pre-existing entries
in files I never touched. Before writing that up as check decay I looked at what
was actually running:

```
27888  node scripts/mutation-check.cjs --all --json /tmp/xnaut201-mutations.json   (mine, 09:16)
35506  node scripts/mutation-check.cjs --all                                        (not mine, 09:17)
49888  node scripts/mutation-check.cjs --all                                        (not mine, 09:19)
```

Two other agents were running the same harness from their own worktrees. The
harness pinned one cargo target dir for the whole machine
(`$TMPDIR/mutation-check-target`), and `playwright.config.mjs` pins port 4173
with `reuseExistingServer: true` while `tests/static-server.mjs` serves the
`src/` next to itself. So concurrent runs share a cargo lock and a static
server: a "mutated" run can execute a binary rebuilt from another tree, or drive
a browser against another tree's `src/`. The mutated file is then never the one
under test, and the entry reads SURVIVED.

Those two SURVIVED lines are therefore not evidence of anything. I am not
reporting them as a finding, and I did not touch the other agents' processes.

The same collision explains the other oddity of the night: a full
`npx playwright test` on the shared port reported 9 failures (agent-space,
raw-values, surfaces-open, update-banner, zoom); the same specs passed when
re-run alone, and the whole suite passed 91/91 on `PW_PORT=4291`. Not flake
under CPU load, which is what I first assumed. A shared static server handing
out somebody else's `src/`.

Three small changes make an isolated run possible:

- `MUTATION_TARGET_DIR` overrides the cargo target dir (`mutation-check.cjs:384`).
- `PW_PORT` overrides the Playwright port (`playwright.config.mjs:6`).
- `--only <substring>` narrows to one ticket's entries, so gathering evidence for
  one fix does not mean re-proving the whole suite first.

The XNAUT-201 numbers above come from
`MUTATION_TARGET_DIR=... PW_PORT=4291 node scripts/mutation-check.cjs --all --only XNAUT-201`,
on a private target and port, so nothing else on the machine could reach them.

Left for André to decide: whether the defaults should be isolated per worktree
rather than opt-in. Two agents overnight is now the normal case here, and the
shared default fails silently in the direction that reads as "the check is fine".

## What I could not prove, and what guards it instead

`spawn_pty_reader` cannot be driven from a test. It takes an
`AppHandle<Wry>`, and Tauri's `mock_app()` hands back an `App<MockRuntime>`, a
different type; making it fit would mean genericising the runtime through every
signature it touches. So the hop "the reader actually calls the tee" is
verified by reading `pty.rs:384`, not by executing it.

Rather than leave that on trust, it is guarded by a source-level check,
`audit::acl_tests::the_pty_reader_still_tees_every_read_to_the_mobile_tap`,
in the same module and the same spirit as the existing
`every_command_the_frontend_calls_exists`. It is a call-site guard, not a
behaviour test, and it is labelled as one. Its ceiling: it would not notice the
tee being called with the wrong bytes. The behaviour underneath it is covered
by the mutation on `tap.push`.

I also could not run a live phone against this build. The bridge binds a fixed
port (8931, `mobile.rs:66`) and the running xNAUT app already holds it; starting
a second instance would either fail to bind or take the port from the app I was
told not to disturb. So "a real iPhone rendered a real agent session" remains
untested. Everything from `register_session` to the broadcast subscriber is
tested; the untested remainder is `handle_socket`'s `socket.send` and
`mobile.html`'s `term.write`, both unchanged by this ticket and both already
shipping.

## Second finding, recorded (not fixed)

`src/mobile.html` is compiled in with `include_str!` (`mobile.rs:279`). The
bridge registers 18 route paths; the phone calls three handlers, on two paths:

```
GET  /api/sessions        mobile.html:173
POST /api/sessions        mobile.html:212
GET  /ws/:session_id      mobile.html:229
```

Fifteen paths are never called from the served app: `DELETE /api/sessions/:id`,
`/api/sessions/:id/files`, `/api/sessions/:id/file`, `/api/sessions/:id/git`,
`/api/artifacts`, `/artifact/:token/*path`, `/api/observatory`,
`/api/observatory/stop-all`, `/api/agents/:id/interrupt`,
`/api/looms/:run_id/stop`, `/api/manager`, `/api/manager/message`,
`/api/manager/launch`, `/api/automations`, `/api/automations/:id/run`. That is a
files browser, a git overview, the Observatory, the Manager and the automation
runner, all built server-side and reachable by nothing. Building the phone UI
for them is a feature, not a fix, and not something to start unattended.

On the CSS transform: `mobile.html:256` renders at the desktop's cols and scales
down with `transform: scale()`, and the comment there says that is deliberate
(mirror what the desktop shows). The `resize` op the bridge supports does the
opposite: `handle_socket` (`mobile.rs:931`) documents it as the phone taking
over the PTY size while attached, so the desktop reflows. I did not switch it.
Reflowing a live agent's terminal to phone width the moment someone glances at
it on their phone is a product decision with real downside, and the desktop only
re-asserts its own dims on its next resize. It is a two-line change in
`startTerm` once that call is made; the backend has been ready the whole time.

Recommendation: keep the CSS fit as the default, and put the `resize` op behind
an explicit control in the terminal head, so phone-fit is something you ask for.

## Other things I read and deliberately left alone

- `close_pty` (`pty.rs:481`) removes the session and the scrollback but not the
  tap. In practice the reader's EOF arm (`pty.rs:374`) removes it a moment later
  once the killed child closes the PTY, so it self-heals. The tap does leak on
  the `Err(e)` read arm (`pty.rs:391`), which breaks without removing anything:
  a bounded 256 KB ring per errored session, invisible to the phone because the
  session is already gone from `pty_sessions`. Small, pre-existing, out of scope.
- `mobile.html` loads xterm from jsdelivr (`mobile.html:130`). The phone needs
  internet, not just the tailnet, for the mirror to render at all. Unchanged by
  this ticket, worth knowing.

## Housekeeping

Per the global rule this write-up belongs in the work vault with a PM ticket
referencing it. This run may not touch `~/.xnaut-control`, so the ticket half
cannot exist and a vault doc with no ticket pointing at it is the half-move the
rule warns against. It is committed here instead, in git, on the branch. If it
should also live at `work:xNAUT/Development/features/2026-08-19_XNAUT-201.md`,
that is a copy and a ticket edit away.

Committed locally on `agent/xnaut-201`. Not pushed, not tagged, `main`
untouched.
