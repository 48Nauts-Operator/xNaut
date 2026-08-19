---
Author: Claude Opus 5 (agent/xnaut-202)
Last modified: 2026-08-19 04:20 CEST
---

# XNAUT-202: surfaces that claim more than the code does

Five of the six items were real and are fixed. One was wrong about the code and
is answered with the reads rather than a change.

Gates, all run in this worktree after the last edit:

- `cargo test --bin xnaut` → 499 passed, 0 failed, 20 ignored.
- `cargo clippy --bin xnaut` → 0 errors, 23 warnings, all pre-existing (dead
  code and style in agent_hooks, canvas, composer's `BUILD_MARKER`, state,
  vault_tools, veto, agents, build_log, chat, decisions, gate_score, inbox,
  plugins, settings). None sits in code this ticket touched.
- `npx playwright test` → 91 passed. The worktree had no `node_modules`, so
  `npm install` came first; `package-lock.json` is unchanged.
- `node scripts/mutation-check.cjs` and the two slow entries → all 8 new
  mutations caught. Evidence below.

## 1. Collaborators said "Enforced at dispatch"

**Read first.** `src/js/agent-space.js:801` writes `collab:<handle>` into
`capabilities`. Nothing in `src-tauri/src/` read that prefix: `composer.rs:26`
strips `skill:`, `plugins.rs:1431` strips plugin ids, and there is no hand-off
tool in `agent_tools::tool_specs` and no dispatch-time check anywhere. The list
was a setting with no consumer, under a label claiming enforcement.

**Changed.** `composer.rs` gains `collaborators_block`, so the handles the owner
picked are named in the prompt the agent is launched with, next to its skills
and its policy limits. The help text now says advisory and says why: nothing
blocks a hand-off, so a determined agent can still ask someone else. Prompting
is not enforcement and the copy no longer pretends otherwise.

**Proved.** `composer::tests::the_collaborators_the_owner_picked_reach_the_prompt`
asserts the section arrives with both handles and that an empty list produces no
empty heading. Deleting the one `out.push_str(&collaborators_block(profile))`
turns it red (`caught`). A second check,
`scripts/surfaces-honest-smoke.cjs`, fails if the word "enforced" returns to that
help string, or if no Rust file reads the `collab:` prefix again.

## 2. "Chat turns do not use plugins"

**Read first.** `agent_tools.rs:681` in `run_turn` calls
`mcp_client::open_for(capabilities)` and puts the plugin tools next to xNAUT's
own, and `agent_tools::tests::a_connected_plugin_can_actually_answer_the_question`
already covers it. The sentence has been false since that call landed. It
appeared twice: `plugins-panel.js:174` and `agent-space.js:1780`.

**Changed.** Both now say a chat turn opens them too. The harness half of the
plugins-panel sentence is untouched and still true (`plugins.rs:1511` and
`:1522` build `--mcp-config` and `-c mcp_servers.<id>=…`).

**Proved.** `scripts/surfaces-honest-smoke.cjs` reads both sides: it asserts
`run_turn` still opens plugins, then fails on any surface saying chat turns do
not, don't or never use them. Restoring the old sentence turns it red.

## 3. "The plugin library calls it attestation": the ticket is wrong here

No change, because there is nothing overclaiming.

I read every occurrence of attest/attestation outside the icon blobs. The plugin
library's entry is `plugin-catalog.json:2897` "Securosys Attestation", and it
describes a third-party MCP server, not `plugins::verify`. That server is
`mcp/securosys-attest.py`: it POSTs to `/v1/synchronousSign` on a Securosys
Primus HSM (`:52-56`), sha256s the subject text (`:109`), and appends receipts to
`attestations.jsonl`. Something does sign. `scripts/securosys-attest-smoke.cjs`
exercises it and is already in the mutation harness.

`plugins::verify` (`plugins.rs:1075`) is separate and is never called
attestation. Its one caller is the connect path (`plugins.rs:1309`), which
reports it as `"verified": detail`, and the tool description at
`agent_tools.rs:64` says "PROVE it starts". That is what it does. Renaming a
correctly named thing would have made the app less honest, not more.

## 4. `ledger_recent` had no reader

**Read first.** Registered at `main.rs:177`, allowed at
`permissions/default.toml:754`, and called from no JS file. The only writers are
`veto.rs:236`, `:242` and `:265`, so the recorded kinds are exactly `conflict`,
`refused` and `asked`; an allowed call writes nothing. `elapsed_secs` therefore
never fills in either, because it keys off a `dispatched` entry that nothing
records.

**Built the pane, in Settings → Guardrails.** It goes under the policy editor and
the "Try a call" box rather than into a new right-pane tab: the question "what
have my rules actually done" belongs beside the rules that caused it, and the
existing Decisions view is deliberately not a log tail (its header comment says
so). Twelve entries, newest first, each with kind, agent, the reason the agent
was handed, and a readable age. The section states in the page that only a
refusal, a question and a conflict are recorded, so an empty list reads as "no
rule has fired" rather than "nothing ran". Elapsed time is not rendered, since
nothing records the dispatch it would be measured from.

**Proved.** `scripts/ledger-view-smoke.cjs` drives the real renderer with a
stubbed backend and asserts the command is called with a limit, that the kind,
the agent and the reason reach the DOM, that the timestamp is made readable, and
that the page carries the "an allowed call records nothing" caveat. Removing the
single `await loadLedger()` turns it red.

## 5. The work-log summary was unreachable

**Read first.** Two bugs, one root cause. `worklog_stop` (`worklog.rs:496`) sets
`*active = None` before returning, and `worklog_summary` and `worklog_qr` served
the active session only. So both errored every time the panel called them.
`app.js:1215` swallowed that into `session.generate_summary`, which is a Rust
method, never a serialised field of `WorkSession`, so it was `undefined` and the
fallback one-liner always won. The fetched `summary` variable was then never
used at all. The QR had the same fate one line down, and the panel still printed
the "Verification QR Code" heading and "scan to verify" over nothing.

**Changed.** `worklog_summary` and `worklog_qr` take an optional `session_id`;
the choice lives in a plain function, `pick_session`, so it is testable without a
Tauri `State`. Omitting the id keeps the old behaviour (the running session).
`worklog_dir()` honours `XNAUT_WORKLOG_DIR`, the trick `ledger.rs:39` already
uses, so the tests stage sessions in a temp dir instead of `~/.xnaut/worklogs`.
The panel passes the id of the session that just stopped, renders the summary it
fetched, and only prints the QR heading when a QR actually rendered.

**Proved.** `worklog::tests::a_stopped_session_can_still_be_summarised` asserts a
finalized session read by id yields the real summary, with its commands and its
merkle root, and a non-empty QR.
`worklog::tests::a_running_session_is_still_the_default` pins the unchanged path.
Making the id branch error again turns the first red (`caught`). The frontend
half is covered by `scripts/surfaces-honest-smoke.cjs`, which parses the
`WorkSession` struct out of `worklog.rs` and fails if `toggleWorkLog` reads any
`session.<name>` that is not a serialised field. Putting
`session.generate_summary` back turns it red.

## 6. `codex_spend` was never called

**Read first.** Registered at `main.rs:366`, allowed at
`permissions/default.toml:431`, no JS caller. The USD estimate existed only in
Rust.

**Changed.** The usage footer asks for the newest session and renders `~$N last
run` beside the Codex block. Two constraints came from `codex_spend.rs:41-49`,
which says the figure is notional and any UI showing it must say which of the two
it means: the tooltip says "at API list prices … not money billed to a
subscription", and a model with no known price renders nothing at all rather
than `$0.00`.

**Proved.** `scripts/codex-spend-footer-smoke.cjs` drives the real footer with a
stubbed backend: it asserts `codex_spend` is invoked, that the figure and the
list-prices wording reach the strip, and that an unpriced model puts no dollar
sign on screen. Both a dropped `invoke` and a `cost_usd || 0` coercion turn it
red.

## Mutation evidence

`node scripts/mutation-check.cjs`, plus the two slow entries run the same way
against an rsync'd copy:

```
caught    XNAUT-202 the Collaborators tab claims enforcement again
caught    XNAUT-202 the plugin library says a chat turn skips plugins
caught    XNAUT-202 the work-log panel renders a Rust method name again
caught    XNAUT-202 the decision ledger loses its only reader
caught    XNAUT-202 the footer stops asking what the last codex run cost
caught    XNAUT-202 an unpriced codex model reads as a free one
caught    XNAUT-202 the collaborators stop reaching the prompt
          test composer::tests::the_collaborators_the_owner_picked_reach_the_prompt ... FAILED
caught    XNAUT-202 a stopped work session cannot be summarised
          test worklog::tests::a_stopped_session_can_still_be_summarised ... FAILED
```

Three unrelated entries report `BASELINE` in a fresh checkout of this worktree
(`a context menu ignores the interface zoom again`, `terminals stop cancelling
the interface zoom`, `a settings renderer loses its only call site`). All three
are Playwright checks and all three failed only because the worktree had no
`node_modules` for the harness to link. After `npm install` the full Playwright
suite passes, so they are an environment artifact, not a regression.

## What I did not do

- Nothing was pushed, tagged or merged. Commits are local to `agent/xnaut-202`.
- The Collaborators list is advisory, not enforced. Enforcing it would need a
  hand-off tool to enforce at, and there is none; inventing one is a different
  ticket from making a label honest.
- `elapsed_secs` on the ledger stays dead until something records a `dispatched`
  entry. I left it unrendered rather than showing a column that is always empty.
- The 23 clippy warnings and the 58 eslint errors already in the tree are
  untouched. All of them are outside what this ticket changed.
