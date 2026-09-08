# XNAUT-266 — finishing the one launch path

Slices 1 (the `launch_env` seam) and 2 (the exe.dev PTY) are on the lineage.
This plan covers what triage says is left: the GitVM leg, lifecycle, remote
adoption, and the deletions that make this a consolidation rather than a
fourth path.

## Where the code actually is today

- `sandbox::launch_env::resolve` is the one place that answers "where does
  this run", and `agent_profile_launch` is its only caller.
- `LaunchRoute` has two variants, `Local` and `ExeDev`. `LaunchEnv::GitVm`
  resolves but then refuses in `route()` with a survey of what is configured.
- `sandbox::exe` has the full remote shape: `ensure`, `push`, `stage_script`,
  `session_name`/`session_prefix`, `run_script`, `launch_argv`, `attach_argv`,
  `live_sessions_for`. `agent_profiles::launch_on_exe_dev` uses it.
- `sandbox::cli` is the GitVM CLI wrapper: `warm_up`, `public_url`,
  `state_is_stale`, `run`/`run_checked`, `pull`, `stop`. It has no PTY.
- `nautloom::loom_sandbox_stats` already proves the raw route into a GitVM
  sandbox: `ssh -J <state.json:jump> root@<state.json:guestIp>`, deliberately
  NOT `gitvm run`, because `gitvm run` rsyncs with `--delete` and would wipe
  the agent's own work.
- `spend::admit_launch` counts live agent SESSIONS and daily launches. It
  counts no machines.
- `status::adopt_surviving_runs` adopts local zellij runs only.

## Slice 3 — the GitVM launch driver

**3a. Extract the shared remote shape.** `exe`'s tmux half is not exe-specific:
session naming, the run-script body, `tmux new-session -A` vs
`tmux attach-session`, base64 script staging, and turning a
`tmux list-sessions` listing into one agent's runs. Move those to
`sandbox::remote`, parameterised by an ssh descriptor (argv prefix + host) and
a workdir. `exe` keeps only what is exe.dev's: the control plane (`ls --json`,
`new`, `restart`), `wait_ready`, and rsync `push`.

Success test: the existing `exe` tests keep passing against the moved code,
unchanged in what they assert.

**3b. `sandbox::gitvm` — the driver.** Same five verbs as `exe`:

| verb | GitVM |
|---|---|
| ensure an environment | `cli::state_is_stale` guard, then `cli::warm_up(dir)`; read `.gitvm/state.json` for `guestIp` + `jump` |
| put the work there | `warm_up`/`gitvm run` already rsyncs the directory in; the launcher pushes nothing further |
| run a command | `remote::run_script` staged over raw ssh, base64, into `/workspace/.xnaut/<session>.sh` |
| stream the output | local PTY hosting `ssh -tt -J <jump> root@<ip> tmux new-session -A -s <name> -c /workspace <script>` |
| tear it down | `cli::pull` THEN `cli::stop`, never the other way (XNAUT-40: teardown destroys `/workspace`) |

Staging goes over raw ssh rather than `gitvm run` for the `--delete` reason
above. The tmux session name is `remote::session_name(handle, run_id)` — the
same derivation local and exe.dev use, so one vocabulary covers all three.

**3c. `LaunchRoute::GitVm` + `agent_profiles::launch_on_gitvm`,** mirroring
`launch_on_exe_dev`: compose the prompt once (already done above the branch),
build argv+env via `agents::build_launch`, `remote_command` them into one line,
stage, open the PTY, `register_agent_session` with no local zellij name and no
local capture path.

`route()` then returns `Ok` for all three environments and the refusal survives
only for an environment that is not configured.

## Slice 4 — a ceiling that counts machines

`admit_launch` counts launches, so a paid provider accumulates VMs.

- `SpendCeiling` gains `max_live_environments`, default 2.
- New `sandbox::envs`: a JSON ledger at `<config>/remote-envs.json`, one row per
  `(env, handle, project)` — `created_ms`, `last_used_ms`, `workdir`, `session`.
  Claimed at launch, touched at attach, removed at reap. Same
  missing-file-means-defaults pattern as `switches.rs`/`spend.rs`.
- `envs::admit(env, handle, project)` reuses an existing row for the same
  (env, handle, project) — which is the isolation granularity the ticket asks
  for, one environment per agent per project reused across tickets, and the
  reason the warm cache is worth paying for. A NEW row past the cap is refused
  by name, listing what is live.
- `envs::reap_idle(idle_ms)` for rows whose session is gone: GitVM gets
  `cli::pull` then `cli::stop`; exe.dev is one persistent shared VM, so reaping
  there kills the tmux session and leaves the VM, which is the whole point of
  choosing it (the warm `target/` cache).
- Called from the same housekeeping tick that already sweeps sessions.

## Slice 5 — remote adoption across restarts

`adopt_surviving_runs` asks local zellij only, and `plan_adoption` would drop a
remote row on sight: a remote row has `zellij_session: None`, so `name` falls
back to the id, which IS `xnaut-<handle>-<run8>`, which is not in the local
live list — dead, dropped, every restart.

- `AgentSessionMeta` gains `remote_env: Option<String>` (the `LaunchEnv` key),
  set by the two remote launch paths.
- `plan_adoption` treats a row with `remote_env: Some(_)` as out of scope for
  the local liveness rule; its liveness comes from the remote listing instead.
- Adoption reads `envs`' ledger (so it asks only environments that were used,
  not the network at large), calls each driver's `live_sessions_for`, and
  adopts the names it does not already have — the same body as the local loop,
  with `remote_env` set and no capture path.
- An unreachable environment leaves its rows ALONE rather than dropping them.
  `exe::live_sessions_for` already separates "unreachable" (ssh 255) from "no
  session" (tmux 1) for exactly this reason; the GitVM driver copies it.

## Slice 6 — the deletions

Without these the ticket has added a path rather than removed one.

- `nautloom::loom_sandbox_stats` hand-rolls the jump-host ssh argv. It becomes
  `sandbox::gitvm::ssh_argv(&state)` — one place builds that command line.
- `AGENT_RUNNER`'s tmux block hardcodes session `nautloom` and its own
  attach story. It moves to `remote::session_name`, so a loom's agent is
  findable by the same rule as every other run.
- `multiagent-pane.js::launchSwarm` builds a worktree and calls `loom_run` with
  a hand-built script and a hardcoded `provider`. The launch half becomes
  `invoke('agent_profile_launch', …)`, which is where the environment option
  now lives; the swarm keeps choosing tickets and worktrees.

**The open question in slice 6.** NautLoom's loom pipeline does more than
launch: it records a video, pulls artifacts, ships a branch and opens a PR.
Routing the swarm through `agent_profile_launch` deletes the duplicate launch
but also, if done naively, that pipeline. My plan is the conservative split:
the swarm's *launch* goes through the one launcher, and the loom's post-run
steps (ship, PR, artifacts) stay as they are, triggered when the launched
session finishes. If reviewers think the video/artifact pipeline should move
too, that is a bigger slice and I would rather do it as its own ticket than
smuggle it in here.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml` green.
- `XNAUT_TEST_PORT=4291 npx playwright test` green.
- Unit tests, all offline (no test may need a VM):
  - a local pin still resolves Local and takes the unchanged path (the existing
    slice-1 guarantee, re-asserted after `LaunchRoute` grows a variant);
  - `gitvm` first in settings resolves GitVm and `route()` returns `GitVm`;
  - the GitVM launch argv contains `-J <jump>`, `root@<ip>`, `tmux
    new-session -A -s xnaut-<handle>-`, and `-c /workspace`;
  - the attach argv uses `attach-session` and never `new-session`;
  - teardown calls `pull` before `stop` (order asserted through a recorder, not
    a live sandbox);
  - `envs::admit` reuses a row for the same (env, handle, project) and refuses
    a third distinct environment at a cap of 2;
  - `plan_adoption` does not drop a remote row when local zellij is empty;
  - an unreachable environment does not drop its rows.
- By hand: with no `sandboxes` entry, launching @claude behaves exactly as
  today (this is the regression that matters); with `gitvm` listed first and an
  api key, a launch opens a pane showing the agent inside the sandbox, and
  quitting the app then reopening it re-adopts that run.
