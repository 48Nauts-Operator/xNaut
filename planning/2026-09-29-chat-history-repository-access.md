# Chat history and local repository access — preview fix

Date: 2026-09-29. Branch `fix/chat-history-repo-access`, based on Jev preview `7b418e6`. Public release remains 1.28.2. NautGate owns the separate in-progress Astra transport fix.

## Reported failures and causes

The owner could not find earlier conversations in the new test app. They also asked Stark to review `/Users/cand0rian/DevHub_Studio/factory/05-DevOps/StarkControl`, received a build handoff, then repeated claims that the chat had no filesystem access.

The preview's new bundle identifier created a new WKWebView storage profile. Agent threads and ordinary chat histories lived only in that profile's localStorage. Older conversations remained in other xNaut profiles. Separately, saving an Agent Space thread retained at most 80 messages and 12 threads; ordinary chat retained 200 messages. The thread picker hid entries beyond its short display slice. There was also no ordinary chat history picker.

The local directory exists and is readable. Chat had native vault/canvas tools and permitted MCP tools, but no native repository reader. Paper grants do not grant repository access. This was a missing chat capability, not evidence of a macOS permission denial or a reason to change file permissions.

## Implemented

- Shared `~/Library/Application Support/xnaut/conversations.sqlite`, independent of bundle ID; protected file permissions, SQLite transactions and optimistic revisions. Two writers cannot silently overwrite one another.
- Read-only import from explicitly named prior xNaut WebKit profiles. Original databases are unchanged. Original per-source/key snapshots are retained in the native database. Ordinary-chat conflicts get separate recovery entries; agent threads are merged by ID, with newer metadata preferred and originals retained in snapshots.
- Startup waits for native hydration before opening chat/Agent Space and before the older Librarian migration. Existing local copies remain a cache. The local pending-write outbox retains interrupted/failed writes; recovery creates a separate conversation instead of overwriting newer text. Save failures are visible.
- Removed silent 80-message/12-thread and 200-message persistence truncation. Model request context remains bounded separately. This does not recover text already discarded from every historical copy, nor does it make model context unlimited.
- Ordinary chat's **History…** selector reopens stored conversations in a fresh tab. Expand an agent's arrow in Agent Space to find its older threads; entries beyond the former eight/five-item slices are reachable. Newly named ordinary chats also persist their titles; old chats without title metadata use their first user message as a label.
- Native `list_repository_files` and `read_repository_file` tools work in the shared chat tool loop, including Agent Space and voice backend turns. A user-supplied absolute Git repository root establishes read scope. Model/assistant/tool-supplied paths cannot grant scope. Quoted paths with spaces work.
- Directory/file pagination and bounded responses; reject traversal, symlinks, excluded private/generated files, binaries, non-UTF-8 and files over 1 MiB. No shell, writes, worktree creation or plugin installation is involved in these reads. The existing schema budget/discovery and Jev recommendation layer still apply.
- Tool instructions distinguish documentation review from building. The agent should use actual readers and report concrete tool errors rather than inventing a missing-permission diagnosis. A real build/test/command still uses the coding-runtime handoff.

Repository scope currently comes from user messages present in the current model turn. If a project path is no longer in the bounded context, supply it again; this change does not grant unrestricted permanent disk access. A working model tool route remains necessary. It does not fix the separate Astra/NautGate transport problem or perform a full StarkControl review.

## Verification

- Full Rust final run: **1,468 passed, 0 failed, 52 ignored**. The first run found a tool-description wording assertion; wording corrected before the passing run.
- Read-only owner-data probe, isolated temporary DB: **35 storage records, 82 agent threads, 686 retained messages** recovered. Actual repository reader read StarkControl `docs/updater.md` (**118 lines**). No existing WebKit database or real native conversation store was modified by this probe.
- Actual SSE model/tool fixture: a user-named repository was read, the tool result was returned with the correct call ID, and the model completed a second response. Unknown scope, traversal, private files and symlink escape refusals tested.
- Focused initial browser run: **24 passed** (history/model selection/Agent Space).
- Full browser run: **376 passed, 4 failed**. The history-startup change delayed the Librarian migration; fixed to run after hydration during startup. Three other failures were Monaco's 500 ms timing budget and two UI timeouts under concurrent build/test load. Their UI code was unchanged.
- Final focused rerun: **7 passed**, covering all four prior failures and the three new recovery/persistence/failure tests. Do not describe the initial full browser run as all green.
- JavaScript lint for changed modules and `git diff --check` passed. Frontend assets built. Final bundle/signature result is recorded in the Obsidian handover.

Evidence: `/Users/cand0rian/xnaut-testing/runs/2026-09-29-chat-history-repo-access/`. Earlier failed-run logs and browser failure contexts retained.

## Delivery and how to test

Preview: `/Users/cand0rian/xnaut-testing/builds/2026-09-29-chat-history-repo-access/xNAUT History Preview.app`, identifier `com.nautcode.xnaut.history-preview`.

The owner’s current Decisions Preview was left running. This preview is not automatically launched, installed or released. When ready, quit the old preview and open this one. First launch imports retained history into the shared store. Use Agent Space → agent arrow for older threads, or New Chat → History… for ordinary chats. Then ask Stark to list/read documentation in the explicit StarkControl path. Tool receipts should name the repository readers and show a real result. Test voice again after selecting a working tool-capable route.

No app/agent session was restarted; no repository permission bits were changed; no full StarkControl audit or security sign-off was performed. No paid inference was needed for these checks. The owner-data recovery count is evidence from an isolated probe, not a claim that the currently running old preview has already acquired the new history store.
