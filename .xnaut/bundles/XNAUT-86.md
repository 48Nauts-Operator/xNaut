# XNAUT-86 — sessions band: pick an agent, resume a conversation, be found again

Branch `agent/claude/xnaut-86`, branched at `dev` (`bf55e17`).

XNAUT_TEST_TOTALS={"rust":[{"passed":1252,"failed":0,"ignored":45}],"ui":[250]}

## The ticket said "project page". It is in the Observatory, and that was asked

XNAUT-86 was written on 2026-08-07 and asks for a sessions band at the top of
the project page. On **2026-09-12**, five weeks later, XNAUT-340 removed exactly
that block and quoted the decision:

> MOVE, per Andre 2026-09-12: the session list with Connect and Kill goes to the
> Observatory, scoped by the project selected in the sidebar. The provider
> selectors and "Open another session" go with them… The Overview tab loses the
> block entirely rather than keeping a link to it.

That shipped as `917a76d`. Building the ticket literally would have reversed it,
so nothing was written until it was asked through the Mesh inbox
(`in-543bf61d-5efe-428e-bb01-3c219af46388`). The answer was **observatory**:
honour XNAUT-340, put XNAUT-86's substance where the sessions already live. The
project Overview is untouched by this ticket — `project-management-panel.js` is
not in the diff.

A second ask (`in-2dd14833-195b-48ff-bc9e-da115ebf79de`, the sandbox Connect
button) was opened first and is superseded by the shape below; it needs no
answer now.

## What the band could not do before

The Observatory's band already listed a project's sessions and could open one.
Three things were missing, and each is a different kind of missing.

**It could only ever open Claude.** `openNewSession` had `'cl-' + project.name`
and `zsh -ic 'claude; exec zsh'` written into it. On a machine that runs Codex
on half its projects there was nowhere in the app to start one.

**It did not say what resuming restores.** An exited session read
`exited · resumable`, which sounds like a shell in the right folder. It is not:
`zellij attach` rebuilds the session from its serialized layout, and that layout
carries the cwd and the exact command, `claude --continue` included. The
conversation comes back. 25 such sessions were restored from backup on
2026-08-07 and nothing anywhere said this.

**The name it chose was a name the sidebar could not match.** This one is the
real bug, and it was silent:

```js
const name = 'cl-' + String((project && project.name) || 'session');   // "cl-Bucky"
// …then zellij_open_command -> zellij::session_name, which LOWERCASES:
//    the session that exists is "cl-bucky"

// sidebar.js::sessionsFor, meanwhile, compared raw strings:
return name === proj || name.startsWith(proj) || proj.startsWith(name);
//     'Bucky' === 'bucky'  -> false, and so on for all three
```

So a session opened from xNAUT for `Bucky` ran fine, worked fine, and was
invisible to the sidebar: no live dot, no agent chip, no attach. **41 of the 45
projects in the control repo carry a capital in their name.** Nothing errors,
nothing logs, and the only symptom is a dot that stays dark — the same failure
class as the `window.*` globals note in CLAUDE.md.

## One rule, in one file

`src/js/session-naming.js` (new) holds the `<agent>-<project>` rule. It is the
Observatory's own matching rule, lifted whole — `slug` drops separators rather
than collapsing them, so `DAT.AG`, `dat-ag` and `datag` are one name — and the
sidebar's third copy of it is gone.

```js
function belongsTo(sessionName, entity) {
  const rest = stripAgent(sessionName);
  if (rest.length < 3) return false;
  return tokensOf(entity).some(
    (token) => token === rest || token.startsWith(rest) || rest.startsWith(token),
  );
}
```

Prefix matching runs both ways because zellij truncates at 24 characters:
`cl-nautflow-incident-loo` is shorter than its project, `cx-xnaut-safety-net` is
longer than `xnaut`. `tokensOf` accepts a PM project record (`name`/`key`/
`source_path`) or a sidebar task row (`name`/`id`/`path`), so the two surfaces
ask the same question without converting themselves into each other.

`nameFor` mirrors `zellij::session_name` exactly — lowercase, non-alphanumerics
collapsed to a dash, capped at 24 — because the caller labels a tab and looks the
session up before `zellij_open_command` has returned the real name, and a
divergence means xNAUT hunting for a session that was never created.

The agent list is data now rather than three hardcoded maps:

```js
const AGENTS = [
  { key: 'cl', label: 'Claude Code', command: 'claude' },
  { key: 'cx', label: 'Codex',       command: 'codex' },
  { key: 'pi', label: 'Pi',          command: 'pi' },
];
```

`xnaut` is deliberately not in it: that prefix belongs to dispatched fleet runs
(`xnaut-claude-01m2zr…`), which belong to a ticket and a worktree, and offering
to start one from a project picker would mint an agent with no work to do.

## The band

`renderSessions` / `openNewSession` in `observatory-panel.js`:

- an **agent select** (Claude Code / Codex / Pi) whose choice survives the
  repaint that follows opening a session;
- the name comes from `rule.nameFor(agent.key, project)` and the command from
  `agent.command`, so `cx-bucky` runs `codex`;
- **the provider pair is Claude's** and is now disabled for anyone else. It sets
  `ANTHROPIC_*` and registers a NautGate Max launch; handing that to Codex or Pi
  points them at an endpoint they do not speak, and the hint says so by name;
- running sorts above resurrectable, each newest first;
- an exited row reads `exited · Resume restores the conversation, not a shell`;
- the empty state names the invariant: opening a session *always* starts it
  inside zellij, because it has to outlive the app. Not a checkbox — the ticket
  is explicit and so is the copy.

## Connect, and why it is disabled rather than absent

The ticket asks for "Open (local) or Connect (sandbox, when the project has
one)". There is no per-project sandbox record anywhere; what exists is
`launch_env::status_of`, which is where "configured" is defined. A UI reading
`settings.sandboxes` itself would call a `gitvm` entry with no api key ready and
offer a button that can only fail — so it asks:

```rust
#[tauri::command]
pub fn launch_env_options() -> Vec<EnvOption> {
    env_options(&crate::settings::load_or_default().sandboxes)
}
```

`EnvOption` carries `ready` **and** `detail`, and `detail` is the sentence the
refusal itself would use. With nothing configured — which is this machine, where
`settings.sandboxes` is `[]` — Connect renders disabled and titled
`exe-dev: no "exe-dev" entry in settings.sandboxes · gitvm: no "gitvm" entry in
settings.sandboxes; a CLI key alone does not opt the fleet in`. Every remote
environment is named, the way `not_configured()` does in Rust: quoting only the
first would answer "no exe-dev entry" to somebody reaching for GitVM.

It reads "Connect to sandbox", not "Connect", because the row buttons already
say Connect for attach and two different Connects in one band is worse than a
longer label.

An `EnvOption` reported ready is an option that routes — asserted directly, so
the band and the launcher cannot hold two answers to "can I run here".

## Files

- `src/js/session-naming.js` — new, the one rule
- `src/js/observatory-panel.js` — agent picker, naming, sorting, exited copy, Connect
- `src/js/sidebar.js` — `sessionsFor` calls the rule instead of carrying a copy
- `src/index.html` — loads `session-naming.js` before every surface that uses it
- `src-tauri/src/sandbox.rs` — `launch_env_options` / `env_options` + 2 tests
- `src-tauri/src/main.rs`, `src-tauri/permissions/default.toml` — the command's
  other two required edits (`xnaut/adding-a-command`)
- `src-tauri/gen/schemas/acl-manifests.json` — regenerated by the build
- `tests/static-server.mjs` — `launch_env_options` stub, this machine's real answer
- `tests/sessions-band.spec.mjs` — new, 14 tests

## Totals

From my own runs, in this worktree:

- `cargo test --manifest-path src-tauri/Cargo.toml` — **1252 passed, 0 failed,
  45 ignored** (1250 before; +2 Rust tests). Run three times, identical.
- `XNAUT_TEST_PORT=4392 npx playwright test` — **250 passed** (236 before;
  +14 UI tests).
- `npx eslint src/js` — 57 pre-existing errors, byte-identical before and after;
  my three files are clean.
- `node scripts/hygiene-check.mjs` — 2 pre-existing failures
  (`vault-pane.js:491`, three unattributed Rust test fns), identical before and
  after, verified by stashing.

### Mutations, to prove the tests bite

Each was applied, run, and restored green.

| mutation | result |
|---|---|
| `slug` stops lowercasing (the original case bug) | 1 failed |
| `openNewSession` hardcodes `'cl-'` and `claude` again | 2 failed |
| the `agent.key === 'cl'` guard on NautGate removed | 1 failed |
| the running-above-exited sort removed | 1 failed |
| `nameFor` stops truncating at 24 | 1 failed |
| `sidebar.js` restored to its old case-sensitive rule | 1 failed |
| Rust `EnvOption.ready` hardcoded `true` | 2 failed |

### Two things worth knowing about the runs

**The port.** `XNAUT_TEST_PORT=4291` from the dispatch note was **held by another
worktree's server** (`lsof -nP -iTCP:4291 -sTCP:LISTEN` → node, pid 72635), and
`playwright.config.mjs` sets `reuseExistingServer: true`, so running there would
have silently tested that worktree's frontend. Exactly the trap XNAUT-87's bundle
recorded. These numbers are from **4392**, verified free first.

**One concurrent failure, not reproducible.** A cargo run launched while the
Playwright suite was running reported `1251 passed; 1 failed`. Three sequential
runs since are `1252 passed; 0 failed`. The two suites share this machine's HOME,
vault and settings, so I read it as cross-suite interference rather than a defect
— but I did not capture the test name before it went green, so that reading is
inference, not evidence.

## How to verify by hand

1. `cd <worktree>/src-tauri && cargo tauri dev`
2. Open the Observatory. Pick a project in the Sessions header.
3. The opener has four controls: agent, provider, model, Open — plus a disabled
   **Connect to sandbox**. Hover it: it names both remote environments and why
   neither is reachable.
4. Choose **Codex** → provider and model grey out and the hint reads "Codex runs
   with its own configuration; the provider pair is Claude's."
5. **Open a new session** → a tab appears labelled `cx-<project>` (lowercase),
   running `codex` inside zellij. `zellij ls` shows the session.
6. Open the sidebar: that project now carries a live dot and a `cx` chip. Before
   this ticket it stayed dark for any project with a capital in its name.
7. Quit the agent, close the tab, `zellij kill-session` it, then reopen the
   Observatory: the row is back as `exited · Resume restores the conversation,
   not a shell`, above the opener and below anything still running.

## Deliberately not done

- **No sandbox launch path.** Connect is honest about being unreachable, and
  nothing more. Opening an owner's interactive session *inside* a sandbox means
  new Rust (`cli::guest` + `stage_script` + `launch_argv`, the shape
  `launch_on_gitvm` uses for fleet runs) and `settings.sandboxes` is `[]` on
  tron, so it would have shipped unverified. The seam it would plug into —
  `launch_env_options` — exists and is tested.
- **The project Overview still has no sessions band**, by decision, not by
  omission. If that is ever reversed, the rule and the agent list are already
  factored out; it is a band's markup and a call to `belongsTo`.
- **`attribute()` source 3 in the Observatory still has its own token loop.** It
  applies the same rule but has to pick one project out of many rather than
  answer yes/no about one, so converging it means reimplementing the best-match
  tie-break. It agrees with `belongsTo` today; it is not shared code.
- **A PM-board project with no task-registry row still gets no sidebar dot**, for
  a hand-started session. `dotStateFor` is only reached for rows that have a
  task, and opening a session from the band registers one
  (`tasks_create_project`), so the flow this ticket owns is whole. Lighting
  PM-only rows is a sidebar change with its own blast radius.
- **Kill keeps its two-click arming** and was not touched: killing an agent from
  here must not get easier (XNAUT-340).
