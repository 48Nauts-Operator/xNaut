# XNAUT-200: SSH, the interactive half

**Outcome 1. Finished it.** A real channel, a reader thread emitting
`ssh-output-<id>`, a working `write_to_ssh`, and the field name fixed on both
sides. The ticket's four findings were all correct; I found two more that would
have kept the feature unusable even after fixing those four.

## Why finish rather than make the app honest

I read the ssh2 0.9.5 source before choosing
(`~/.cargo/registry/.../ssh2-0.9.5/src/channel.rs:22` and `session.rs:108`):

- `Channel` is `Clone`, `Send` and `Sync`, and holds an `Arc<Mutex<SessionInner>>`.
  Storing the channel therefore keeps the whole connection alive, so the "complex
  trait bounds" the old comment in `state.rs` worried about do not exist.
- `Session::set_blocking(false)` plus `TcpStream::set_nonblocking(true)` gives a
  poll loop, and EAGAIN arrives as `io::ErrorKind::WouldBlock`
  (`error.rs:219`), so a reader thread cannot wedge a writer.
- `pty.rs:246` already has the reader-thread and coalesced-emit pattern this
  needs, including the two freeze fixes it must not undo.

That is an evening, not a rewrite, and outcome 2 would have shipped a dialog
saying "not available" on top of code that was one struct field away from
working.

## What was actually broken

Six things, four from the ticket and two found while reading:

1. `ssh.rs:240` bound the authenticated session to `_ssh_handle` and dropped it.
2. `commands.rs:521` `write_to_ssh` was a TODO that logged and returned `Ok`.
3. Nothing emitted `ssh-output-<id>`.
4. The editor sent `privateKey`, `SshConfig` read `key_path`.
5. **New:** the key field the user can actually see and type into is
   `#ssh-key-path`. `saveSSHProfile` read `#ssh-private-key`, a textarea inside
   `#ssh-key-content-group`, which `toggleSSHAuthMethod` never unhides. So the
   saved value was always the empty string, and renaming the field alone would
   have changed nothing. Fixing 4 without 5 looks like a fix and is not.
6. **New:** there was no way into the SSH list. Both live entry points
   (`app.js:2184` Settings, `app.js:7119` More menu) called
   `showModal('ssh-modal'); loadSSHProfiles();` and never `renderSSHProfiles()`,
   so the modal opened with an empty list and no Connect button. The only call
   site that did render, `showSSHModal()`, hung off `target.id === 'btn-ssh'`,
   and no element with that id exists in `index.html`.

## What changed

**`src-tauri/src/ssh.rs`**. The substance.

- `connect_shell` connects, authenticates, opens a channel, requests an
  `xterm-256color` PTY, starts a shell, then flips the socket and the session to
  non-blocking. Returns the `Channel`.
- `auth_method` is a pure function so the credential choice can be tested
  against the exact JSON the editor sends. `SshConfig` is now
  `rename_all = "camelCase"`, so the wire name is `keyPath`, matching every
  other config struct in the codebase.
- `spawn_ssh_reader` drains everything available per tick into one
  base64-encoded `ssh-output-<id>` event, sleeps 16 ms, and on EOF forgets the
  session and emits `ssh-closed-<id>`. Own OS thread, matching the reasoning in
  `pty.rs:311`: a std mutex is held across libssh2 calls, and terminal I/O on
  tokio workers is what caused the keystroke stalls fixed in b528872. The 16 ms
  tick is also the coalescing from a431c07, without needing a second task.
- `write_all_retrying` / `retry_would_block` handle EAGAIN and short writes.
  `write_all` is unusable on a non-blocking channel: it throws away how far it
  got, so one EAGAIN mid-paste would resend from the start.
- Connect, write and resize all run on `spawn_blocking`. A wedged link makes a
  write spin for up to the 2 s budget, and parking tokio workers on terminal
  I/O is the freeze this codebase paid for twice.
- `expand_tilde` so a hand-typed `~/.ssh/id_ed25519` works. Paths taken from
  `~/.ssh/config` were already absolute.
- `SshSessionHandle` and its two unreachable methods are gone; `connect_shell`
  replaces them.

**`src-tauri/src/state.rs`**. `SshSession` carries
`Arc<std::sync::Mutex<ssh2::Channel>>`. `Debug` dropped, `Channel` has none.

**`src-tauri/src/commands.rs`**. `write_to_ssh` calls the module; new
`resize_ssh`, because the channel opens at a placeholder 80x24 and only xterm
knows the real size.

**`src/js/app.js`**. Reads `#ssh-key-path` and saves it as `keyPath`; sends
`keyPath`; decodes the base64 output as UTF-8; pushes the real size after fit
and on resize; listens for `ssh-closed-<id>`; both menu entries now call
`showSSHModal()`. Removed: the duplicate `ssh-output` listener in `connectSSH`
which wrote every chunk twice, the now-unused `findTerminalBySSHSession`, and
the dead `#btn-ssh` branch. `testSSHConnection` closes the session it opens
instead of leaking a live connection per press.

**`src/index.html`**. The hidden private-key textarea is gone.

No migration for old profiles: the `privateKey` field they carry was always
empty, for the reason in finding 5.

## What I proved

`cargo test --bin xnaut`: 501 passed, 0 failed, 21 ignored.
`cargo clippy --bin xnaut`: no warning points at anything I touched (the one in
`state.rs` is the pre-existing `VoiceCapture::stop` field, untouched).
`XNAUT_TEST_PORT=4183 npx playwright test`: 92 passed.
`node scripts/ssh-interactive-smoke.cjs`: passes.

One caveat on the Rust suite, and it is not mine. In 12 consecutive runs under
heavy parallel load, one run failed
`claims::tests::a_second_agent_on_the_same_file_is_a_conflict`. Cause, read
rather than guessed: `claims.rs` keeps `TOUCHES` in a process-global mutex, and
both tests in that module call `reset()` on it (`claims.rs:126`). Cargo runs
them on parallel threads, so the sibling's reset can wipe the first agent's
touch between this test's two `note_at` calls. It is a shared-global race in
XNAUT-190's tests (5cecfae), nothing to do with SSH, and it reproduces about
1 run in 12. My own tests: 60/60 clean under the same load.

**A real server, end to end.** `src-tauri/src/ssh.rs`
`a_real_shell_runs_what_is_typed_into_it` is `#[ignore]`d because it needs one.
I ran a throwaway OpenSSH on 127.0.0.1:2222 with its own host key, client key
and authorized_keys under `/tmp/xnaut200-sshd`, and ran:

```
XNAUT_SSH_TEST='cand0rian@127.0.0.1:2222:/tmp/xnaut200-sshd/client_key' \
  cargo test --bin xnaut -- --ignored a_real_shell    # 1 passed
```

It authenticates with a key, opens a PTY and a shell, writes
`echo xnaut-$((100+100))` and waits for `xnaut-200` to come back. The
arithmetic is deliberate: the echoed command line carries the expression, so
only a shell that actually evaluated it produces the string. The server and its
keys were deleted in the same session.

**Each check goes red when the thing it covers breaks.** Verified by hand, then
registered in `scripts/mutation-check.cjs`:

| break | check that noticed |
| --- | --- |
| delete `channel.shell()` | live test: "the remote shell never answered" |
| `rest = &rest[..0]` in the write loop | `a_write_survives_eagain_and_short_writes` |
| drop `rename_all = "camelCase"` | `a_key_profile_from_the_editor_resolves_to_key_auth` |
| drop `channel:` from the stored session | `ssh-interactive-smoke` |
| rename the emitted event | `ssh-interactive-smoke` |
| `showSSHModal()` back to `showModal('ssh-modal')` | `ssh-interactive.spec.mjs`: no Connect button |
| send `privateKey` again | `ssh-interactive.spec.mjs`: `keyPath` undefined |
| write `payload.data` raw | `ssh-interactive.spec.mjs`: buffer holds `cmVtb3Rl...` |

`node scripts/mutation-check.cjs --all`: 37/38 caught. The one survivor was
mine and it was the needle, not the check: `term.write(new
TextDecoder('utf-8').decode(bytes));` also appears in the PTY scrollback
restore at `app.js:2917`, so the harness mutated that line instead of the SSH
one and the SSH test rightly stayed green. Re-anchored on the `atob` line,
which is unique, and verified by hand with the same substitution: red, with the
buffer holding `cmVtb3RlIHNheXM6...`.

`tests/ssh-interactive.spec.mjs` drives the real More menu, the real modal, the
real Connect button and the real xterm instance with the Tauri bridge stubbed:
it asserts the invoked config carries `keyPath`, that `resize_ssh` follows with
the measured size, that a base64 `ssh-output-S1` event lands in the buffer as
UTF-8 (the payload has a `✓` in it, so a latin-1 decode fails), and that typing
produces exactly one `write_to_ssh` with the typed bytes.

## What I could not do

- **No run inside the real Tauri shell.** There is a running xNAUT I was told
  not to disturb, and a second instance from this worktree would fight it for
  singleton state. The backend is proved against a real SSH server, the frontend
  against a stubbed bridge; what is not proved is the two halves inside one
  webview. First thing to do with a build.
- **No PM ticket or vault doc.** The run rules forbid touching `~/.xnaut-control`.
  XNAUT-200 still needs its status moved and a `documentation:` reference.

## One thing to know, because it invalidated a test run

`playwright.config.mjs` hardcoded port 4173 with `reuseExistingServer: true`.
My first full suite run passed 91/91 against **another worktree's frontend**:
`xnaut-192` already had `tests/static-server.mjs` on 4173, so my server failed
to bind and Playwright attached to theirs. Confirmed with `lsof -p <pid> | grep
cwd`. The port is now `process.env.XNAUT_TEST_PORT || 4173`, and
`mutation-check.cjs` hands its children their own port, or every Playwright
mutation would read as survived while the un-mutated worktree answered the
request. Everything reported above was re-run on a private port.

Anyone running the suite in a worktree while another agent is running theirs
should set `XNAUT_TEST_PORT`.
