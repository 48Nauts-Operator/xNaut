# XNAUT-75 — Surface startup failures instead of swallowing them

Branch `agent/claude/xnaut-75`, worktree
`/Users/zelda/DevHub_Studio/factory/02-Development/xnaut-worktrees/agent-claude-xnaut-75`,
machine `tron.candoo`.

## What changed

XNAUT-74 was invisible for as long as it existed. `init()` DID catch the throw and
the stack DID reach `debug.log` — but the only thing pointed at the user was
`alert()`, a no-op in Tauri's WKWebView. The window looked *broken* rather than
*errored*, so the report that came back was "nothing works" instead of "chat
sessions failed". XNAUT-80 turned `alert()` into a toast, which is better than
nothing and still wrong for this: a startup failure is a state the app is now in,
and a toast is gone in eight seconds whether or not anyone was looking.

This ticket replaces that reporting path with a record and a surface over it.

**`src/js/startup-health.js` (new)** — the record and the surface. Loaded straight
after `debug-log.js` and before `app.js`, so the recorder exists before the first
init step can fail.

- `pass(step)` / `fail(step, error)` push structured entries:
  `{ at: ISO timestamp, step, ok, error, stack }`.
- On failure it raises `#startup-error-banner`, **in the flow above the top bar**,
  never `position:fixed` over it — that is the defect `#update-banner` shipped
  (XNAUT-70), where the banner made every top-bar control unclickable.
- "What failed?" opens `#startup-health-detail`: every step with ✓/✗, its
  timestamp, its error and its stack, plus **Show debug.log** (the tail, inline),
  **Reveal debug.log**, and **Copy report**.
- The passes are what make it a first-run self-check. A list of only the broken
  things cannot tell "settings loaded" apart from "settings never ran because we
  died earlier", so the heading reads `N/M subsystems came up`.

**`src/js/app.js`** — the `step()` wrapper added in a132140 already names each
phase; it now feeds the record:

```js
const step = async (label, fn) => {
  try {
    await fn();
    health.pass(label);
  } catch (e) {
    console.error(`⚠️ init step "${label}" failed (continuing):`, e);
    health.fail(label, e);
  }
};
```

Both `alert()` reporting paths are gone: the top-level init catch and the
"Tauri API failed to load" catch now call `health.fail(...)` + `health.seal()`.
`setupEventListeners()` and `createNewTab()` report too. The accessor is
defensive on purpose — this is the reporting path for a broken startup, and a
surface that throws while reporting a failure restores exactly the silence the
ticket exists to end:

```js
function startupHealth() {
  const real = window.xnautStartupHealth;
  if (real) return real;
  const noop = () => {};
  return { pass: noop, fail: noop, seal: noop, /* … */ };
}
```

**`src/index.html`** — the script tag, and a `Startup Diagnostics` item in the
More menu. That last part is the other half of the ticket: a log nobody can find
until something breaks is not discoverable, so the detail (and the reveal button)
is reachable on a clean startup too.

**`src-tauri/src/debug_log.rs`** — two new commands so the log is reachable from
inside the app, both built on pure functions that can be asserted without
spawning anything:

```rust
fn reveal_argv(path: &Path) -> (&'static str, Vec<String>) {
    let file = path.to_string_lossy().into_owned();
    if cfg!(target_os = "macos") {
        // -R selects the file in Finder rather than opening it in whatever
        // app claims .log — on this machine that is Xcode.
        ("open", vec!["-R".into(), file])
    } else if cfg!(target_os = "windows") {
        // No space after the comma: explorer treats "/select, path" as two
        // arguments and silently opens Documents instead.
        ("explorer", vec![format!("/select,{file}")])
    } else { /* xdg-open, containing dir — it has no selection primitive */ }
}
```

`debug_log_tail(lines)` reads only the last 256 KB off disk (the log is capped at
2 MB; shipping it whole across IPC on every click is not free) and `tail_lines`
drops the leading fragment, because a byte cut lands mid-line and showing half an
entry as an entry is how a reader mis-attributes a stack.

Registered in all three places per `xnaut/adding-a-command`: `main.rs`'s
`generate_handler!`, and `permissions/default.toml`'s existing (already granted)
`allow-tasks-mode` block — so no fourth edit was needed.
`gen/schemas/acl-manifests.json` regenerated; the semantic diff is exactly
`+debug_log_reveal, +debug_log_tail`, nothing removed.

### Files

| File | |
|---|---|
| `src/js/startup-health.js` | new — the record, the banner, the detail |
| `src/js/app.js` | `step()` feeds the record; both `alert()` paths replaced; menu action |
| `src/index.html` | script tag; `Startup Diagnostics` menu item |
| `src-tauri/src/debug_log.rs` | `debug_log_reveal`, `debug_log_tail`, + 3 tests |
| `src-tauri/src/main.rs` | `generate_handler!` registration |
| `src-tauri/permissions/default.toml` | ACL: the two commands |
| `src-tauri/gen/schemas/acl-manifests.json` | regenerated by the build |
| `tests/startup-health.spec.mjs` | new — 6 UI tests |
| `tests/static-server.mjs` | stub answers for the two new commands |

## Test results

Both from my own runs in this worktree on `tron.candoo`.

```
$ cargo test --manifest-path src-tauri/Cargo.toml --bin xnaut
test result: ok. 1043 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out
```

1040 before this change; the 3 added are `debug_log::tests::reveal_points_the_file_manager_at_the_log_itself`,
`::tail_returns_the_last_whole_lines_and_never_a_fragment`, `::tail_reads_only_the_end_of_a_large_log`.

```
$ XNAUT_TEST_PORT=4297 npx playwright test
  139 passed (2.7m)
```

133 before this change; the 6 added are in `tests/startup-health.spec.mjs`.

XNAUT_TEST_TOTALS={"rust":[{"passed":1043,"failed":0,"ignored":45}],"ui":[139]}

### A note on the port

The ticket's `XNAUT_TEST_PORT=4291` could not be used for the recorded run.
Port 4291 was already held by a static server belonging to a **different**
worktree (`agent-claude-xnaut-88`), and `reuseExistingServer: true` silently
attaches to it — the exact trap `playwright.config.mjs` documents. A full run on
4291 reported `6 failed`, and the failures were `startup-health.js never loaded`,
because it was serving that worktree's `src/`. That is not a fair run of this
branch either way, so the recorded run uses a free port with the server managed
by Playwright itself. Nothing belonging to the other worktree was touched.

`npx eslint` on the changed JS reports 55 errors in `src/js/app.js` both with and
without this change (pre-existing `no-undef` for cross-file functions); the two
new files report none.

## Mutation evidence

Two mutations, one per side. Both were restored and re-proved green.

**1 — the surface (JS).** The behaviour under test is *a startup failure is
visible*, so the mutation is the XNAUT-74 silence itself: keep recording, render
nothing.

```js
// src/js/startup-health.js, in render()
-    const failed = failures();
+    const failed = []; // MUTATION: the failure is recorded but the window never says so
```

```
$ XNAUT_TEST_PORT=4293 npx playwright test tests/startup-health.spec.mjs
  ✘ a failed init step is visible in the window, and names the step
  ✘ the failure banner does not cover the top bar
  ✘ the detail reports which subsystems came up, not only which failed
  ✘ debug.log is readable from inside the app
  4 failed, 2 passed

  Error: a step failed and the window said nothing — the XNAUT-74 silence
  expect(locator).toHaveCount(expected) failed
  Expected: 1
  Received: 0
```

Restored → `6 passed (16.4s)`.

A first attempt mutated `if (!ok) render();` in `add()` and was **not** caught,
because `seal()` renders at the end of the run and masked it. Recorded here
rather than hidden: the immediate render is a redundant second path, and the
suite pins the one that matters.

**2 — the reveal (Rust).** Dropping `-R` makes the button open debug.log in
whatever claims `.log` — on this machine, Xcode.

```rust
-        ("open", vec!["-R".into(), file])
+        ("open", vec![file]) // MUTATION
```

```
$ cargo test --bin xnaut debug_log
test debug_log::tests::reveal_points_the_file_manager_at_the_log_itself ... FAILED
thread '…' panicked at src/debug_log.rs:223:13: assertion `left == right` failed
test result: FAILED. 3 passed; 1 failed
```

Restored → `test result: ok. 4 passed; 0 failed`.

## Verifying by hand

```bash
cd src-tauri && cargo tauri dev
```

1. Three-dot menu → **Startup Diagnostics**. The detail opens on a clean start
   and reads `N/N subsystems came up`, one ✓ row per init phase with its
   timestamp. **Show debug.log** prints the real tail inline; **Reveal
   debug.log** selects the file in Finder (not opens it in Xcode).
2. To see the failure surface, break one step in the running app's DevTools
   before init — or simply run
   `XNAUT_TEST_PORT=<free port> npx playwright test tests/startup-health.spec.mjs`,
   whose first test reproduces the real XNAUT-74 case (`initChatSessions()`
   throwing on a fresh profile) and asserts the red banner names `chat sessions`,
   survives past the 8s a toast would live, and does not cover the top bar.
3. Pick a port nothing else is on — `lsof -ti:<port>` must be empty, or
   `reuseExistingServer` attaches you to another worktree's frontend.

## Deliberately not done

- No change to the jury, the registry, the sweep's scheduling, or anything
  outside the files listed above.
- The banner is not persisted across restarts. A startup failure is re-derived on
  every launch, so a stored one would go stale the moment it was fixed.
- `debug.log` itself is unchanged in format and location; only its reachability
  is new.
