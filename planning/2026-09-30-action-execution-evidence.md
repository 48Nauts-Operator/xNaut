# Agent worktrees and execution evidence

The reported failure was real: Cortana attached `cx-JobUp` and updated JOBUP-11, then described a security audit as started. A terminal attachment is a viewport, not a command or worker launch. The registered JobUp source checkout exists. The chat tool layer could inspect explicitly supplied repositories but could neither resolve the project name nor create and launch its own worktree.

## Change

- Resolve project names/keys in **user messages** through the PM project registry. Accept only actual local Git roots; ambiguous display names, assistant-supplied names and unrelated paths do not grant access.
- Give every configured agent identity `create_worktree` and `start_repository_task`. The caller identity comes from the chat context, never a model-supplied handle. Cross-agent ticket dispatch remains NautBot's responsibility.
- Derive a stable, agent-specific workspace from the task key. Reuse the existing worktree builder and identity-aware launcher, including writer leases, profile policy, spend/admission checks, read-only switch, and configured local/exe.dev/GitVM execution.
- Validate any existing destination against the exact Git root, branch and common directory. Refuse symlink containers/destinations and scratch directories. Work starts from the selected repository's committed HEAD, so uncommitted owner files are not copied.
- Include the full requested task, user conversation and registered ticket scope in the worker prompt. JOBUP-11 currently asks for a **read-only** review; remediation is not implied by starting it.
- Reserve a launch on disk before starting it. The same agent/repository/task key returns its prior receipt instead of starting a duplicate. An uncertain pending launch after a crash is not blindly retried. A deliberately new task/run needs a new task key.
- Show the backend launch receipt in chat with its workspace and a Terminal button. Preserve it in follow-up context, explicitly as historical evidence, not current status or completion.
- Attachments, ticket bookkeeping and workspace creation do not prove an audit started. Preparation-only audit turns get one bounded recovery; unsupported start claims are replaced. Existing repositories no longer immediately fall through to the path-picker handshake.

A launch receipt proves that the launcher accepted and launched a worker. It does not prove a scanner ran, authentication succeeded, findings were remediated, or a security sign-off is justified. CLI/runtime failure after launch still needs the existing status/output/handback checks.

## Aikido: separate machine repair

Aikido was configured and enabled: `npx -y @aikidosec/mcp@1.0.17`. The npm launch cache had lost its root package.json; npx exited with ENOENT before MCP initialization. This was not evidence of an Aikido authentication failure.

The broken cache was moved intact to `~/xnaut-testing/recovery/2026-09-30-aikido-npx/3d810bd0aa093a23-broken`, then the pinned command recreated its cache. MCP initialize and tools/list succeeded and returned `aikido_issues_list`, `aikido_full_scan`, `aikido_login`, and `aikido_ignore_issue`. No authenticated scan or source upload was performed. MCP connections are opened per turn, so the existing owner app can retry this connector without restarting.

## Verification and limits

- Full Rust suite: 1,485 passed, 53 ignored. Covers project-name authorization, Git worktree identity, different agents using the same task key, launch reservations, preparation versus execution, and existing launch/lease policies.
- Chat/voice browser suite: 25 passed. New regression checks wrong-request isolation, duplicate receipt delivery, opening the actual session, and preserving its identity in the next chat turn.
- Final focused tool-loop suite after the known-repository path-picker recovery change: 21 passed, 11 ignored. The regression first returns BUILD-REQUEST for a known project, then verifies recovery reads the registered source without asking for its path.
- JavaScript syntax and git diff whitespace checks passed.
- No live JobUp security audit was started as part of testing. No claim of real scanner findings, completed remediation, remote-environment end-to-end acceptance, or final security sign-off.

Evidence directory: `~/xnaut-testing/runs/2026-09-30-action-execution-evidence/` (Rust/browser logs and Aikido initialization probe).

## Delivery / manual acceptance

Branch: `fix/action-execution-evidence`, based on `f6abbce` (native Astra Responses preview). This is a separate preview, not a public release or an installed-app replacement.

Once ready to switch apps, use the new Execution Preview and ask Cortana to run the read-only JOBUP-11 audit. Expected: resolve the registered JobUp checkout, create its own clean Git worktree, launch the configured runtime with the ticket scope, display the launch receipt and an operable Terminal button. Verify actual output before treating the audit as running or complete. Repeating the same task key must not create a second worker. A missing runtime/credential must be reported as the concrete failure, not replaced with a success claim.

The current owner app and its sessions must remain untouched during preparation. Rollback is returning to the prior Responses Preview; existing conversations/settings are shared. Do not erase prepared worktrees, receipts or the npm recovery cache to roll back.
