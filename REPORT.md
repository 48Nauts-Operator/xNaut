# XNAUT-193: every MCP tool answers in the same shape

## What changed

One response contract for every `xnaut_*` MCP tool, applied once at the place a
tool result becomes an MCP reply.

`src-tauri/src/agent_hooks.rs`

- `tool_call_result(name, Result<Value, String>)` is now the only path from a
  tool answer to a `tools/call` reply. `handle_mcp` used to build the reply
  inline in a two-arm match; that match is gone and the call site is one line.
- The text of the reply is the envelope:

      status        "success" | "warning" | "error"
      summary       one line, naming the tool and what it did
      next_actions  what the agent can usefully call now
      artifacts     the ids and paths from the payload, capped at 20
      data          the payload that used to be the whole answer, unchanged

- `tool_next_actions` is a static per-tool table. It is the only per-tool part
  of the contract, because the useful next move is a property of the tool, not
  of the row it returned. A tool missing from the table produces empty
  `next_actions`, which is a test failure rather than a silent gap.
- `warning` means an empty answer. A list or search that matched nothing is the
  case where an agent otherwise reads the tool as broken and retries the same
  call; the first next action tells it to widen the query instead.
- Errors keep `isError: true` on the reply, so a client that branches on it
  before reading the text is unaffected. Three error shapes get specific
  recovery steps: a document conflict (re-read, then retry with
  `current_sha256`), a missing required argument (name the argument), and
  everything else (fix and retry once, then stop).
- Credit is in the file header, in the form used by `gate_score.rs` and
  `veto.rs`: ECC (github.com/affaan-m/ecc, MIT),
  `skills/agent-harness-construction/SKILL.md`, Observation Design. Three
  departures are named there: one place rather than a rule per tool, the old
  payload preserved under `data`, and `warning` reserved for the empty answer.

`scripts/mutation-check.cjs`

- One entry: "XNAUT-193 an MCP tool answers outside the envelope", which drops
  the success arm back to the bare payload and expects
  `cargo test --bin xnaut agent_hooks::` to go red.

## What I proved

Four tests in `agent_hooks::tests`:

- `a_tool_answer_carries_status_summary_next_actions_and_artifacts`: the
  representative tool (`xnaut_read_document`), asserting all five keys, the
  exact summary, an artifact, and that `data` still equals the old payload.
- `every_tool_answers_in_the_envelope`: the guard the ticket asked for. It
  walks every name `project_mcp_tools()` advertises, puts each through
  `tool_call_result` on both a success and a failure, and fails if any of the
  five keys is missing, if the status is outside the three-value vocabulary, if
  the status disagrees with the outcome, or if `next_actions` is empty.
- `a_document_conflict_says_how_to_recover`: the one error an agent can act
  on keeps its structured payload and gains the two-step recovery.
- `an_empty_result_is_a_warning_not_a_success`.

Mutation, run by hand with the same `from`/`to` now recorded in
`scripts/mutation-check.cjs` (edit applied in the worktree, then restored from a
copy taken first):

    -        Ok(data) => success_envelope(name, data),
    +        Ok(data) => data,

    test result: FAILED. 9 passed; 3 failed
    xnaut_list_projects answered with a bare value, no status: {"id":"XNAUT-1","revision":3}

The guard test names the exact regression. Restored, and the tree is back to
`success_envelope(name, data)`.

Gates, all on this worktree:

- `cargo test --bin xnaut`: 500 passed, 0 failed, 20 ignored.
- `cargo test --bin xnaut audit::`: 3 passed, including
  `every_command_is_allowed_by_the_acl` and
  `every_command_the_frontend_calls_exists`.
- `cargo clippy --bin xnaut`: finished; 23 pre-existing warnings on the bin,
  none of them in `agent_hooks.rs`.
- `npx playwright test`: 91 passed, exit 0.

## What I did not do, and why

- I ran the mutation by hand rather than through `node scripts/mutation-check.cjs
  --all`. The harness has no per-case filter and `--all` runs every slow cargo
  case in the file, which is a very long serial run for one new entry. The entry
  is in the list for the next full run; its `from` string was checked to appear
  exactly once in the file, which is the failure mode the harness reports as
  NOT FOUND.
- `npm install` was needed first: this worktree had no `node_modules`, so the
  first `npx playwright test` died in config loading with `ERR_MODULE_NOT_FOUND`
  on `@playwright/test` before running anything. That is environment, not a test
  failure. The chromium browser was already present.
- Nothing else consumes these tools inside the repo. I grepped for every tool
  name across `.rs`, `.md`, `.toml` and the frontend; the only hits are
  `agent_hooks.rs` itself. The consumers are external MCP clients, and for them
  the payload still arrives intact under `data` with `isError` unchanged.
- The ticket was right about the code. The tools did each return their own ad
  hoc JSON, and there was one obvious choke point to fix it in.
