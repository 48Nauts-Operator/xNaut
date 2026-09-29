# Jev-assisted compute routing — proposal

Owner suggestion, 2026-09-29: use the request and available resources to select local, exe.dev or GitVM. This is a proposed follow-up, not implemented or benchmarked in the current preview.

## Placement

Keep the existing execution drivers. Add a compute-routing service before `dispatch::pm_ticket_dispatch` launches a worker, backed by the existing `sandbox::launch_env` vocabulary. It returns a proposed destination and typed evidence, never executes a command itself. After selection, the existing dispatcher remains responsible for policy, admission, worktree isolation, authentication, launch and audit.

The current resolver's Automatic setting chooses from configured providers; it does not reason about the task or probe live readiness. Introduce a separate opt-in Smart routing setting, preserving explicit Local/exe.dev/GitVM and legacy Automatic behavior until evaluated.

## Decision order

1. **Owner instruction.** An explicit destination takes precedence. Validate it and either use it or report the concrete blocker. Do not quietly override it.
2. **Code-enforced eligibility.** Check allowed data location, required OS/architecture, repository access, required tools/services, valid credentials, writable independent worktree, disk/memory headroom, active-run collisions, concurrency limits, allowed cost and endpoint reachability. Configuration alone is not proof of readiness. Unknown facts remain unknown.
3. **Measured state.** Gather time-stamped resource/capability snapshots in parallel with bounded timeouts. Track queue depth, warm build caches, availability, expected setup latency and configured vendor prices. Never infer isolation, free idle billing or installed software just from a vendor name. Our current exe.dev route reuses an agent/project directory, so overlapping runs must not rsync over each other.
4. **Skip unnecessary AI.** Zero eligible destinations: queue or report the blocker. Exactly one eligible destination: select it deterministically. An obvious fully specified task can also use rules directly.
5. **Jev recommendation.** For ambiguous tasks with several eligible destinations, send a minimized task description and measured snapshot. Use a Choice over eligible destinations plus `defer`, with criteria separating compatibility, isolation, setup time, cache reuse and cost. Batch independent questions about workload characteristics in one request; do not serialize several judges. Dependent recommendations can use one Choice with the complete state rather than pretending parallel answers can consume each other.
6. **Validate and reserve.** Validate the schema and returned option; enforce constraints again, refresh stale checks and atomically reserve capacity before launching. A high confidence value cannot override an authentication, privacy, budget or platform failure.
7. **Fallback without routine owner interruption.** On timeout, malformed response or weak preference among equally eligible options, use the configured deterministic policy or queue with a clear explanation. Ask only when execution needs a new owner decision, such as allowing external transfer or changing a cost ceiling. No automatic loop of paid retries.

Example outcomes, conditional on measured readiness:

| Request | Candidate route | Reason |
| --- | --- | --- |
| Test the native macOS microphone interaction | Local Mac | Needs its OS, audio device and app UI |
| Rebuild a large Linux service with an existing warm cache | exe.dev | Persistent cache may shorten setup if resources are available |
| Verify an independent Linux ticket from a clean checkout | GitVM | Fresh isolated environment if provisioned and dependencies available |
| Keep this private repository on this Mac | Local | Privacy policy excludes remote execution |
| Local disk is full; both remote targets lack credentials | Defer | No eligible route; a model cannot fix missing capability |

No fixed promise that any vendor is cheapest or fastest. Estimate the full marginal cost and completion time, including provisioning, transfer, cache misses, queueing and teardown.

## UI and evidence

Show in the dispatch card and Observatory:

> GitVM selected · fresh Linux verification · local disk below launch floor · authentication checked · automatic policy

Generate that sentence from verified checks and bounded typed reason codes. Jev returns judgments, not a trustworthy prose explanation. Keep a receipt containing task ID, candidate eligibility/exclusions, snapshot time, decision mode, selected route, policy/model version, confidence, fallback reason, estimated cost, actual launch environment and eventual result. Never put tokens or private credentials into it.

A cloud Jev call is itself a transfer. Local execution does not imply private routing. For local-only projects, use deterministic rules or an approved sanitized descriptor; do not submit repository content or full private tickets. Route through the approved TypeSafe connector/NautGate integration only after verifying its supported API, rather than treating the typed API as a generic chat endpoint.

## Evaluation before automatic selection

Begin in shadow mode: current rules select; Jev recommends and logs a comparison. Use held-out historical ticket cases and measured outcomes to compare against a rule-based baseline. Include unknown requirements, offline hosts, revoked auth, stale resource data, full disk, incompatible OS, concurrent capacity races, prompt injection in ticket text, explicit owner overrides, all destinations excluded and model outages.

Measure invalid-route rate, launch success, completion time, queue/setup delay, compute spend, model overhead and interventions. Report confidence as model preference certainty, not as a probability the coding task will succeed. Choose thresholds using this task set; do not copy example numbers from docs. Low confidence can mean equally good routes and should not automatically bother the owner. Enable unattended Smart routing only if it adds value over rules while keeping hard-constraint violations at zero in the evaluation set.

Sources read:
- [TypeSafe Choice](https://docs.typesafe.ai/primitives/choice): fixed options, distribution and confidence; independent questions may be batched.
- [Intent routing](https://docs.typesafe.ai/patterns/intent-routing).
- [Confidence](https://docs.typesafe.ai/confidence): derived from answer distribution; thresholds need domain evaluation.
- [Confidence-based routing](https://docs.typesafe.ai/patterns/confidence-routing).
