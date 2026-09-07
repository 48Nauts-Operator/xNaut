# XNAUT-107 test bundle

Author: @codex
Date: 2026-09-07
Session: 7406d5ef-a4c4-4de7-87ca-f2b067436c33
Branch: agent/codex/xnaut-107

## Change

Dedicated unattended Claude tasks now pass an invocation-only settings profile disabling inherited hooks. This covers NautFlow personas (including Validator/reviewer), build planner, Designer, local multiagent workers, and the NautLoom sandbox runner. The planner also uses strict empty MCP configuration, matching personas and Designer; workers retain their existing MCP access.

The profile is an empty settings overlay when XNAUT_VETO_URL or XNAUT_HOOK_TOKEN is present, preserving hooks for managed/nested runs. Interactive terminals and conversational Agent Space are unchanged. No global/user settings are written. Claude's administrator-managed hook policy still takes precedence.

Hook dependency audit: loom_run tracks these tasks through process state, logs and result polling, and does not install hooks or mint a hook token. xnaut-veto.sh needs XNAUT_VETO_URL; status and brief scripts use session authentication. The conditional overlay preserves all inherited hooks when this managed context is present. Ordinary unattended task prompts already contain their task/context and use file tools. User/plugin startup, discovery and teardown hooks are not needed by that execution contract.

Files:

- src/js/project-management-panel.js
- src/js/designer-agent.js
- src/js/multiagent-pane.js
- src-tauri/src/nautloom.rs
- tests/persona-one-run.spec.mjs
- tests/headless-profile.spec.mjs
- scripts/measure-headless.py
- .xnaut/bundles/XNAUT-107.md
- .xnaut/plans/XNAUT-107.md
- .xnaut/measurements/XNAUT-107/results.json

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml`: exit 0; 959 passed, 0 failed, 41 ignored, 0 filtered. Includes the xnaut binary suite. Existing compiler warnings remain. Test execution 6.75s after compilation.
- `XNAUT_TEST_PORT=4291 npx playwright test`: exit 0; 133 passed, 0 failed, 0 skipped, 2.6m.
- `XNAUT_TEST_PORT=4291 npx playwright test tests/headless-profile.spec.mjs tests/persona-one-run.spec.mjs`: 7 passed. Actual persona UI invocation is inspected. Production expressions for all five command paths are evaluated and executed through bash into an argv capture. Tests parse settings/MCP JSON, preserve prompt/model/resume, cover both managed-context signals and NautLoom's nested shell, and check interactive Agent Space remains unchanged.
- Mutation: temporarily replaced the persona profile's disableAllHooks JSON with `{}`. `XNAUT_TEST_PORT=4291 npx playwright test tests/headless-profile.spec.mjs --grep 'persona and Validator'` exited 1, with 1 failed assertion (expected disableAllHooks:true, received {}). Restored production code before final full suites.
- `node --check` on all three edited JavaScript files and `git diff --check`: exit 0.
- Initial UI attempt lacked @playwright/test. `npm ci` installed 107 packages, audit found 0 vulnerabilities. Baseline suites before implementation: Rust 959 passed/41 ignored; UI 127 passed.

## Real CLI measurement

`python3 scripts/measure-headless.py` exited 0 using Claude Code 2.1.226. It resolves the production profile, runs a controlled two-second SessionStart hook and compares the hook marker and result event against the minimal profile. Three calls maximum, one turn each, $0.50 ceiling each, 60-second timeout each. Recorded model cost for these three calls: $0.158048.

| Run | Wall ms | duration_ms | duration_api_ms | Reported ratio | Hook marker |
| --- | ---: | ---: | ---: | ---: | --- |
| Controlled baseline | 4250 | 1530 | 1509 | 1.014 | Present |
| Controlled minimal | 2485 | 1449 | 2840 | 0.510 | Absent |
| Inherited settings plus minimal profile | 2630 | 1506 | 1488 | 1.012 | Absent |

All three emitted successful result events, exit 0, one turn. Controlled wall time fell by 1765ms (41.5%) and the hook did not execute. The two-second hook is outside this CLI version's reported duration_ms; duration_api_ms can exceed duration_ms. Therefore the requested duration ratio is recorded but cannot reliably measure hook overhead on this installation. The marker plus wall time establishes the causal behavior.

A separate inherited-settings baseline before implementation took 3264ms wall, duration_ms 2173, duration_api_ms 2968, success. It did not reproduce the ticket's historical 125-second overhead. Do not interpret these short runs as proof of a 125-second saving or guaranteed production latency. MCP services retained for build workers may still add teardown time.

Committed timing evidence: .xnaut/measurements/XNAUT-107/results.json. Local full logs: .xnaut/measurements/XNAUT-107/rust-suite.log and ui-suite.log. The execution logs are ignored; totals are transcribed above.

## Manual verification

1. Run `python3 scripts/measure-headless.py` from this worktree with Claude authenticated. This performs three small model calls with the limits above. Verify baseline hook_fired=true and controlled_minimal hook_fired=false, successful results, and compare wall_ms. No user settings are changed.
2. Run the persona test above, or open the worktree frontend and start a NautFlow draft/review. Inspect its captured loom_run command: --settings must contain the conditional per-invocation profile and --strict-mcp-config must remain. The prompt/model/resume arguments stay intact.
3. Exercise the generated command fixtures with `XNAUT_TEST_PORT=4291 npx playwright test tests/headless-profile.spec.mjs`. Managed-context cases must receive `{}` instead of disabling hooks; NautLoom must receive valid JSON after the second shell pass.
4. In a development app built from this branch, run a Designer turn or a build plan. Check a result appears, expected artifact/plan is produced, and inherited startup hook output is absent for ordinary unattended runs. Interactive Claude sessions must retain their normal hooks. This live installed-app smoke is for independent review; this run did not replace/restart the owner's app.

CLI setting reference: https://code.claude.com/docs/en/hooks (disableAllHooks and managed policy precedence). No mechanism was ported from another project.

## Review and limits

Implementation-only plan in .xnaut/plans/XNAUT-107.md approved by both independent reviewers, request in-c55df411-9a39-483b-9847-8c0e9ce68148. Earlier requests escalated completion-bookkeeping authorization separately; they did not reject the final implementation design.

Not reproduced: the historical 125-second incident and its original hook/plugin environment. Managed sessions intentionally keep hooks; this change does not promise zero overhead for them. No installed-app deployment, GitVM desktop launch or end-to-end production artifact build was performed. The real Claude subprocess and actual UI command-generation surface were exercised, with nested shell execution checked in fixtures.
