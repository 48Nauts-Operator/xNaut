// Usage footer (XNAUT-24) — MAX-plan usage strip at the bottom of the app.
// Polls the `max_usage` command (Keychain OAuth → api.anthropic.com/api/oauth/usage)
// and renders 5h / weekly / per-model % with mini progress bars. Degrades to "—".
(function () {
  const POLL_MS = 3 * 60 * 1000; // refresh every 3 min
  const invoke = (...a) => window.__TAURI__?.core?.invoke(...a);
  let appVer = ''; // app version (set once from the tauri app API), shown next to the logo

  function injectStyles() {
    if (document.getElementById('uf-styles')) return;
    const st = document.createElement('style');
    st.id = 'uf-styles';
    st.textContent = `
      #xnaut-usage-footer{flex-shrink:0;display:flex;align-items:center;gap:10px;
        height:26px;padding:0 12px;font-size:11px;line-height:1;
        background:var(--bg-secondary,#1a1a1f);border-top:1px solid var(--border,#2a2a2f);
        color:var(--text-secondary,#a0a0a0);user-select:none;overflow:hidden;white-space:nowrap}
      #xnaut-usage-footer .uf-logo{height:16px;width:16px;border-radius:4px;flex-shrink:0;margin-right:2px}
      #xnaut-usage-footer .uf-ver{opacity:.65;font-variant-numeric:tabular-nums;margin-right:6px;letter-spacing:.02em}
      #xnaut-usage-footer .uf-prov{opacity:.9;font-size:12px}
      #xnaut-usage-footer .uf-metric{display:inline-flex;align-items:center;gap:5px}
      #xnaut-usage-footer .uf-bar{width:42px;height:4px;border-radius:3px;
        background:var(--bg-tertiary,#2a2a2f);overflow:hidden;flex-shrink:0}
      #xnaut-usage-footer .uf-fill{display:block;height:100%;border-radius:3px;transition:width .3s}
      #xnaut-usage-footer .uf-pct{font-variant-numeric:tabular-nums}
      #xnaut-usage-footer .uf-lbl{color:var(--text-secondary,#a0a0a0);opacity:.7}
      #xnaut-usage-footer .uf-sep{opacity:.35;margin:0 2px}
      #xnaut-usage-footer .uf-div{opacity:.4;margin:0 9px}
      #xnaut-usage-footer .uf-spacer{flex:1}
      #xnaut-usage-footer .uf-refresh{background:none;border:none;color:inherit;cursor:pointer;
        opacity:.6;font-size:12px;padding:2px 4px;border-radius:4px}
      #xnaut-usage-footer .uf-refresh:hover{opacity:1;background:var(--bg-tertiary,#2a2a2f)}
      #xnaut-usage-footer .uf-refresh.spin{animation:uf-spin .8s linear infinite}
      @keyframes uf-spin{to{transform:rotate(360deg)}}
      #xnaut-usage-footer .uf-err{opacity:.55;font-style:italic}
    `;
    document.head.appendChild(st);
  }

  const fillColor = (pct) =>
    pct >= 85 ? '#f2555a' : pct >= 60 ? '#e0902e' : '#f5b840'; // red / amber / xNaut yellow

  function metric(pct, label) {
    const p = Math.max(0, Math.min(100, Math.round(pct)));
    return `<span class="uf-metric" title="${p}% of ${label}">`
      + `<span class="uf-bar"><span class="uf-fill" style="width:${p}%;background:${fillColor(p)}"></span></span>`
      + `<span class="uf-pct">${p}%</span> <span class="uf-lbl">${label}</span></span>`;
  }

  function claudeBlock(u) {
    if (!u) return `<span class="uf-prov" title="Claude MAX">✳</span><span class="uf-err">—</span>`;
    const parts = [
      metric(u.five_hour_pct, '5h'),
      metric(u.seven_day_pct, 'wk'),
      ...(u.per_model || []).map((m) => metric(m.percent, m.name)),
    ];
    return `<span class="uf-prov" title="Claude MAX plan usage">✳</span>`
      + parts.join('<span class="uf-sep">·</span>');
  }

  function codexBlock(u, err) {
    if (err) {
      // No session data yet reads the same as a real failure (both Err) — surface
      // the reason in the tooltip rather than silently hiding, so it's diagnosable.
      return `<span class="uf-prov" title="Codex — ${String(err).replace(/"/g, '')}">⬡</span>`
        + `<span class="uf-err">—</span>`;
    }
    if (!u || !u.primary) return '';
    const wins = [u.primary, u.secondary].filter(Boolean)
      .map((w) => metric(w.used_percent, w.window_label));
    return `<span class="uf-prov" title="Codex (${u.plan_type || 'GPT'}) — from your last codex run">⬡</span>`
      + wins.join('<span class="uf-sep">·</span>');
  }

  function render(footer, claude, codex, codexErr) {
    const blocks = [claudeBlock(claude)];
    const cb = codexBlock(codex, codexErr);
    if (cb) blocks.push(cb);
    footer.innerHTML =
      `<img class="uf-logo" src="assets/xnaut-mark.png" alt="xNAUT">`
      + `<span class="uf-ver" title="xNAUT version">${appVer ? 'v' + appVer : ''}</span>`
      + blocks.join('<span class="uf-div">|</span>')
      + `<span class="uf-spacer"></span>`
      + `<button class="uf-refresh" title="Refresh usage" aria-label="Refresh usage">↻</button>`;
    wireRefresh(footer);
  }

  function wireRefresh(footer) {
    const btn = footer.querySelector('.uf-refresh');
    if (btn) btn.onclick = () => refresh(footer, btn);
  }

  async function refresh(footer, btn) {
    if (btn) btn.classList.add('spin');
    const [claude, codex] = await Promise.allSettled([
      invoke('max_usage', { account: null }),
      invoke('codex_usage'),
    ]);
    if (codex.status === 'rejected') console.warn('[usage] codex_usage:', codex.reason);
    render(
      footer,
      claude.status === 'fulfilled' ? claude.value : null,
      codex.status === 'fulfilled' ? codex.value : null,
      codex.status === 'rejected' ? codex.reason : null,
    );
  }

  function mount() {
    const app = document.getElementById('app');
    if (!app || document.getElementById('xnaut-usage-footer')) return;
    injectStyles();
    const footer = document.createElement('footer');
    footer.id = 'xnaut-usage-footer';
    footer.innerHTML = `<img class="uf-logo" src="assets/xnaut-mark.png" alt="xNAUT"><span class="uf-ver"></span><span class="uf-prov">✳</span><span class="uf-err">usage…</span>`;
    app.appendChild(footer);
    // App version (from the tauri app API) — set once, then shown in every render.
    Promise.resolve(window.__TAURI__?.app?.getVersion?.()).then((v) => {
      if (!v) return;
      appVer = v;
      const el = footer.querySelector('.uf-ver');
      if (el) el.textContent = 'v' + v;
    }).catch(() => {});
    refresh(footer, null);
    setInterval(() => refresh(footer, null), POLL_MS);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount);
  } else {
    mount();
  }
})();
