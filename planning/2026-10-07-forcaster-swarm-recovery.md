# Forcaster swarm dispatch diagnosis and correction

Investigation by Codex on 7 October 2026 against the installed xNAUT 1.30.2 and release commit 8e9274d. The refreshed plan is approved and queued; the reported “0 of 0” does not establish that it was consumed.

## Observed state

- Live `instance_stamp` returns version 1.30.2 and role `workstation`. The saved legacy `dispatch_here` setting is false.
- Codex, Claude, Grok, Pi and Rudi profiles all specify local execution.
- Three FORCASTER groups remain approved with five queued members each. The latest is `swarm-dfd6d80cfe09496f9a31a7d49c835c75`.
- The latest group contains the corrected repository URL and local destinations. Earlier groups retain an empty repository URL.
- Ten recorded FORCASTER attempts are failed admission records: five missing-repository refusals and five changed-approval refusals. None supplies evidence that a worker started.

Evidence: the local native instance response, settings and agent profile stores, `~/.config/xnaut/registry/swarm-plans`, and the matching run manifests. The original records were inspected without changing approval state, profile destinations or the instance role.

## Causes

1. `swarm_plan` exposed no environment argument, unlike `dispatch_ticket`. Planning always resolved the profile setting, so instructions to use exe.dev could not override locally pinned profiles through this tool.
2. `refill` returns successfully without advancing groups on a non-Fleet instance. Confirmation persisted approval before calling it. On this Workstation that produced five queued members with no dispatch attempt and no explanation of the actual role hold.
3. The model-facing `swarm_dispatch` response discarded `queued` and calculated the denominator as started plus failed. Five queued members therefore became “0 of 0”. Its tool description also incorrectly said plans were consumed, although the durable implementation supports repeated confirmation and refill.

The repository update correctly invalidated approvals tied to the old repository setting. That safety check should remain. The refreshed group does not need replacement merely because no worker started; a new exe.dev scope would, however, require a new explicit destination and approval.

## Patch

Branch: `fix/forcaster-swarm-recovery`.

- Add an explicit swarm environment argument and persist it separately from the resolved destination. Use that choice during pin validation and native dispatch. Reject unknown or unconfigured destinations without falling back to local. Existing plans without an override continue following their profile pin.
- Show each destination and any instance-role hold on the approval card.
- Refuse dispatch on Workstation or Sandbox roles before adding a new approval, with a concrete explanation. Preserve the existing Fleet restriction.
- Return queued members, total planned membership and per-member reasons to NautBot. Remove the incorrect consumed-plan contract.
- Preserve the resolved destination when a proposed swarm contains only one runnable ticket.

## Validation

- 22 `swarm_plan::tests` passed, including persistence, explicit remote selection over a local profile, invalid/unconfigured destination refusal, five-member queued reporting, retained approvals, legacy deserialisation and role refusal.
- 1 tool-schema and durable-queue contract test passed.
- 8 `dispatch::tests` passed.
- 4 Playwright swarm tests passed, including visible destinations, role hold and approval behaviour.
- JavaScript syntax, ESLint for the changed frontend file and `git diff --check` passed.

Rust compilation emitted eight warnings in unchanged files. No live exe.dev worker was launched and no remote clone or model execution was smoke-tested.

## Operational status

This is a tested source patch, not an installed release. The running application remains 1.30.2. Existing groups, assignments, evidence, profile settings and the Workstation role remain intact.

Recovery must respect the owner's exe.dev destination. After installing a reviewed build, create an explicitly exe.dev plan on an authorised dispatch-capable instance with the project and approval state available. Check provider readiness and repository access from that destination before claiming execution. Do not enable Fleet on the owner's workstation as an implicit repair, delete failed evidence, or report an approved queue as running work.
