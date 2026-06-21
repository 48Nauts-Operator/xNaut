---
name: xnaut-review
description: Leave inline review notes on a worktree's diff inside a running xNaut window. Steers the user's diff viewer; never opens its own.
---

xNaut is a Tauri-native terminal + worktree review app. When the user opens a diff pane (the side-by-side / unified view with a worktree path), this skill lets an AI agent leave inline annotations that float beside the changed lines.

**The diff viewer is for the user — do NOT run your own viewer.** Use the local-loopback HTTP broker at `http://127.0.0.1:<port>/v1/notes` to inspect and control the live session. The port is the same one xNaut prints on startup as `Agent hook listener at http://127.0.0.1:<port>/v1/hook` — replace `/v1/hook` with `/v1/notes`.

## Action vocabulary

Every request is a `POST` with `Content-Type: application/json`. The `action` field discriminates:

```bash
curl -X POST http://127.0.0.1:<port>/v1/notes \
  -H 'content-type: application/json' \
  --data '{ "action": "<verb>", ...payload }'
```

### Read

- `{ "action": "list" }` — placeholder; xNaut doesn't track all known worktrees yet.
- `{ "action": "get", "worktree": "<abs path>" }` — full notes document.
- `{ "action": "review", "worktree": "<abs path>", "includePatch": true, "includeNotes": true }` — diff + existing notes together. **Call this first** before authoring any new comment so you know the line numbers and structure.

### Annotate

- `{ "action": "comment-add", "worktree": "<abs>", "filePath": "src/foo.rs", "side": "new", "line": 42, "summary": "<headline>", "rationale": "<why, optional>", "author": "claude", "reveal": true }` — single inline note.
- `{ "action": "comment-apply", "worktree": "<abs>", "comments": [{ ... }, { ... }], "revealMode": "first" }` — batch. Validates the whole list before mutating, so a single bad item doesn't half-apply.
- `{ "action": "comment-rm", "worktree": "<abs>", "commentId": "<uuid>" }` — remove a single annotation.
- `{ "action": "comment-clear", "worktree": "<abs>", "filePath": null, "includeUser": false }` — clear AI/agent notes. With `includeUser: true`, also drops the user's own annotations.
- `{ "action": "comment-list", "worktree": "<abs>", "filePath": null }` — list current notes.

## Required fields

- `comment-add`: `worktree`, `filePath`, `side` (`old` or `new`), `line`, `summary`. Everything else is optional.
- `comment-apply`: each item in `comments` needs `filePath`, `side`, `line`, `summary`.

## Soft rules — read these before authoring

1. **Don't comment on every hunk.** Highlight what the user wouldn't spot themselves — subtle logic errors, missed edge cases, accidental scope expansion. Skip "renamed variable" or "removed unused import" unless something about them is wrong.
2. **Navigate before commenting.** Use `review` to see the actual line numbers and content. Inline-note line numbers are 1-indexed on the side you target (`old` for deletions, `new` for added or modified lines).
3. **Use `rationale` for the "why".** The `summary` is the headline (≤ 60 chars usually reads well); `rationale` is where the reasoning goes. Both render — summary bold, rationale muted underneath.
4. **Set `reveal: true` on a single add** (or `revealMode: "first"` on a batch) so the user's diff pane scrolls to your note instead of the user having to hunt for it.
5. **Range vs single-line.** The note model accepts `[start, end]` ranges via `oldRange` / `newRange`, but the broker's `comment-add` collapses to a single anchor line. If you want range coverage, use the lower-level `notes_write` Tauri command or write the JSON directly to `<worktree>/.xnaut/notes.json` — agents in sandboxed environments often need this path anyway because loopback HTTP is blocked.

## Storage

Notes live at `<worktree>/.xnaut/notes.json` in this shape:

```json
{
  "version": 1,
  "summary": "Optional changeset-level note",
  "files": [
    {
      "path": "src/foo.rs",
      "summary": "What changed in this file",
      "annotations": [
        {
          "id": "uuid",
          "oldRange": [10, 14],
          "newRange": [10, 16],
          "summary": "Tighten this wording",
          "rationale": "We say 'must' twice in a row.",
          "tags": ["clarity"],
          "confidence": "high",
          "source": "agent",
          "author": "claude",
          "createdAt": "2026-05-28T12:00:00Z"
        }
      ]
    }
  ]
}
```

xNaut watches this file. Edit it directly and the diff pane re-renders within ~100ms — no broker round-trip needed. This is the fallback path for sandboxed environments where the loopback HTTP broker isn't reachable.

## Common errors

- `400 Bad Request` — payload didn't deserialize. Most common: missing `summary` or `side: "left"` instead of `"old"`.
- `500 Internal Server Error` with body `read <worktree>/.xnaut/notes.json: ...` — the worktree path doesn't exist or you don't have write permission to it.
- Empty `list` response with xNaut visibly running — the listener isn't bound yet, or you're hitting `127.0.0.1` while the loopback is sandboxed. Falling back to writing the JSON file directly always works.

## The killer pattern

Open one xNaut tab with the terminal you spawn the agent in. Split → Diff (Cmd+Alt+D when wired, or the diff icon in the top bar). Now the agent works in the terminal *and* leaves notes in the diff pane next to it. The user sees both at once.
