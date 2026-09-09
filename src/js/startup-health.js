// The startup health record, and the surface that makes it visible (XNAUT-75).
//
// XNAUT-74 was invisible for as long as it existed. init() DID catch the throw
// and it DID reach debug.log, but the only thing pointed at the user was
// alert(), which is a no-op in Tauri's WKWebView. So the window did not look
// errored, it looked merely broken, and the report that came back was "nothing
// works" rather than "chat sessions failed". dialogs.js (XNAUT-80) turned
// alert() into a toast, which is better than nothing and still wrong for this:
// a startup failure is not a passing notice, it is a state the app is now in,
// and a toast is gone in eight seconds whether or not anyone was looking.
//
// So: a record, and a surface over it.
//
//   record   every named init phase reports here, pass AND fail, with a
//            timestamp, the error and the stack. The passes are what make it a
//            self-check: "which subsystems came up" is only answerable if the
//            ones that worked are written down too.
//   banner   raised only on failure, in the flow above the top bar. NEVER
//            position:fixed over it — that is the defect update-banner shipped
//            (XNAUT-70), where the banner ate every top-bar control underneath.
//   detail   the banner and the More menu both open it. Every step with its
//            outcome, the stacks, and the two things support actually needs:
//            the tail of debug.log inline, and a button that reveals the file.
//
// Loaded straight after debug-log.js and before app.js, so the recorder exists
// before the first step can fail. It deliberately depends on nothing: if the
// Tauri bridge never arrives, the record and the banner still work and only the
// two log buttons degrade.
(function () {
  'use strict';

  /** @type {{at:string,step:string,ok:boolean,error:string,stack:string}[]} */
  const steps = [];
  let sealed = false;

  const invoke = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
  const failures = () => steps.filter((s) => !s.ok);

  function describe(error) {
    if (error == null) return { error: 'unknown error', stack: '' };
    if (error instanceof Error) return { error: error.message || String(error), stack: error.stack || '' };
    if (typeof error === 'string') return { error, stack: '' };
    try { return { error: JSON.stringify(error), stack: '' }; } catch (_) { return { error: String(error), stack: '' }; }
  }

  function add(step, ok, error) {
    const { error: message, stack } = ok ? { error: '', stack: '' } : describe(error);
    steps.push({ at: new Date().toISOString(), step: String(step), ok, error: message, stack });
    if (!ok) render();
  }

  // ---- the record -----------------------------------------------------------

  function pass(step) { add(step, true, null); }
  function fail(step, error) {
    add(step, false, error);
    // The log is where the detail lives permanently; console.error is what puts
    // it there, via debug-log.js. Prefixed so it is greppable in debug.log.
    try { console.error(`[startup] step "${step}" failed:`, error); } catch (_) { /* never break init */ }
  }

  /** Plain-text report — what a user pastes into a bug report. */
  function report() {
    const lines = [`xNAUT startup report (${steps.length} steps, ${failures().length} failed)`];
    for (const s of steps) {
      lines.push(`${s.ok ? 'ok  ' : 'FAIL'} ${s.at} ${s.step}${s.ok ? '' : ` — ${s.error}`}`);
      if (s.stack) lines.push(s.stack.split('\n').map((l) => `       ${l}`).join('\n'));
    }
    return lines.join('\n');
  }

  // ---- the banner -----------------------------------------------------------

  function render() {
    if (!document.body) { // called before the DOM exists; the next add() retries
      document.addEventListener('DOMContentLoaded', render, { once: true });
      return;
    }
    const failed = failures();
    const existing = document.getElementById('startup-error-banner');
    if (!failed.length) { if (existing) existing.remove(); return; }

    const label = failed.length === 1
      ? `Startup problem: ${failed[0].step} failed`
      : `Startup problems: ${failed.length} steps failed`;
    if (existing) {
      const text = existing.querySelector('[data-startup-text]');
      if (text) text.textContent = label;
      return;
    }

    const banner = document.createElement('div');
    banner.id = 'startup-error-banner';
    banner.setAttribute('role', 'alert');
    // In the flow, not fixed over the top bar. See XNAUT-70.
    banner.style.cssText = 'flex:0 0 auto; background:#7f1d1d; color:#fff; padding:8px 16px; display:flex;'
      + ' justify-content:center; align-items:center; gap:12px; font-size:13px; font-weight:500;';

    const text = document.createElement('span');
    text.setAttribute('data-startup-text', '');
    text.textContent = label;

    const details = document.createElement('button');
    details.id = 'startup-error-details';
    details.textContent = 'What failed?';
    details.setAttribute('aria-label', 'Show startup failure details');
    details.style.cssText = 'background:#fff; color:#7f1d1d; border:none; padding:4px 14px; border-radius:4px;'
      + ' font-size:12px; font-weight:600; cursor:pointer;';
    details.onclick = show;

    const dismiss = document.createElement('button');
    dismiss.textContent = '×';
    dismiss.setAttribute('aria-label', 'Dismiss startup problem banner');
    dismiss.style.cssText = 'background:none; border:none; color:#fff; cursor:pointer; font-size:18px; margin-left:8px;';
    dismiss.onclick = () => banner.remove();

    banner.append(text, details, dismiss);
    (document.getElementById('app') || document.body).prepend(banner);
  }

  // ---- the detail -----------------------------------------------------------

  function hide() {
    const overlay = document.getElementById('startup-health-detail');
    if (overlay) overlay.remove();
  }

  function show() {
    hide();
    const overlay = document.createElement('div');
    overlay.id = 'startup-health-detail';
    overlay.setAttribute('role', 'dialog');
    overlay.setAttribute('aria-modal', 'true');
    overlay.setAttribute('aria-label', 'Startup diagnostics');
    overlay.style.cssText = 'position:fixed; inset:0; z-index:1250; display:flex; align-items:center;'
      + ' justify-content:center; background:rgba(0,0,0,.55);';

    const box = document.createElement('div');
    box.style.cssText = 'background:var(--bg-secondary,#1a1a1f); border:1px solid var(--border,#2a2a2f);'
      + ' border-radius:10px; padding:18px 20px; width:min(720px,92vw); max-height:82vh; display:flex;'
      + ' flex-direction:column; gap:12px; color:var(--text-primary,#e0e0e0);'
      + ' font-family:var(--font-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); font-size:13px;';

    const failed = failures();
    const heading = document.createElement('div');
    heading.style.cssText = 'font-weight:600; font-size:14px;';
    heading.textContent = steps.length === 0
      ? 'Startup diagnostics'
      : `Startup diagnostics — ${steps.length - failed.length}/${steps.length} subsystems came up`;

    // The self-check list. Passes are shown as well as failures, because the
    // question a user is answering is "what came up", and a list of only the
    // broken things cannot distinguish "settings loaded" from "settings never
    // ran because we died earlier".
    const list = document.createElement('div');
    list.id = 'startup-health-steps';
    list.style.cssText = 'overflow:auto; border:1px solid var(--border,#2a2a2f); border-radius:8px;'
      + ' background:var(--bg-primary,#0a0a0f); padding:8px 10px; display:flex; flex-direction:column; gap:6px;';
    if (!steps.length) {
      const empty = document.createElement('div');
      empty.style.cssText = 'color:var(--text-secondary,#a0a0a0);';
      empty.textContent = 'No startup steps have been recorded yet.';
      list.appendChild(empty);
    }
    for (const s of steps) {
      const row = document.createElement('div');
      row.className = 'startup-health-step';
      row.dataset.ok = String(s.ok);
      row.dataset.step = s.step;
      // textContent throughout: an error message can carry a path, a shell
      // fragment or a chunk of HTML, and this surface exists to be readable
      // when things are already wrong.
      const head = document.createElement('div');
      head.style.cssText = `font-family:var(--font-mono,ui-monospace,monospace); color:${s.ok ? '#4ade80' : '#f87171'};`;
      head.textContent = `${s.ok ? '✓' : '✗'} ${s.step}${s.ok ? '' : ` — ${s.error}`}`;
      row.appendChild(head);
      const when = document.createElement('div');
      when.style.cssText = 'color:var(--text-secondary,#a0a0a0); font-size:11px;';
      when.textContent = s.at;
      row.appendChild(when);
      if (s.stack) {
        const stack = document.createElement('pre');
        stack.style.cssText = 'margin:4px 0 0; white-space:pre-wrap; font-size:11px;'
          + ' color:var(--text-secondary,#a0a0a0); max-height:180px; overflow:auto;';
        stack.textContent = s.stack;
        row.appendChild(stack);
      }
      list.appendChild(row);
    }

    const logBox = document.createElement('pre');
    logBox.id = 'startup-health-log';
    logBox.hidden = true;
    logBox.style.cssText = 'margin:0; white-space:pre-wrap; font-size:11px; max-height:220px; overflow:auto;'
      + ' border:1px solid var(--border,#2a2a2f); border-radius:8px; background:var(--bg-primary,#0a0a0f);'
      + ' padding:8px 10px; color:var(--text-secondary,#a0a0a0);';

    const actions = document.createElement('div');
    actions.style.cssText = 'display:flex; gap:8px; justify-content:flex-end; flex-wrap:wrap;';
    const button = (id, label, onclick) => {
      const b = document.createElement('button');
      b.id = id;
      b.textContent = label;
      b.setAttribute('aria-label', label);
      b.style.cssText = 'font:inherit; font-size:12px; padding:6px 14px; border-radius:7px; cursor:pointer;'
        + ' border:1px solid var(--border,#2a2a2f); background:transparent; color:var(--text-secondary,#a0a0a0);';
      b.onclick = onclick;
      actions.appendChild(b);
      return b;
    };

    // Both log buttons say what went wrong IN THE BUTTON when the bridge is
    // missing. Reporting a failure of the failure surface through the failure
    // surface is the one place a silent catch would be indefensible.
    button('startup-health-show-log', 'Show debug.log', async () => {
      const inv = invoke();
      if (!inv) { logBox.hidden = false; logBox.textContent = 'The Tauri bridge is not available, so debug.log cannot be read.'; return; }
      logBox.hidden = false;
      logBox.textContent = 'Reading debug.log…';
      try {
        const body = await inv('debug_log_tail', { lines: 200 });
        logBox.textContent = body && String(body).trim() ? String(body) : 'debug.log is empty.';
      } catch (e) {
        logBox.textContent = `Could not read debug.log: ${describe(e).error}`;
      }
    });
    const reveal = button('startup-health-reveal', 'Reveal debug.log', async () => {
      const inv = invoke();
      if (!inv) { reveal.textContent = 'No Tauri bridge'; return; }
      try {
        await inv('debug_log_reveal');
      } catch (e) {
        reveal.textContent = `Reveal failed: ${describe(e).error}`;
      }
    });
    const copy = button('startup-health-copy', 'Copy report', async () => {
      const text = report();
      try {
        await navigator.clipboard.writeText(text);
        copy.textContent = 'Copied';
      } catch (_) {
        // Clipboard is permission-gated and refuses in some webviews. Falling
        // back silently would be the original sin of this ticket.
        logBox.hidden = false;
        logBox.textContent = text;
        copy.textContent = 'Copy failed — shown below';
      }
    });
    const close = button('startup-health-close', 'Close', hide);
    close.style.cssText += ' background:#f5b840; color:#0a0a0f; border:none; font-weight:600;';

    box.append(heading, list, logBox, actions);
    overlay.appendChild(box);
    overlay.onclick = (e) => { if (e.target === overlay) hide(); };
    const onKey = (e) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      document.removeEventListener('keydown', onKey, true);
      hide();
    };
    document.addEventListener('keydown', onKey, true);
    document.body.appendChild(overlay);
    close.focus();
    return overlay;
  }

  /** Called once init() has run every step, so "0 of 0 came up" is never shown
   *  as if it were a clean bill of health. */
  function seal() { sealed = true; render(); }

  window.xnautStartupHealth = {
    pass, fail, seal, show, hide, report,
    steps: () => steps.slice(),
    failures,
    sealed: () => sealed,
  };
})();
