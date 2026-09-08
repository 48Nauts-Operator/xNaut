# XNAUT-266 — one launch path, the environment as an option

@claude, branch `agent/claude/xnaut-266`, based on `2fcee16`. Slices 1 (the
`launch_env` seam) and 2 (the exe.dev PTY) were already on the lineage. This is
what triage said remained: the GitVM leg, isolation granularity, lifecycle,
remote adoption, and the deletions.

## What changed

### 1. GitVM is now a launch environment, not a refusal

`LaunchRoute` gained `GitVm`, and `agent_profiles::launch_on_gitvm` drives it in
the same shape as exe.dev: tmux inside the sandbox owns the agent, a local PTY
hosts an `ssh -tt` that is only the viewport. `route()` now returns `Ok` for all
three of the owner's options and the only refusal left is the honest one —
resolved to an environment that is not configured.

The driver lives in `sandbox::cli` (`guest`, `ssh`, `push`, `stage_script`,
`run_script`, `launch_argv`, `attach_argv`, `live_sessions_for`). It speaks the
raw jump-host ssh the CLI itself speaks, because `gitvm ssh` opens an
interactive shell and takes no command, and because `gitvm run` rsyncs with
`--delete` and would erase the agent's own work on every probe. Everything
remote runs `sudo -u user -H bash -lc`, matching `gitvm run`: `/workspace` is
`user`-owned, and an agent running as root would hand back files the pull cannot
overwrite.

### 2. One environment per agent per project

`launch_env::project_root` resolves a worktree to the repository that owns it
(`--git-common-dir`, not `--show-toplevel`, which inside a linked worktree
returns the worktree). `exe::agent_workdir(handle, repo)` keys the remote
directory by agent and repository rather than by worktree path, so an agent's
next ticket lands in the same directory and finds `target/` and `node_modules`
as it left them — `push` excludes both, so the mirror never touches them. That
is the three-minutes-instead-of-thirty the ticket names. Two agents still get
two directories, because `push` mirrors with `--delete`.

GitVM keeps the CLI's own granularity (one sandbox per directory), because a
GitVM sandbox is a fresh VM from a template with no cache to keep warm. What its
accumulation costs is money, which is slice 3's job.

### 3. A ceiling that counts machines

`spend::admit_launch` counts launches, and a launch count cannot see a running
VM: twenty launches that reuse one environment cost one, twenty that each create
a sandbox cost twenty, and both look identical to a launch counter.

`sandbox::launch_env::live` is a JSON ledger at
`<config>/xnaut/launch-environments.json`, one row per `(env, handle, project)`.
`SpendCeiling` gained `max_live_environments` (default 2, per provider).
`agent_profiles::admit_environment` reaps idle rows (45 min) and *then* admits,
so the cap throttles a fleet that is busy now rather than one that once was.
Reuse is always admitted — that is the granularity rule paying for itself.
Reaping honours XNAUT-40: `live::reap` pulls before it destroys and refuses to
destroy anything whose pull failed, and never destroys exe.dev, where one
persistent VM and its warm cache are the point.

`Local` never enters the ledger. Nothing accumulates and nothing is billed on
the owner's own Mac.

### 4. Remote adoption across restarts

`AgentSessionMeta` gained `remote_env: Option<String>`. It is load-bearing: an
adopted remote row is name-shaped exactly like a dead local one ("xnaut-…",
label "· adopted", absent from local zellij), so `plan_adoption` would have
dropped a working agent on the very next tick. Remote rows are now out of scope
for the local liveness rule.

`status::adopt_remote_runs` runs after the local pass at startup. It asks the
ledger which environments the launcher actually created, asks each one what tmux
still has, and adopts what it finds. Nothing remembers a session name — names
are rebuilt from the handle with the same derivation the launch used, which is
why this works after the session map is gone. An unreachable environment leaves
its rows and its ledger entry alone; the drivers separate "unreachable" (ssh
255) from "no session" (tmux 1) by exit code precisely so this can.

`agent_remote_sessions` / `agent_remote_attach` are no longer exe.dev-specific:
they resolve the environment the same way a launch does.

### 5. The deletions

Four copies of "which agent binary, and what flags does it need to run
unattended" existed: `project-management-panel.js` (twice),
`designer-agent.js`, `multiagent-pane.js`, and a `case "$MODEL" in codex*)`
inside `nautloom.rs`'s `AGENT_RUNNER` heredoc. There is one now:
`agents::headless_command`, with `agents::runtime_for_model` doing the
model-string → runtime translation each copy also had. The panes call it through
the new `agent_headless_command`; `loom_run` stages the line into
`.loom-agent-cmd.txt` and the session name into `.loom-session.txt`, and
`AGENT_RUNNER` reads both and refuses if the line is missing.

The two options the four copies actually differed on are named concepts rather
than pass-through flags: `resume` (continue an earlier session, sanitised in one
place now) and `isolate_mcp` (no user MCP servers).

The loom's sandbox session name is now `launch_env::session_name("nautloom", …)`
rather than the hardcoded word `nautloom`, so a loom agent in a sandbox is found
by the same adoption rule as everything else.

Three copies of the jump-host ssh command line collapsed to one:
`cli::ssh_opts` + `cli::guest`, used by the new driver, by
`cli::expose_local_port`, and by `nautloom::loom_sandbox_stats` (each had its own
timeout and its own default jump host).

**Found while consolidating, not a bug:** `multiagent-pane.js` sent
`--allow-dangerously-skip-permissions` where the other three sent
`--dangerously-skip-permissions`. Both are real Claude Code flags, so nothing was
broken — but four copies drifting on a permissions flag with nobody noticing is
the failure mode, and the spelling is now asserted rather than assumed.

### 6. The first-run wizard on a machine we do not own

A launch environment that is not this Mac starts an agent CLI that has never
run there. Measured on the exe.dev VM 2026-09-03: `claude` opened its first-run
THEME PICKER and sat on it, ahead of any prompt, so the pane showed a wizard
instead of an agent. The local path has fixed this class twice already — codex
on the wake workspace (XNAUT-274) and gemini on tron (2026-09-06 14:51) — but
never for a machine we do not own.

`launch_env::onboarding_seed(cfg)` is that answer as shell, run by both remote
run scripts immediately after their `cd`. Two things make it more than the
local seeders called over ssh, and both are the reason it is its own function:

1. **The remote needs MORE.** Locally the CLI has already been run by the
   owner, so only per-project trust is missing. A fresh image has never run it,
   so `hasCompletedOnboarding` — the theme picker, the thing actually measured
   — has to be seeded too. A remote seed that merely copied the local one would
   still hang.
2. **It must MERGE, never overwrite.** `gitvm warm-up --authSync` copies
   credentials into `~/.claude.json`. A seeder that wrote the file fresh would
   log the agent out of the very machine it was preparing — trading a wizard
   for an auth prompt.

`$PWD` names the trusted directory rather than a path computed here: on exe.dev
the workdir sits under a `$HOME` this side cannot know, so asking the far side
is exact where computing would be a guess. It reaches python through the
environment, not string interpolation, so a directory containing a quote is
data rather than syntax.

Codex's branch is pure shell and needs no interpreter, so the one runtime whose
config is TOML cannot fail for want of python. Cursor and Copilot get nothing,
matching `apply_preflight_trust`, which says out loud that it has no artifact
writer for them: an invented one, unexercised against those CLIs, would be a
guess written into a config file — worse than the wizard it replaced.

## Suite results

Run from the worktree root:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
XNAUT_TEST_PORT=4291 npx playwright test
```

- **Rust: 1005 passed, 0 failed, 42 ignored**, 6.61s. (998 before this slice;
  the 7 new ones are `launch_env`'s onboarding tests.)
- **Playwright: 132 passed, 0 failed**, 2.6 minutes. `npm install` was needed
  first — this worktree had no `node_modules`.
- `cargo clippy --bin xnaut`: **0 errors, 45 warnings, and 45 on the base
  commit too** — measured by stashing, so the count is a comparison rather
  than an adjective. None of them names the new code.

### The live leg, on a real machine

The gap this ticket carried was "nothing was run against a live machine", and
this slice is remote-only shell whose risks are exactly the ones a Mac cannot
show: is python3 there, does `grep -qxF` behave, is bash what we assumed.

So the generated seed — dumped from the code, not hand-written — was executed
on a real GitVM sandbox (`sb-8682a6ae`, template `agent-desktop`), created for
this check and destroyed after it:

```
== uname: Linux 5.10.223        == bash: 5.2.21(1)-release
== python3: /usr/bin/python3 Python 3.12.3
== PWD: /workspace
== claude seed exit: 0
== onboarding answered: True
== trust for pwd: True
== pre-existing keys lost: NONE
== codex stanza count: 1        (after running the seed twice)
== codex trust_level line: trust_level = "trusted"
```

The merge guarantee is the result worth having. `warm-up` ran its `authSync`
regardless of the `"authSync": []` in `.gitvm.json`, so that sandbox held a
REAL Claude config — 71 keys including `oauthAccount`. After the seed, **none
of them was lost**. That is the credential-survival rule proven against a
genuinely synced config rather than a fixture.

One honest limit on what this proved: because authSync had already placed a
config there, `hasCompletedOnboarding` was present before the seed ran, so the
live run demonstrates the merge, the idempotence and the Linux compatibility —
not the never-run-at-all case. That case stays covered offline, by
`a_fresh_machine_gets_claudes_onboarding_answered_and_not_only_its_trust`.

No `pull` preceded the `stop`. XNAUT-40's rule protects an agent's work in
`/workspace`; this sandbox was warmed from a scratch directory, nothing was
built in it, and the source of truth never left this Mac.
- `git diff --check`: clean.

Not clean, and not clean on the base commit either — verified by stashing:
`cargo fmt -- --check` reports diffs in 70 files; `npm run lint` reports the same
56 pre-existing errors; `node scripts/hygiene-check.mjs` fails the same check
(`foundation.rs::the_handback_route_gets_a_real_url`, `main.rs::print_startup_banner`).
None of those name a file this change touched for a reason this change caused.

One flake seen once and not since, on this change and unrelated to it:
`veto::tests::every_decision_writes_one_verifiable_record` failed in one full run
("record does not hash to its own hash") and passed alone and in five subsequent
full runs. `XNAUT_EVIDENCE_DIR` is process-global and that test's scratch lock is
not taken by every test that writes evidence.

## Mutation checks

Two, on the load-bearing new rules:

1. Removed `.filter(|row| row.remote.is_none())` from `plan_adoption`.
   `status::tests::a_remote_row_is_not_dropped_for_being_absent_from_local_zellij`
   failed: `left: ["xnaut-claude-remote1", "xnaut-claude-remote2",
   "xnaut-claude-local1"], right: ["xnaut-claude-local1"]`. Restored: green.
2. Removed the reuse short-circuit from `live::Ledger::admit`.
   `sandbox::launch_env::live::tests::a_new_environment_is_refused_at_the_cap_but_reuse_never_is`
   failed at `assert!(ledger.admit("gitvm", "a", "/p1", 2).is_ok())`.
   Restored: green.
3. Dropped `d["hasCompletedOnboarding"] = True` from the claude seed — the
   theme picker returns.
   `launch_env::tests::a_fresh_machine_gets_claudes_onboarding_answered_and_not_only_its_trust`
   failed. Restored: green.
4. Replaced the seed's read-modify-write with a fresh `dict()`, i.e. made it
   overwrite. `launch_env::tests::seeding_merges_so_synced_credentials_survive_it`
   failed. Restored: green.

The onboarding tests EXECUTE the generated shell against a temporary `HOME`
and read the config files back, rather than asserting on the string. The bug
being prevented is a wizard on a machine the test cannot reach, and a seed that
parses but writes the wrong shape would satisfy a `contains` assertion while
still stranding the agent.

## How to verify by hand

**The regression that matters** — nothing configured must behave exactly as
before. With no `sandboxes` entry in
`~/Library/Application Support/xnaut/settings.json`, launch any agent from the
Agent pane. It runs in local zellij, on the path it took before this change.
`sandbox::launch_env::tests::a_local_pin_stays_local_whatever_is_configured`
holds the same guarantee with both providers configured.

**The owner's acceptance test** — "if I pay X USD for exe.dev then I want to use
it, always". Add `{"kind": "exe-dev", "base_url": "", "api_key": null}` as the
FIRST entry of `settings.sandboxes`, set a profile's execution to `sandbox`, and
launch. The pane opens an `ssh -tt` into a tmux session on
`nautbox-verify.exe.xyz`. Remove the entry and the same launch runs on the Mac
again, with no other switch to remember.

**GitVM** — list `{"kind": "gitvm", "base_url": "http://gitvmd-control-01.tail138398.ts.net:7070", "api_key": "…"}`
first instead. A launch warms the worktree's sandbox and opens a pane on
`tmux new-session -A -s xnaut-<handle>-<run8> -c /workspace`. `gitvm ssh` from a
terminal, then `tmux ls`, shows the same session name.

**Adoption** — with a remote run going, quit xNAUT and reopen it. The roster
shows `<handle> · adopted (exe-dev)` (or `(gitvm)`) and the row survives
subsequent ticks. Before this change the run kept working on the paid machine
and the app showed nothing.

**The machine ceiling** — set `max_live_environments` to 1 in
`~/Library/Application Support/xnaut/spend-ceiling.json`, then launch two
different agents remotely on two projects. The second is refused by name, listing
what is live and how long until it idles out. Launching the FIRST agent again on
the same project is admitted, because reuse is not a new machine.

**The consolidation** — `rg -n 'dangerously-skip-permissions|disableAllHooks' src/js`
returns only comments. `tests/headless-profile.spec.mjs` fails if any pane grows
its own copy back.

**The onboarding seed** — on any machine with python3:

```sh
cd /tmp && mkdir -p seedcheck && cd seedcheck
HOME=$PWD/fakehome bash -c '<the seed>'   # from launch_env::onboarding_seed
python3 -c 'import json;d=json.load(open("fakehome/.claude.json"));print(d["hasCompletedOnboarding"])'
```

prints `True`, and the directory appears under `projects` with
`hasTrustDialogAccepted`. Run it twice against a `~/.codex/config.toml` that
already has the stanza and it stays at one. To see the failure it prevents,
launch a remote agent against a machine whose `~/.claude.json` you have first
deleted: without the seed the pane opens on the theme picker and no prompt is
ever consumed.

## Not finished

- **`multiagent-pane.js`'s swarm still calls `loom_run`, not
  `agent_profile_launch`.** Its *command building* is consolidated, which is the
  duplication that could drift; its *launch* is not. `loom_run` is a headless
  batch runner (returns pid + log, streams stream-json into a pane) with six
  frontend callers including the Designer and every NautFlow persona, while
  `agent_profile_launch` returns an interactive PTY session. Converting the
  swarm alone means rewriting its "wait for `__LOOM_DONE__` then ship + PR"
  pipeline against session lifecycle, and I could not exercise that end to end
  here (it needs a live GitVM sandbox and a real agent). PLAN.md raises the same
  question and proposes the same conservative split. It is the last piece of
  slice 6 and it wants its own ticket or an explicit owner call.
- **The exe.dev and GitVM drivers still have their own `run_script` /
  `launch_argv` / `attach_argv`.** PLAN.md's 3a wanted a shared
  `sandbox::remote`. I shared the part that must not drift — session naming and
  `sessions_for_handle`, now in `launch_env` and re-exported by `exe` — and left
  the argv builders per-driver, because they differ in host, user wrapping and
  workdir resolution, which is what a driver *is*. Worth revisiting if a fourth
  environment arrives.
- **No full remote LAUNCH was executed against a live machine.** The seeding
  shell now has been (a real GitVM sandbox, above), which closes the part this
  slice added. What remains unexercised is the launch itself end to end: an
  agent actually started under tmux in a sandbox and adopted after an app
  restart. The exe.dev leg was exercised live on 2026-09-03 per the vault doc;
  the GitVM launch leg has not been, and its first real run may surface the
  same class of thing the exe.dev leg did (a relative `tmux -c` that failed
  silently). The tests covering it are offline by construction: argv shapes,
  ledger rules, and the bash parse of the generated command lines.

## Files changed

```
PLAN.md
src-tauri/gen/schemas/acl-manifests.json
src-tauri/permissions/default.toml
src-tauri/src/agent_profiles.rs
src-tauri/src/agents.rs
src-tauri/src/jury_proof.rs
src-tauri/src/main.rs
src-tauri/src/nautloom.rs
src-tauri/src/nudge.rs
src-tauri/src/sandbox.rs
src-tauri/src/scheduler.rs
src-tauri/src/spend.rs
src-tauri/src/status.rs
src-tauri/src/sweep.rs
src/js/designer-agent.js
src/js/multiagent-pane.js
src/js/project-management-panel.js
tests/headless-profile.spec.mjs
tests/persona-one-run.spec.mjs
tests/static-server.mjs
```
