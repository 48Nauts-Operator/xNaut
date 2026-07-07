# AgentFather Agent System Design

Date: 2026-07-07
Status: Draft approved for planning
Scope: NAUT-Flow agent profiles, AgentFather, Agent Library, skills, access, tools, and guardrails

## Purpose

xNAUT needs a modular agent system that lets the user create, store, tune, select, and test agents for NAUT-Flow. The system should behave like a "BotFather for xNAUT agents": AgentFather helps create and maintain agents, while every generated profile remains editable and inspectable.

This is not a replacement for the existing CLI agent launcher. Existing CLI runtimes such as Codex, Claude Code, Pi, Gemini, OpenCode, and shell remain execution tools. The new system defines role agents that can use those tools when their profile allows it.

## Existing Context

xNAUT already has:

- A user-editable CLI runtime registry at `~/.config/xnaut/agents.toml`.
- Default CLI runtimes: `claude`, `codex`, `gemini`, `grok`, `opencode`, and `pi`.
- Terminal launch plumbing with multiple prompt injection modes.
- Agent session status tracking and a local hook server.
- Vault documents and Librarian tooling.
- Task and project scaffolding that can choose an `agent_id`.

What is missing:

- Reusable role agent profiles.
- Agent personas, roles, skills, access policies, tools, and constraints.
- AgentFather setup flow.
- Agent Library UI.
- On-the-fly creation from Vault, Librarian, or NAUT-Flow.
- Stage-aware NAUT-Flow routing.

## Core Model

An agent profile is a role contract:

```text
Agent = Persona + Role + Skills + Access + Tools + Constraints
```

Definitions:

- Persona: who the agent is, tone, perspective, and domain expertise.
- Role: the responsibility the agent owns in NAUT-Flow.
- Skills: reusable workflows the agent can apply, such as create PRD, review architecture, write threat model, or generate sprint stories.
- Access: what the agent may read, write, or is denied.
- Tools: executable xNAUT capabilities the agent may request.
- Constraints: hard behavioral boundaries.

The existing CLI runtimes are not agents in this model. They are tools or runtime implementations behind tools. For example, `launch_terminal_agent` may offer Codex, Claude Code, Pi, Gemini, OpenCode, or shell as runtime choices.

## Built-In Profiles

The first version should ship editable built-in profiles:

| Agent | Responsibility |
|---|---|
| AgentFather | Create, edit, clone, test, and validate agent profiles. |
| Librarian | Organize Vault knowledge, link notes, retrieve context, and maintain documentation health. |
| Analyst | Turn rough ideas into concepts and clarify the problem. |
| PM | Create PRDs, scope, success criteria, business value, and stakeholder framing. |
| Architect | Create architecture, data models, technical constraints, and implementation readiness checks. |
| Security | Review trust boundaries, secrets, data handling, and abuse cases. |
| Planner | Create development plans, sprint plans, epics, stories, and acceptance criteria. |
| Builder | Execute approved handoffs using terminal/coding tools. |
| Reviewer | Review implementation, tests, acceptance criteria, documentation, and release readiness. |

Custom profiles can be created from scratch or cloned from existing profiles.

## Profile Storage

Profiles should be human-readable and dogfooded through the Vault model.

Recommended storage:

```text
System/Agents/
  AgentFather.md
  Librarian.md
  Analyst.md
  PM.md
  Architect.md
  Security.md
  Planner.md
  Builder.md
  Reviewer.md
  Custom/
    Compliance-Reviewer.md
    SAP-Migration-Architect.md
```

Each profile uses YAML frontmatter plus Markdown instructions.

Example:

```yaml
---
xnaut_agent: true
id: architect
name: Architect
status: enabled
version: 1
role: architecture
skills:
  - create-architecture
  - create-data-model
  - review-technical-risk
access:
  read:
    - vault
    - project_docs
    - repo_structure
  write:
    - vault
  denied:
    - source_code
    - settings
    - secrets
tools:
  - read_vault
  - create_note
  - update_note
  - ask_llm
  - generate_mermaid
  - search_repo
constraints:
  - Do not edit implementation code.
  - Do not launch terminal coding agents.
  - Mark assumptions explicitly.
outputs:
  - architecture
  - data-model
  - readiness-check
---
```

The Markdown body should contain:

```markdown
# Persona

# Operating Rules

# Output Style

# Test Cases
```

## Skills, Access, And Tools

Skills describe methods. Tools execute capabilities.

Example skills:

- create-concept
- create-prd
- create-architecture
- create-data-model
- write-threat-model
- create-development-plan
- generate-sprint-stories
- execute-handoff
- review-implementation
- prepare-release-notes

Example tools:

- read_vault
- create_note
- update_note
- search_vault
- search_repo
- ask_llm
- generate_mermaid
- create_handoff
- launch_terminal_agent
- run_tests
- update_task_status

Access policies decide what data surfaces tools may touch.

## Guardrails

New agents must start conservative. AgentFather can recommend access, but the user approves the final profile.

Access presets:

| Preset | Meaning |
|---|---|
| Read Only | Can inspect Vault and project docs, cannot write. |
| Can Draft Docs | Can create draft notes, cannot overwrite existing docs. |
| Can Update Docs | Can create and update Vault docs. |
| Can Plan Execution | Can create handoff files and task plans. |
| Can Launch Agents | Can start terminal/coding agents, with confirmation. |
| Full Project Access | Can use project tools, including repo/source-related tools, with confirmations for risky actions. |

Risky actions remain visible and require confirmation:

- launch terminal agent
- edit source files
- run shell commands
- modify settings
- delete or rename documents
- access secrets
- push, release, or package

If the user grants Full Project Access, AgentFather must summarize the implications before saving.

## AgentFather UX

AgentFather should feel like Telegram BotFather adapted to xNAUT.

Supported commands:

```text
/newagent
/cloneagent
/editagent
/testagent
/enableagent
/disableagent
/listagents
```

The UI should also support natural language:

```text
Create a new compliance reviewer for this document.
Clone Architect and tune it for SAP migration projects.
Test Security against the current architecture note.
```

AgentFather setup flow:

1. Identify the agent purpose.
2. Suggest role and persona.
3. Suggest skills.
4. Suggest access preset and explicit read/write/deny rules.
5. Suggest tools.
6. Ask for hard constraints.
7. Generate a profile.
8. Show editable preview.
9. Save only after user approval.

## Agents Pane

The `Agents` entry should live in the top-right three-dot menu.

Pane layout:

```text
Agents
+- AgentFather chat / setup assistant
+- Agent library list
|  +- Built-in
|  +- Custom
+- Profile editor
   +- Persona
   +- Skills
   +- Access
   +- Tools
   +- Constraints
   +- Test
```

Primary actions:

- New Agent
- Clone Agent
- Edit Agent
- Test Agent
- Enable or Disable
- Save

Import and export can wait until a later version.

## On-The-Fly Creation

Agents can be created from the Agents pane, but also from active work.

Examples:

- In Vault: "This needs a compliance reviewer."
- In Librarian: "Create a Security agent for this feature."
- In NAUT-Flow: "No Security agent exists. Create one?"
- In Builder launch: "Create a specialized React Native builder."

The app should route these requests to AgentFather and pass context:

- current Vault note
- current NAUT-Flow stage
- requested responsibility
- suggested skills, tools, and access
- source document link

After saving, the new agent appears immediately in:

- Agents pane
- NAUT-Flow role selector
- Librarian routing suggestions
- Promote-to-next-stage choices

## Testing Profiles

AgentFather should be able to test a profile against a selected Vault note.

The test result should show:

- what the agent would read
- what the agent would write
- which skills it would use
- which tools it would request
- which actions are blocked by access policy
- the draft output it would produce

Testing should be non-destructive by default.

## First Version Scope

The first implementation should include:

- Agent profile schema.
- Built-in editable profiles.
- Agents menu entry.
- Agents pane with library list and editor.
- AgentFather create, clone, edit, test, enable, disable flows.
- Profile storage in Vault Markdown files.
- Basic skill/tool/access catalogs.
- Conservative guardrail enforcement at the UI/action routing layer.
- On-the-fly creation from Vault/Librarian/NAUT-Flow context.

Out of scope for the first version:

- Fully autonomous orchestration.
- Multi-agent scheduling.
- Automatic source-code edits by non-builder agents.
- Import/export marketplace.
- Project-specific override files, unless needed by implementation details.

## Open Implementation Decisions

These should be resolved during implementation planning:

1. Whether profile files live in the work Vault only or in both Vault and app config cache.
2. Whether built-in profiles are seeded every install or versioned with migrations.
3. Whether the first profile parser should support only YAML frontmatter or also TOML.
4. How strict action enforcement should be in Rust versus JavaScript for v1.
5. How AgentFather talks to the same LLM path as Librarian without mixing their chat histories.

## Self-Review

- No placeholder requirements remain.
- The design separates role agents from CLI runtimes.
- Skills, access, and tools are explicit and independently configurable.
- Guardrails support conservative defaults and deliberate escalation.
- The scope is focused on profile management and setup, not autonomous orchestration.
