# XNAUT-107 implementation-only plan

@codex; assigned branch agent/codex/xnaut-107, session 7406d5ef-a4c4-4de7-87ca-f2b067436c33.

This replaces the prior plan for code review only. In direct response to Codex reviewer note 3, Mesh notification, typed handback, and ticket state updates are EXCLUDED from the actions this plan authorizes. Their authorization question stays on the prior pending request. No push, merge, publication, deployment or release.

## Scope and containment
All edited files, fixtures, command working directories and local commits stay in this assigned worktree. Implement per-invocation hook isolation for dedicated unattended Claude commands: personas and Validator review, build planner, Designer, multiagent worker, NautLoom runner. Agent Space uses --print for human conversations with safety veto hooks and is unchanged, as are all interactive terminal builders. No global/user settings writes.

## Implementation and dependency audit
1. Inspect xNAUT hooks and launcher environments before applying the profile. loom_run tracks dedicated tasks through process state and result polling; it does not install session hooks or mint hook tokens. xnaut-veto.sh is active only with XNAUT_VETO_URL. Keep hooks whenever an active xNAUT hook/veto context is present, using a conditional per-command settings value, so nested managed launches retain their guardrails. Otherwise pass --settings JSON disableAllHooks:true on these dedicated commands. Preserve tools, model, prompt and resume behavior. Do not remove administrator-managed hooks.
2. Add strict empty MCP configuration to the build planner, matching personas and Designer. Preserve build-worker MCP behavior.
3. Test actual UI persona invocation and command-capture fixtures for every changed builder. Parse shell argv and settings JSON (including NautLoom nested AGENT quoting), confirm profile disabled hooks in ordinary task environment and retained them with XNAUT_VETO_URL, confirm model/resume/prompt and intended MCP flags. Confirm Agent Space builder remains unmodified with safety hooks. Remove profile temporarily to prove regression goes red, then restore.
4. Use a real Claude CLI with an isolated two-second SessionStart hook writing a fixture marker. Baseline must write marker and minimal run must not; both must emit success. Record wall time, duration_ms, duration_api_ms, ratio. Require absence of hook and delay; report inconclusive timing if API noise dominates. Historical 125s overhead did not reproduce in one existing baseline (3264ms wall; duration_ms 2173; duration_api_ms 2968). These duration fields can be non-comparable, so do not manufacture a claimed 125s saving.
5. Run full cargo test --manifest-path src-tauri/Cargo.toml (including binary suite) and XNAUT_TEST_PORT=4291 npx playwright test. Write .xnaut/bundles/XNAUT-107.md containing paths, suite totals, live evidence, and manual verification. Make a local commit of finished work only in assigned branch. No outward action is part of this plan.

## Spend and stop conditions
Maximum $2 additional measurement spend, at most THREE additional real Claude subprocesses, each --max-turns 1, --max-budget-usd 0.50, and timeout 60 seconds. Stop measuring on auth failure or exhausted cap. Local suites request no paid model runs. Measurements and temporary settings are contained under .xnaut/measurements/XNAUT-107/. No owner configuration is changed. If required suites or real verification fail, record that in the local bundle; do not claim success.
