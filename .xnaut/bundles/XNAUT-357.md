# XNAUT-357 — The core team: Researcher, Reviewer, PoC, Judge

The standing method (read other people's solutions, reimplement, credit the
source) turned into a loop that runs on a clock instead of by accident.

XNAUT_TEST_TOTALS={"rust":[{"passed":1236,"failed":0,"ignored":45}],"ui":[225]}

## What changed

### New: `src-tauri/src/core_team.rs` (2462 lines, 36 tests)

The whole loop, with the four members resolved to agents that already exist:

- **Researcher** — `@researcher` (XNAUT-356), asked for GitHub candidates on a
  topic list and parsed into `Finding` records. `parse_findings` pulls a JSON
  array out of prose, a fence or a bare answer; `triage` dedups against the
  board AND against the batch, and names every skip.
- **Reviewer** — the profile whose role is `reviewer`, on a provider the
  Researcher did not use (asserted in `status_from`). Scores a five-dimension
  `Rubric` (fit 30, novelty 25, port size 15, licence 15, activity 15 → 0..100).
- **PoC** — an ordinary dispatch. A finding that clears the threshold moves to
  `ready` with an owner, and the sweep dispatches it onto `poc/<slug>` with a
  brief carrying the credit rule, the wall clock and the document template.
- **Judge** — `jury::Gate::Poc`: the existing two-blind-reviewer jury asked a
  pro/con question instead of a merge question.

Four decisions worth knowing, all argued in the module header:

1. `poc` is a TAG, not a global ticket status (a status would grow a column on
   every project's board).
2. `finding` IS a new ticket type — a candidate nobody agreed to build.
3. The licence is a GATE, not a scored dimension. Copyleft and unknown are
   refused equally hard at any score: a licence we cannot name is a credit we
   cannot write.
4. Nothing merges. An `in` writes `council.verdict` and opens the Plan Canvas.

### Changed

| File | What |
|---|---|
| `settings.rs` | `CoreTeamSettings` — off by default; threshold 70, beat 7 days, PoC 90 min, topics, boards. |
| `jury.rs` | `Gate::Poc` + its pro/con rubric. `Poc` settles; it never merges. |
| `project_management.rs` | `TICKET_TYPES` (one list, was four copies) now carries `finding`; `ticket_retag_in`. |
| `agent_hooks.rs` | The MCP tool schema reads the same `TICKET_TYPES`. |
| `dispatch.rs` | `branch_for_ticket` → `poc/<slug>` for a tagged finding; the PoC brief in the prompt. |
| `swarm_plan.rs` | Uses `branch_for_ticket`, so the card and the repo still agree. |
| `sweep.rs` | The weekly beat, the wall-clock stop, the judge pass; findings leave the verify lane. |
| `src/js/core-team-settings.js` | The one surface: switch, threshold, topics, roster, why it is idle, Scan now. |
| `app.js`, `index.html` | The Core Team settings section and its nav entry. |
| `project-management-panel.js` | `finding` in the board's type list. |

## The loop, end to end

```
sweep tick (3 min)
  └─ beat due? (durable, ~/.xnaut registry/core-team.json)
       └─ core_team_scan: @researcher → findings → dedup → CORE tickets
            └─ @reviewer weighs each ONE, at the moment it is filed
                 ├─ < threshold or bad licence → tag `shelved`, stays in inbox
                 └─ ≥ threshold → tag `poc`, status ready, owner assigned
                      └─ sweep dispatches → poc/<slug> worktree + brief
                           ├─ over 90 min → stopped, "too big to PoC" on ticket
                           └─ done/review → sweep judges (one per tick)
                                ├─ no credit / no measurement → RETURNED unjudged
                                ├─ jury says in  → council.verdict + Plan Canvas
                                ├─ jury says out → council.verdict + ticket done
                                └─ jury escalates → council.verdict, OPEN
```

## Verify by hand

```bash
cd /Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-357

# Both suites
cargo test --manifest-path src-tauri/Cargo.toml     # 1236 passed, 0 failed, 45 ignored
XNAUT_TEST_PORT=4291 npx playwright test            # 225 passed

# The new tests alone
cargo test --manifest-path src-tauri/Cargo.toml core_team          # 36
XNAUT_TEST_PORT=4291 npx playwright test core-team-settings        # 4

# Nothing regressed in the repo's own hygiene (2 failures + 80 unreachable
# commands, identical to the baseline before this branch)
node scripts/hygiene-check.mjs

# Clippy is clean on every file this branch touched
cargo clippy --manifest-path src-tauri/Cargo.toml --bin xnaut 2>&1 | grep core_team.rs
```

In the app: Settings → **Core Team**. It is off, and says so. It names who
would research and who would review, and lists everything in the way. **Scan
now** works with the switch off on purpose — that is how you try the loop
before agreeing to a weekly run.

## Deliberately not done

- **No GitHub API client.** The Researcher searches through its own search
  provider (Perplexity `sonar-pro`) and reports metadata it read. A GitHub
  token, rate limits and a second credential were not worth it for eight
  repositories a week, and the licence/file fields are refused when missing
  rather than guessed.
- **No GitVM sandbox for the PoC.** The dispatch note ("the PoC run goes
  through dispatch.rs like a swarm run") makes it a worktree run, so it inherits
  the run registry, the writer lease, the spend cap and the Observatory. The
  original body's "loom budget" does not apply: a dispatched agent run is not a
  `loops.rs` workflow run, so the budget is a wall clock the app enforces.
- **No manual Weigh/Judge buttons.** `core_team_review` and `core_team_judge`
  are not Tauri commands: the scan and the sweep are their only callers, and
  the Reviewer's cost bound *is* "once per finding, when it is filed".
- **No live end-to-end run.** Every layer is unit-tested against fixtures; the
  loop has not been run against the real Perplexity and Anthropic endpoints,
  because that files tickets on the real CORE board and spends money. Turning
  the switch on and pressing Scan now is the first real exercise.
