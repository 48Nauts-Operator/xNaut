# XNAUT-393: a Copy id button on every ticket row

Branch `agent/claude/xnaut-393`. Demo ticket, 2026-09-14.

## What changed

Each row of the Work list now carries a small copy button after the id. It
copies the ticket id to the clipboard, shows a one second "Copied XNAUT-123"
toast, and does not open the ticket.

Two files:

- `src/js/project-management-panel.js`
  - CSS for `.pmw-copy-id`: muted by default, brand yellow on hover, a visible
    `:focus-visible` outline. Deliberately not hover-only, because the
    acceptance is that every row *shows* the button and a hover-only control is
    one a keyboard user never finds.
  - `toast(message, error, ms)` gained an optional duration. It defaults to the
    previous 3500ms, so every existing caller is unchanged.
  - New `copyToClipboard(text)`: `navigator.clipboard.writeText` first, falling
    back to an offscreen textarea plus `document.execCommand('copy')`. The
    fallback is real rather than ceremonial: `navigator.clipboard` is absent on
    an insecure origin and rejects when the document is not focused.
  - The list row renders the button inside `td.pmw-c-id`.
  - `bindTickets()` binds the buttons before the rows and calls
    `event.stopPropagation()`.

- `tests/work-filter.spec.mjs`
  - One new test: the button exists on all three rows, a click writes the id to
    a stubbed clipboard, the toast appears and then clears, the detail pane
    never opens, and Enter on a focused button copies too.

## The one trap worth recording

`bindTickets()` binds `openTicket` to every `[data-id]` node under
`.pmw-content`. The copy button therefore uses `data-copy-id`, not `data-id`:
had it reused `data-id`, the button would itself have been wired to open the
ticket, which is the exact behaviour the ticket asks to prevent. The button
also sits inside `tr[data-id]`, so `stopPropagation` is what keeps the click
from bubbling to the row. Enter on a focused `<button>` dispatches the same
click event, so the keyboard requirement needs no separate keydown handler.

## How to verify by hand

1. `cd src-tauri && cargo tauri dev`
2. Open a project, Work section, List view.
3. Every row shows a small copy glyph after the id. Hover it: it turns yellow
   and its tooltip reads "Copy XNAUT-123".
4. Click it. A "Copied XNAUT-123" toast appears at the bottom of the pane and
   clears after about a second. The ticket detail pane does NOT open.
5. Paste somewhere: the ticket id is on the clipboard.
6. Tab to the button and press Enter: same copy, same toast, still no detail
   pane.

## Test runs

Both from this worktree, on `tron.candoo`.

- `cargo test --manifest-path src-tauri/Cargo.toml`
  → `test result: ok. 1250 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out`
- `XNAUT_TEST_PORT=4291 npx playwright test`
  → `237 passed (3.1m)`

`node scripts/hygiene-check.mjs` reports two failures, both pre-existing and
untouched by this change: an undefined identifier at `vault-pane.js:491` and
three unattributed test fns in `foundation.rs`, `main.rs` and `veto.rs`. None of
those files appear in this diff.

## Deliberately not done

Copying the title or a ticket URL, and the board view. Both are named out of
scope on the ticket. The board's `pmw-card` shows the id in its meta row and
would be the obvious next place for the same button.

XNAUT_TEST_TOTALS={"rust":[{"passed":1250,"failed":0,"ignored":45}],"ui":[237]}
