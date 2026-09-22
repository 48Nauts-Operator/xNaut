# XNAUT-341: a project overview that is either live or dated

Branch `agent/claude/xnaut-341`, 2026-09-22.

## What changed

The project workspace header already read four live numbers (XNAUT-342 landed
that, with a comment saying "XNAUT-341 does the fuller job"). What it did not
do was say anything when a number could not be read: all four fell back to a
bare dash.

A bare dash is three different situations wearing one face.

| The situation | What the header said | What it says now |
|---|---|---|
| A folder that is not a repository | a dash | `not a git repository` |
| A project with no checkout here | a dash | `no checkout on this machine` |
| `project_facts` threw | a dash | `the git facts could not be read` |
| `project_facts` resolved null | a dash | `the git facts could not be read` |
| A repository with no commits | a dash | `never`, plus `this repository has no commits yet` |
| git answered only in part | a dash | `git answered only in part` |
| The ticket store failed | a dash | `the ticket store could not be read` |

Two of those are fine and the rest are bugs, and a reader had no way to tell
them apart. That is the whole ticket: a number on a screen is read now, and a
number that could not be read says why.

### The reason is also more correct than it was

`is_repo` was `dir.join(".git").exists()`. That answers "no" for a linked
worktree pointed at by a `.git` FILE, and for a project rooted at a
subdirectory of a checkout, which has no `.git` entry at all. Both are inside
a repository. As a silent dash that was tolerable; as the printed sentence
"not a git repository" it is an actively wrong claim, so the probe became
`git rev-parse --is-inside-work-tree`.

## Files

- `src-tauri/src/repo_check.rs`: `ProjectFacts` gains `unavailable` and
  `no_commits`; `project_facts` reports why, and detects a repository through
  git rather than through a `.git` probe. New `facts_tests` module, 5 tests.
- `src/js/workspace.js`: `loadFacts` renders the reason: a chip beside the
  row and a `title` on each dash it explains. Reasons accumulate rather than
  overwrite. New `.wsp-why` style and a `[data-fact-why]` span in the header.
- `tests/project-workspace.spec.mjs`: 8 new Playwright cases against the
  stub, covering a git project, a plain folder, no checkout, an empty
  repository, a throwing command, a null-answering command, a failing ticket
  store, and two absences at once.

## The key code

`src-tauri/src/repo_check.rs`, the reason:

```rust
let (ok, out) = git(&["rev-parse", "--is-inside-work-tree"], Some(&path), short);
f.is_repo = ok && out.trim() == "true";
if !f.is_repo {
    // The stderr text is deliberately NOT quoted into the reason; `git()`
    // reports a failure to spawn as "timed out", so interpolating it would
    // print a confident wrong sentence on a machine without git.
    f.unavailable = Some(if ok || out.contains("not a git repository") {
        "not a git repository".into()
    } else {
        "git could not be read here".into()
    });
    return Ok(f);
}
```

`src/js/workspace.js`, where it lands. Reasons accumulate because two can be
true at once, and a chip that kept only the last one assigned would report the
second and quietly drop the first:

```js
const reasons = [];
const because = (keys, reason) => {
  keys.forEach((key) => {
    const el = pane.querySelector(`[data-fact="${key}"]`);
    if (el) el.title = reason;
  });
  if (!whyEl || reasons.includes(reason)) return;
  reasons.push(reason);
  whyEl.textContent = reasons.join('; ');
  whyEl.title = reasons.join('; ');
  whyEl.hidden = false;
};
```

## How to verify by hand

1. `cd src-tauri && cargo tauri dev`.
2. Open a project whose `source_path` is a real checkout. The header reads
   four numbers and shows no reason beside them.
3. Point a project's source path at a folder that is not a repository (any
   plain directory). The header keeps the ticket count, dashes the three git
   numbers, and reads `not a git repository` beside them. Hovering any dash
   shows the same sentence.
4. Open a project registered with an empty `source_path`. The reason reads
   `no checkout on this machine`, not `not a git repository`: nothing failed,
   the code simply is not here.
5. `git init` a fresh folder and point a project at it. Last commit reads
   `never` with `this repository has no commits yet`, and Uncommitted reads
   `0`, because in a repository that answered, zero is a fact.

## What is deliberately not done

- **The vault `Overview.md` generator.** The ticket's second half (a generated
  per-project Project Overview document at `work:<project>/Overview.md`) is
  out of scope for this run by the dispatch note of 2026-09-22: "Scope for
  this run: the workspace header... No Overview tab." The live half is what
  André wants in the next release; the dated half still needs its own run.
- **Deleting the old Projects-panel health card.** That is XNAUT-342's job,
  explicitly, once the header shows the same facts. It now does.
- **The duplicated `ago()`.** It exists verbatim in both `workspace.js` and
  `project-management-panel.js`. Merging them means touching the card that
  XNAUT-342 is about to delete, so it is left alone on purpose.

## Totals

Both suites run on this branch, on tron.candoo, 2026-09-22.

- `cargo test --manifest-path src-tauri/Cargo.toml`: 1255 passed, 0 failed,
  45 ignored. Baseline before this work was 1250 passed, 0 failed, 45 ignored.
- `XNAUT_TEST_PORT=4291 npx playwright test`: 244 passed, 0 failed.
  `tests/project-workspace.spec.mjs` alone: 23 passed, of which 8 are new.

XNAUT_TEST_TOTALS={"rust":[{"passed":1255,"failed":0,"ignored":45}],"ui":[244]}
