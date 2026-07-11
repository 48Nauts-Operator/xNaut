# xNAUT 1.9.0

xNAUT 1.9.0 adds the project and Agent foundations needed to move from documentation into controlled delivery while keeping each subsystem optional.

## Highlights

- Agent Library and AgentFather for reusable personas, roles, skills, tools, access scopes, constraints, outputs, and runtime model assignments.
- Provider-aware Agent Chat with LM Studio, Ollama, OpenAI, and OpenRouter model discovery and durable per-user model selection.
- Git-backed Project Management with Forgejo-first setup, optional GitHub support, project migration, tickets, workflow events, revision checks, and synchronization.
- NautFlow project workspaces with overview, commercial baselines, lifecycle stages, versioned Markdown artifacts, preview/edit controls, Agent collaboration, review, and stage promotion.
- Live Agent access to the active NautFlow artifact, related Vault documents, and constrained active-document updates.
- In-app Forge issue and PR review with Agent-assisted RCA analysis.
- Local Excalidraw MCP installation and lifecycle management without requiring Excalidraw+.

## Reliability Improvements

- Full-document Vault actions receive a larger completion budget instead of being truncated into invalid JSON.
- Agent model selections are stored in the authoritative xNAUT settings file.
- Provider and model are routed together, preventing cloud model IDs from being sent accidentally to LM Studio.
- Project and Vault paths are normalized before Agent tool execution.
- Project artifact writes update the visible editor immediately.

## Upgrade Notes

- Project Management remains disabled by default. Enable it under **Settings -> Tasks Mode -> Modules** and connect or create a dedicated control repository.
- The local Excalidraw MCP requires `git`, Node.js, and `pnpm` on first installation. It binds only to `127.0.0.1`.
- Existing Vault data, Agent profiles, project records, and conversations remain in their current locations.

## Scope Boundary

This release provides the Project, NautFlow, ticket, Agent, and artifact foundation. OpenSpec-style Change records, canonical project baseline reconciliation, and GitVM sandbox orchestration remain planned under ticket `XNAUT-5` and are not included in 1.9.0.

## Verification

- Rust unit tests: 172 passed.
- Full xNAUT preflight: passed.
- Release targets: macOS Apple Silicon, macOS Intel, and Windows x64 through the public GitHub release workflow.
