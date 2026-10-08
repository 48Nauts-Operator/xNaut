# Open-source agent crash recovery

Owner decision, 2026-10-08: the Registry is for paying users. Use Pi's durability
approach for the open-source version. This implementation provides local native
Agent Space recovery without a Registry subscription or external service.

## Design and attribution

Inspired by Pi Durable (MIT), especially durable submissions, persisted task
intent/results and explicit safe replay:
https://github.com/earendil-works/pi/tree/main/packages/durable
https://earendil.com/posts/pi-durable/
Video discussion: https://www.youtube.com/watch?v=ja_7AF54OtE&t=1236s

This is a Rust implementation around xNAUT's existing model/tool loop. It does
not install the experimental JavaScript framework. Native worker reservations,
PM mutation receipts and dispatch approval checks remain the authority for their
effects. An interrupted write must be reconciled through those receipts or its
external system; a model-generated call ID alone cannot prove whether it ran.

## Runtime contract

- A UUID identifies the submission before native invocation. The saved browser
  outbox closes the gap between saving the owner request and admitting its job.
- Native admission persists exact input and conversation identity in the local
  conversation SQLite database. Reusing an ID with different input is rejected.
- A native task owns execution, independently of the invoking webview. A portable
  OS file lock serializes each conversation across app instances; SQLite owns
  committed state. The lock is released automatically if its process dies.
- Startup and periodic recovery find unfinished turns. Reopening xNAUT resumes
  them. A closed desktop app does not execute work while it is off.
- Provider/endpoint/model, repository roots, capabilities and tool schemas are
  bound before effects. A changed binding stops recovery with an explicit error.
  Credentials are read from the existing settings, not copied into checkpoints.
- Prepared model context and tool selection are retained. Each model response,
  including native Responses continuation state, and each tool result is committed
  before the next operation. Reconstructing the bounded loop reads those records;
  completed effects are not repeated. Model request digests avoid saving a full
  duplicate transcript for every round.
- Tool intent is committed before execution. Only explicitly audited local read
  and catalog tools may repeat after an interrupted call. Other calls produce a
  retained interrupted result for reconciliation. This does not promise exactly-once
  behavior for arbitrary external systems or instruct the model to repeat writes.
- Streamed text is batched and committed before display. A cut-off model request
  can be sent again; this does not resume the provider's in-memory generation.
- Completed replies, swarm cards and worker receipts return to the original thread.
  A reply is acknowledged only after conversation persistence succeeds. Recovery
  does not approve a swarm or redirect the currently selected project.
- Execution records carry a format version. Future incompatible loop/storage
  changes require a migration before old pending turns may execute.

## Scope

The first complete path is native Agent Space, including NautBot and all profiles
that use `agent_chat_turn`. This is not a replacement for external Codex/Claude/Pi
CLI recovery, nor does it add durability to legacy frontend JSON action chains,
shared chat submissions or every automation. Pi-style document forks, compaction,
distributed failover and a general child-task scheduler are separate work.

## Acceptance evidence

`durable_turn::tests` starts isolated test processes and kills them before intent,
after intent, after the external effect and after result commit. The external
effect is an actual append plus fsync outside SQLite. Tests verify OS lock release,
unchanged operation identity, retained results, safe read/model replay and refusal
to repeat uncertain writes. Further cases cover scope drift and result-write failure.

`agent_tools::tests::durable_model_tool_loop_resumes_with_committed_results_and_no_repeated_read`
exercises both Chat Completions and native Responses over real local HTTP. It
interrupts execution after a tool result, changes the file that was read, reopens
the checkpoint and verifies the original result and provider continuation are
used. A fully committed loop also succeeds with the model server closed.

`tests/durable-agent-turns.spec.mjs` verifies outbox retry, persistence-before-submit,
result delivery to the original thread/project, restored worker links and swarm
cards, deduplication and no acknowledgement when conversation storage fails.

These cases are mandatory in `scripts/release-behavior.mjs`. Fixtures use temporary
storage and local services. They do not kill an installed xNAUT or launch paid
workers. No product release is performed by this feature branch.
