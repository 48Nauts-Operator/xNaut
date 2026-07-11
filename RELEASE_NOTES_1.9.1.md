# xNAUT 1.9.1

xNAUT 1.9.1 adds the first governed Agent Loop workspace and closes the connection between project documentation and external Agents.

## Highlights

- Build and validate visual Agent Loops with Rete, or ask LoopBuilder to create the draft conversationally.
- Require repository, branch, and ticket scope before a run is scheduled.
- Inspect durable run events in a live execution console, including explicit waiting-for-worker status.
- Deactivate an Agent Loop or use Emergency Stop to cancel an active run.
- Open project-scoped Vault documents from the new Docs tab without moving existing Markdown files.
- Switch from project documents to All Vault when broader access is needed.
- Remove the redundant left-side Vault entry when project Docs is available, while retaining it as a fallback for disabled or empty Project Management setups.
- Keep the Librarian and its conversation history in the shared right pane.
- Connect local Agents through the new Project Management MCP to list projects/tickets and create or revision-safe update tickets.

## MCP Connection

Open **Projects -> Settings -> Agent connection - MCP**. Copy the connection object and add it to any client that supports Streamable HTTP MCP with custom headers. The endpoint binds only to `127.0.0.1`, and its bearer token changes whenever xNAUT restarts.

## Important Runtime Boundary

The Agent Loop runtime persists scheduling, validation, node state, approvals, budgets, events, and cancellation. A run whose console says `WAITING` has no connected execution worker; xNAUT now reports that state explicitly instead of implying the node is executing.

## Verification

- Rust tests: 198 passed.
- Production clippy: passed with warnings denied.
- Frontend bundle and JavaScript syntax: passed.
- xNAUT fast preflight: passed with no failures.
