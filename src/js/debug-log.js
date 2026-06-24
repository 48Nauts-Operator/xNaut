// App-wide debug log (frontend half). Captures every console.{log,warn,error,
// info}, plus uncaught errors and unhandled promise rejections, and forwards
// them to the Rust side (debug_log_append) which appends to debug.log. Loaded
// FIRST so it catches early output. Flushing is batched and best-effort.
(function () {
  'use strict';

  const buf = [];
  let timer = null;

  const invoke = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);

  function flush() {
    timer = null;
    if (!buf.length) return;
    const inv = invoke();
    if (!inv) { // Tauri not ready yet — keep buffering, retry shortly.
      timer = setTimeout(flush, 500);
      return;
    }
    const entries = buf.splice(0, buf.length);
    inv('debug_log_append', { entries }).catch(() => { /* never let logging break the app */ });
  }
  function schedule() {
    if (buf.length > 200) { flush(); return; }
    if (!timer) timer = setTimeout(flush, 800);
  }

  function fmt(a) {
    if (typeof a === 'string') return a;
    if (a instanceof Error) return a.stack || a.message;
    try { return JSON.stringify(a); } catch (_) { return String(a); }
  }
  function push(level, args) {
    try {
      buf.push(`${new Date().toISOString()} [${level}] ${Array.from(args).map(fmt).join(' ')}`);
      schedule();
    } catch (_) { /* ignore */ }
  }

  ['log', 'info', 'warn', 'error'].forEach((level) => {
    const orig = console[level] ? console[level].bind(console) : function () {};
    console[level] = function () { push(level, arguments); orig.apply(null, arguments); };
  });

  window.addEventListener('error', (e) => {
    push('uncaught', [e.message, `${e.filename}:${e.lineno}:${e.colno}`, e.error && e.error.stack].filter(Boolean));
  });
  window.addEventListener('unhandledrejection', (e) => {
    const r = e.reason;
    push('rejection', [(r && (r.stack || r.message)) || String(r)]);
  });
  window.addEventListener('pagehide', flush);

  // Public hooks: jump to the log file path or clear it from elsewhere.
  window.xnautDebugLogPath = () => { const inv = invoke(); return inv ? inv('debug_log_path') : Promise.reject('no tauri'); };
  window.xnautDebugLogClear = () => { const inv = invoke(); return inv ? inv('debug_log_clear') : Promise.reject('no tauri'); };

  console.log('[debug-log] capture active');
})();
