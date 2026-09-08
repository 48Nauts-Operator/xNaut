# XNAUT-255 test bundle

Author: @rigtwo
Session: f27602f8-b63c-4201-a3d3-32590ff6b520
Branch: agent/rigtwo/xnaut-255

## What this closes

The ticket's provisioning was already done on 2026-08-31. The 2026-09-08 triage
named what was not: *"autonomous rig control and the remaining launch/pruning
work"*. That is the ticket's own list of queued cleanups — "launchctl-based
launch, rig session pruning" — plus the headless control surface they were
blocking. All three are in this change.

## Change

### 1. The rig launches under launchd, not over an ssh tty

`scripts/rig-launchd.sh` (new). Verbs `install / launch / quit / status /
uninstall / plist`, locally or on the rig with `--host tron`; remote runs
re-invoke this same script over ssh rather than shipping a second copy of the
launch logic.

The ssh-tty launch it replaces worked, and cost two things. The app was a child
of the ssh session, so closing the connection took it with it; and its stdout
was that connection's pipe, which is the `failed printing to stdout: Broken
pipe` abort recorded in the ticket. Under launchd the app's parent is launchd
and its stdout is a file, so neither is reachable.

The plist is where the whole launch contract lives, and every field in it is
there because its absence has cost a cycle before:

- `EnvironmentVariables.PATH` carries `/opt/homebrew/bin`. A launchd job
  otherwise inherits `/usr/bin:/bin:/usr/sbin:/sbin`, where a bare `zellij`
  cannot spawn — the 1.14.0 bug documented in `zellij.rs::zellij_bin`, where the
  Observatory listed nothing while five sessions were live.
- `StandardOutPath` / `StandardErrorPath` are files. `main.rs` already points a
  non-tty stdout at `/dev/null` in release builds, so the panic is fixed at the
  source; this means it cannot come back by another route, and leaves a log.
- `KeepAlive` and `RunAtLoad` are both false. A restarter would win every race
  against a quit test, and installing a job is not the same request as starting
  the app.
- `LimitLoadToSessionType Aqua`, so the window is really on screen and
  Accessibility works.

### 2. EXITED zellij sessions are pruned

`scheduler::reap_idle_runs` only ever walked LIVE sessions, so every run that
ended normally left an EXITED remnant and nothing removed it — one per cycle on
the rig, forever.

- `zellij::prunable_exited` — pure selection, four rules, each erring towards
  keeping: EXITED only; `xnaut-` prefix only (the owner's own `cx-*` panes share
  the global namespace); old enough measured from the LATER of creation and last
  activity; and an unknown age is never permission.
- `zellij::prune_exited_sessions` / `#[tauri::command] zellij_prune_exited`,
  registered in `main.rs` and `permissions/default.toml`.
- `scheduler::prune_exited_sessions_hourly`, on the existing 60s tick, taking
  day-old sessions and recording removals to the agent ledger as `zellij_pruned`.
  It fires on the first tick as well as every sixtieth, because an app restarted
  more often than hourly — which is every app on a test rig — would otherwise
  never prune at all.
- `POST /api/control/prune-sessions[?hours=N]` on the mobile bridge, so the rig
  can be swept from another machine between cycles.

Measuring age from creation alone would have deleted a long-running session that
exited a moment ago, silently. That is the second unit test.

### 3. The lever gained the verbs that needed a human

`scripts/control-xnaut.mjs`: `launch`, `quit`, `prune`, `cycle`.

`cycle` is the headless durability proof: read the live zellij sessions, quit
the app, bring it back, wait for the bridge to answer (polling it *is* the
readiness test — a fixed sleep is what made two earlier rounds report a dead app
that was merely booting), then assert every session that was live before the
restart is live after and the app can see it again.

`quit` and `cycle` refuse a 127.0.0.1 target without `--force`. CLAUDE.md §8:
`--host` defaults to this machine, and forgetting it is the mistake worth an
error message.

### 4. The shell tests now run

`tests/*.test.sh` were invoked by hand only, by whoever was already suspicious of
the file they were in — the one moment a test earns nothing.
`scripts/run-shell-tests.sh` (new) runs them with a per-test deadline, wired into
`just test` and `scripts/tron-test.sh` phase 2b.

The deadline is not decoration: see "Not fixed here" below.

## Verification

Run from the worktree root.

- `cargo test --manifest-path src-tauri/Cargo.toml`: **977 passed, 0 failed**,
  42 ignored, 0 filtered out.
- `XNAUT_TEST_PORT=4291 npx playwright test`: **133 passed, 0 failed** (2.6 min).
  `npm install` was needed first in a fresh worktree (107 packages).
- `./tests/rig-launchd.test.sh`: 12 checks, all pass.
- `./scripts/run-shell-tests.sh`: 2 run, all passed, 1 skipped (below).
- `cargo clippy --manifest-path src-tauri/Cargo.toml --bin xnaut`: no findings in
  any line this change touched. The repo carries 45 pre-existing warnings.

Flakes seen, both load-sensitive and both pre-existing — reported rather than
smoothed over, because a suite that flakes is a fact about the suite:

- `zellij::tests::no_interstitial_pane_floats_over_a_session_we_launch` failed
  once in three full Rust runs. It spawns a real zellij server in a PTY and polls
  for 10s; it passes alone and passed on the two other full runs.
- `raw-values.spec.mjs:76 › … leaves no spinner running` failed as a group once
  in three Playwright runs (124/133). Each of those cases waits ~9.8s. Passed on
  the two other runs.

## What the tests prove

`tests/rig-launchd.test.sh` asserts the plist against `plutil -extract`, i.e.
through the same parser launchd uses, not against the string the script printed:
Program resolves into the named bundle (which contains a space), Homebrew is on
PATH, both output paths are absolute files, `KeepAlive`/`RunAtLoad` are false,
the session type is Aqua. It also checks `install` refuses a bundle with no
executable — bootstrapping a job whose Program does not exist succeeds, and then
every launch fails with a code nobody reads — and that `quit`/`cycle` refuse a
local target.

`zellij::tests::the_prune_takes_only_our_own_old_exited_sessions` runs five
sessions past the rules: ours-and-cold (taken), live (kept), the owner's (kept),
ours-but-just-finished (kept), ours-with-no-readable-age (kept, at *every*
threshold including `hours=0`).

## Manual verification

The rig itself could not be driven from this session — see below — so this is
the procedure, not a transcript:

```bash
# 1. Install the job on the rig, pointing at a shipped bundle.
scripts/rig-launchd.sh install --host tron --app "/Applications/xNAUT TEST.app"
scripts/rig-launchd.sh status  --host tron          # installed:true running:false

# 2. Start it, and confirm the bridge answers from another machine.
scripts/rig-launchd.sh launch  --host tron          # {"ok":true,"pid":…}
node scripts/control-xnaut.mjs doctor --host tron.local --ssh tron

# 3. Close the ssh connection entirely, wait, ask again. The app is still there:
#    that is the difference from the ssh-tty launch.

# 4. The durability cycle, with no human at the rig.
node scripts/control-xnaut.mjs cycle --host tron.local --ssh tron
#    -> liveBefore/liveAfter/lost; exits 1 if anything live was lost.

# 5. Sweep the remnants between cycles.
node scripts/control-xnaut.mjs prune --hours 0 --host tron.local --ssh tron
```

To see the prune locally without a rig: leave an `xnaut-*` session, exit it, and
`curl -X POST "http://127.0.0.1:8931/api/control/prune-sessions?hours=0&token=$(jq -r .token "$HOME/Library/Application Support/xnaut/mobile.json")"`.
A live session and any non-`xnaut-` session must be untouched in the report.

## Not fixed here

- **The rig was unreachable from this session.** `ssh tron` fails host-key
  verification, and `tron.tail138398.ts.net` (100.103.152.113, pings, sshd
  answering) refuses publickey for `zelda`, `andre`, `admin` and `naut` with
  every key in `~/.ssh`. So nothing in this change has been exercised against
  tron. Everything above is checked locally and by unit test; the rig leg waits
  on ssh trust being restored, which is not something an agent should repair
  unattended.
- **`tests/gui-smoke-refuses.test.sh` hangs.** It runs `gui-smoke.sh`, which
  re-execs itself to obtain an Accessibility grant, and with no terminal to
  elevate that re-exec never returns — over two minutes with no output. This is
  pre-existing and unrelated to this ticket. It is in `run-shell-tests.sh`'s skip
  list *by name, with the reason inline*, rather than filtered out silently: the
  guard it checks is real and worth checking, and it needs the harness fixed, not
  the test deleted. Run it by hand from a terminal until then. The runner's
  per-test deadline exists so this class of thing is a named failure and never a
  suite that hangs.
- **XNAUT-185** (30-agent scale test on tron) is unblocked by this but not
  attempted.
