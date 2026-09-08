# XNAUT-307 — a beacon in every sandbox: the reaper acts on liveness, never on age

Branch `agent/claude/xnaut-307`, commit `3d8bb95`, on top of `dev` at `4f00415`.

XNAUT_TEST_TOTALS={"rust":[{"passed":1035,"failed":0,"ignored":45}],"ui":[132]}

## What changed

XNAUT-266 shipped a sandbox reaper that destroyed a GitVM environment 45
minutes after launch whether or not an agent was working in it. Destroying a
sandbox destroys `/workspace` (XNAUT-40). The selector was
`now - last_used_ms > 45 min`, and `last_used_ms` is stamped once, at launch —
so "idle for 45 minutes" actually meant "launched 45 minutes ago", and an agent
that had been working steadily since was exactly as reapable as an abandoned
box.

Age is not evidence of anything. A beacon is.

| file | what it now does |
|---|---|
| `src-tauri/src/beacon.rs` (new) | `xnaut-beacon`: the shell loop that runs inside the sandbox and pongs every 60 s |
| `src-tauri/src/agent_hooks.rs` | `/v1/beacon`, authenticated like every hook route, and ownership-checked |
| `src-tauri/src/run_control.rs` | `Pong`, `apply_pong`, `beacon_in`, `remote_proofs`, `RunManifest.remote_env` |
| `src-tauri/src/sandbox.rs` | `Liveness` + `reapable` + `reapable_now` replace the age selector; `cli::push` now runs `apt-get update` |
| `src-tauri/src/agent_profiles.rs` | `admit_environment` acts on the registry verdict; `launch_on_gitvm` registers a run and starts the beacon |
| `src-tauri/src/main.rs` | `mod beacon` |

Four properties are load-bearing:

1. **The beacon pongs unconditionally.** It reports `agent_pid: null` after the
   agent exits and the same numbers when nothing has moved. Because a pong is
   never conditional on work happening, SILENCE carries exactly one meaning —
   the VM is gone — and that is the only thing the reaper is allowed to act on.
2. **It runs in its own tmux session.** In the agent's session it would die with
   the agent, and "the agent exited but the box is still up and still billing"
   would arrive as silence, which means something else entirely.
3. **A pong never speaks for the agent.** `apply_pong` deliberately does not
   touch `last_hook_at`, which `verdict` treats as proof of work. A beacon
   pinging beside a dead agent must not read as an agent making progress.
4. **`Unlinked` is not reapable.** A ledger entry naming no run has no evidence
   either way. The honest response to "I cannot tell" is to refuse the launch
   and name the machine, not to destroy a workspace on a guess.

## Test results

Run in this worktree on tron, 2026-09-08.

```
cd src-tauri && cargo test --bin xnaut
test result: ok. 1035 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out

XNAUT_TEST_PORT=4291 npx playwright test
132 passed (2.6m)

cargo clippy --bin xnaut
warning: `xnaut` (bin "xnaut") generated 45 warnings
```

Clippy is 45 both before and after, measured by stashing this branch's changes
and re-running — a comparison rather than an adjective. One warning was added
mid-work (`script_path` never used) and the dead function was deleted rather
than left.

45 ignored rather than 42: the three added are the live-only helpers below,
which need a real sandbox and destroy it.

## Mutation check

Removing the liveness check from the reaper — moving `Liveness::Alive` into the
reapable arm of `sandbox.rs::launch_env::live::reapable` — turns three tests
red, including the regression by name:

```
sandbox::launch_env::live::tests::a_sandbox_with_a_live_beacon_is_never_reaped_however_old_it_is ... FAILED
sandbox::launch_env::live::tests::only_a_run_that_is_over_or_unreachable_may_be_destroyed ... FAILED
sandbox::launch_env::live::tests::reaping_spares_the_environment_this_launch_wants_back ... FAILED
```

Restored afterwards; `cargo test --bin xnaut live::tests` is 16 passed / 0
failed.

## The live leg, on a real machine

Run on tron against GitVM sandbox `sb-8cac65d0` (template `agent-desktop`),
created for this check and destroyed by it. The beacon script was DUMPED FROM
THE CODE rather than hand-written, via the ignored helper
`beacon::tests::dump_the_beacon_script_for_a_live_run`, so what ran on Linux is
what `beacon::script` produces. The one deviation: `interval_secs` was 20 rather
than 60, so three pongs took a minute rather than three. It is a config field,
not a code path.

```
18 pongs landed, authenticated, 20 s apart:
  19:34:39 {"run_id":"live-run-307","agent_pid":1338,"capture_bytes":0,"head":"275e93b","dirty":0}
  19:36:19 {"run_id":"live-run-307","agent_pid":1604,"capture_bytes":600,"head":"275e93b","dirty":0}
  19:36:39 {"run_id":"live-run-307","agent_pid":1604,"capture_bytes":900,"head":"275e93b","dirty":0}

beacon killed -> pongs before 18, after 45 s of silence 18   (the agent kept running)

replayed through the real registry and the real reaper:
  pong ... bytes=0    head=""        -> no movement
  pong ... bytes=0    head="275e93b" -> moved
  pong ... bytes=300  head="275e93b" -> moved
  pong ... bytes=1200 head="275e93b" -> moved
  liveness while reporting: Alive
  liveness after silence:    Gone

production reap:
  pulled agent-uncommitted-work.txt back, then destroyed the sandbox
  control plane: {"code":"not_found","message":"sandbox not found: sb-8cac65d0"}
```

The replay deserialises the recorded bodies with the PRODUCTION `Pong` type. The
shell builds its JSON with `printf`; if the two ever disagree about a field name
or a type, every pong 400s, the run goes silent, and the reaper destroys a
working sandbox — the original bug arrived at from the other direction. Nothing
in the offline suite compares them.

### Two real bugs the live run found, both fixed here

- **`cli::push` could not install tmux.** On a fresh `agent-desktop` image
  `apt-get install -y -q tmux` answers *"Package tmux is not available, but is
  referred to by another package"* and exits non-zero, because the image's
  package lists do not carry it. Every GitVM launch would have failed at that
  line — the leg XNAUT-266 shipped without ever running live. With
  `apt-get update` in front, the same install produces tmux 3.4.
- **`git rev-parse` cannot work in any GitVM sandbox.** The `gitvm` CLI rsyncs
  with `--exclude '.git/objects'` (hard-coded, `~/bin/gitvm` line 177), so
  `/workspace` has a `.git` that git itself refuses: *"not a git repository"*.
  The beacon would have lost half its progress evidence in the one environment
  this ticket is about. `HEAD` and the ref file it points at are plain text and
  ARE synced, so the beacon falls back to reading them — measured live,
  recovering `275e93b`, the exact local commit.

## How to verify it by hand

Offline, no sandbox needed:

```sh
cd src-tauri
cargo test --bin xnaut                      # 1035 passed, 0 failed, 45 ignored
cargo test --bin xnaut beacon::             # the script, executed against a real shell
cargo test --bin xnaut live::tests          # the reaper's rules
cargo test --bin xnaut run_control::        # the pong and remote observation
```

The beacon tests EXECUTE the generated shell rather than string-matching it: a
script that parses but reports the wrong numbers would satisfy a `contains`
assertion and still get a working agent's `/workspace` destroyed.

The mutation, to see the guard is real:

```sh
# in sandbox.rs::launch_env::live::reapable, move Liveness::Alive to the true arm
cargo test --bin xnaut live::tests          # 3 failed
git checkout src-tauri/src/sandbox.rs
```

Live, needs a GitVM key and destroys what it creates:

```sh
mkdir -p /tmp/beacon-check && cd /tmp/beacon-check && git init -q \
  && echo one > a.txt && git add -A && git commit -qm first \
  && echo '{"template":"agent-desktop","timeout":3600,"authSync":[]}' > .gitvm.json \
  && gitvm warm-up

# 1. the script the code actually generates
XNAUT_BEACON_HOOK=http://127.0.0.1:51899 XNAUT_BEACON_TOKEN=t XNAUT_BEACON_RUN=r \
XNAUT_BEACON_BIN=sleep XNAUT_BEACON_SESSION=xnaut-live-agent XNAUT_BEACON_INTERVAL=20 \
  cargo test --bin xnaut dump_the_beacon -- --ignored --nocapture

# 2. listen on :51899, ssh -R 51899:localhost:51899 into the box, stage the
#    script and start it with `tmux new-session -d`. Pongs arrive every 20 s.

# 3. the real pongs through the real registry and reaper
XNAUT_BEACON_REPLAY=/tmp/pongs.jsonl \
  cargo test --bin xnaut replay_live_pongs -- --ignored --nocapture

# 4. XNAUT-40's ordering, against the live box (this DESTROYS it)
#    write a file inside the sandbox first, then:
XNAUT_REAP_DIR=/tmp/beacon-check \
  cargo test --bin xnaut reap_a_live_sandbox -- --ignored --nocapture
```

## Deliberately not done

- **The GitVM lease is not renewed, because the provider exposes no way to renew
  it.** Ticket item 3 asks for `timeout_secs` to be renewed while a run is
  alive. Measured against the control plane on 2026-09-08: `POST
  /v1/sandboxes/{id}/renew`, `/extend` and `/heartbeat` all answer the Go mux's
  `404 page not found`, while the known-real `/exec` answers
  `{"code":"not_found","message":"sandbox not found: …"}` for the same fake id —
  so those routes do not exist, rather than the id being wrong. `PATCH
  /v1/sandboxes/{id}` answers 405. The lease is settable only at create time,
  through `timeout` in `.gitvm.json`. Nothing was invented to paper over this;
  it needs a GitVM-side endpoint, and until there is one a long-running agent
  can still be reaped by the PROVIDER at its lease expiry. That is a different
  reaper from the one this ticket fixed, and it is outside xNAUT.
- **`dirty` is always 0 in a GitVM sandbox**, for the same `.git/objects`
  reason: `git status` needs objects and the HEAD-file fallback cannot supply
  them. Nothing acts on `dirty` — it is reported for the owner's eyes — so this
  costs information, not correctness. It is left in the payload because the
  field is right and the environment is what is missing.
- **No end-to-end launch through `agent_profile_launch`.** The live leg
  exercised every remote piece — the script on Linux, the reverse tunnel, tmux,
  `pipe-pane`, curl, the wire shape, the reaper — but starting a real agent
  through the Tauri command needs the running app, and the owner's app is his,
  not a test fixture. The seam between them is `launch_on_gitvm`, whose beacon
  wiring is covered offline.
- **exe.dev gets no beacon.** Its ledger entries stay `Unlinked`, which is not
  reapable — and `live::reap` never destroys an exe.dev workdir anyway, because
  the warm cache is the reason to pay for it. Extending the beacon there is a
  separate slice with a different cost model.
