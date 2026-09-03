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
      #xnaut-usage-footer .uf-acct{opacity:.75;margin-left:4px;margin-right:2px}
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
      #xnaut-usage-footer .uf-disk{display:inline-flex;align-items:center;gap:5px;background:none;
        border:none;color:inherit;font:inherit;cursor:pointer;padding:2px 4px;border-radius:4px}
      #xnaut-usage-footer .uf-disk:hover{background:var(--bg-tertiary,#2a2a2f)}
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

  const esc = (s) => String(s).replace(/[<>"&]/g, (c) => `&#${c.charCodeAt(0)};`);

  // `label` is the Keychain account name, shown only when there is more than one
  // MAX account — otherwise a percentage with no owner is worse than useless.
  // Real money, from Anthropic's own accounting: extra-usage credits billed
  // once plan limits are passed. Distinct in kind from spendBlock() below,
  // which is a notional API-list-price estimate for Codex. They are never
  // added together and never share a caption. Shown only once something has
  // actually been billed; a permanent "$0.00" would just be noise.
  function extraSpendBlock(spend) {
    if (!spend || !(spend.used > 0)) return '';
    const sym = spend.currency === 'USD' ? '$' : '';
    const suffix = spend.currency === 'USD' ? '' : ' ' + esc(spend.currency);
    const cap = spend.limit != null ? ` of ${sym}${spend.limit.toFixed(2)}${suffix}` : ' (no cap set)';
    return `<span class="uf-sep">·</span><span class="uf-metric" title="Extra-usage credits actually billed `
      + `beyond your plan's included limits this period${cap}. Work inside the plan limits adds nothing here."`
      + `><span class="uf-pct">${sym}${spend.used.toFixed(2)}${suffix}</span> <span class="uf-lbl">extra</span></span>`;
  }

  function claudeBlock(u, label, err) {
    const who = label ? ` ${esc(label)}` : '';
    const tag = `<span class="uf-prov" title="Claude MAX plan usage${who ? ' —' + who : ''}">✳</span>`
      + (label ? `<span class="uf-acct">${esc(label)}</span>` : '');
    // A dash with no reason cannot be told from a genuine zero (XNAUT-257).
    // codexBlock has always surfaced its reason; this now matches it.
    if (!u) {
      return `<span class="uf-prov" title="Claude MAX plan${who} — ${esc(String(err || 'no usage data')).replace(/"/g, '')}">✳</span>`
        + (label ? `<span class="uf-acct">${esc(label)}</span>` : '')
        + `<span class="uf-err">—</span>`;
    }
    const parts = [
      metric(u.five_hour_pct, '5h'),
      metric(u.seven_day_pct, 'wk'),
      ...(u.per_model || []).map((m) => metric(m.percent, m.name)),
    ];
    return tag + parts.join('<span class="uf-sep">·</span>') + extraSpendBlock(u.spend);
  }

  // What the last Codex session would have cost at API list prices. codex_spend
  // computed this and nothing ever called it, so the number lived only in Rust.
  //
  // It is notional, never a bill: on a subscription the marginal cost is zero,
  // and codex_spend.rs is explicit that any UI showing it must say which of the
  // two it means. Hence the wording, and hence nothing at all when the model has
  // no known price. An unpriced session must not read as a free one.
  function spendBlock(spend) {
    const usd = spend && spend.length && typeof spend[0].cost_usd === 'number' ? spend[0].cost_usd : null;
    if (usd === null) return '';
    const amount = usd >= 10 ? usd.toFixed(0) : usd.toFixed(2);
    const model = spend[0].model ? ` on ${spend[0].model}` : '';
    return `<span class="uf-sep">·</span><span class="uf-metric" title="Last Codex session${model}: `
      + `about $${usd.toFixed(2)} at API list prices. A notional figure for comparing and quoting work, `
      + `not money billed to a subscription."><span class="uf-pct">~$${amount}</span> `
      + `<span class="uf-lbl">last run</span></span>`;
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

  // Disk (XNAUT-264). Shown only from 80% up: a pill that is always there is
  // furniture, and the whole point is that the owner learns he is at 90% from
  // xNAUT rather than from a failing build. He found the disk at 94% and then
  // at 98% by hand, four days apart, while this strip sat on screen saying
  // nothing about it. Clicking opens the worktree manager, which holds the
  // report of what can be reclaimed.
  function diskBlock(volume) {
    if (!volume || volume.used_pct < 80) return '';
    const p = Math.max(0, Math.min(100, volume.used_pct));
    const free = volume.free >= 1024 ** 3
      ? (volume.free / 1024 ** 3).toFixed(0) + ' GB'
      : Math.round(volume.free / 1024 ** 2) + ' MB';
    return `<span class="uf-div">|</span>`
      + `<button class="uf-disk" title="Disk ${p}% full, ${free} free. Agent worktrees and their `
      + `build caches are the usual cause. Click to see what can be reclaimed.">`
      + `<span class="uf-bar"><span class="uf-fill" style="width:${p}%;background:${fillColor(p)}"></span></span>`
      + `<span class="uf-pct">${p}%</span> <span class="uf-lbl">disk</span></button>`;
  }

  function render(footer, claudes, codex, codexErr, spend, disk) {
    const blocks = claudes.map((c) => claudeBlock(c.usage, c.label, c.err));
    const cb = codexBlock(codex, codexErr);
    if (cb) blocks.push(cb + spendBlock(spend));
    footer.innerHTML =
      `<img class="uf-logo" src="assets/xnaut-mark.png" alt="xNAUT">`
      + `<span class="uf-ver" title="xNAUT version">${appVer ? 'v' + appVer : ''}</span>`
      + blocks.join('<span class="uf-div">|</span>')
      + diskBlock(disk)
      + `<span class="uf-spacer"></span>`
      + `<button class="uf-refresh" title="Refresh usage" aria-label="Refresh usage">↻</button>`;
    wireRefresh(footer);
  }

  function wireRefresh(footer) {
    const btn = footer.querySelector('.uf-refresh');
    if (btn) btn.onclick = () => refresh(footer, btn);
    const disk = footer.querySelector('.uf-disk');
    // Exported by worktree.js; checked rather than assumed, because an
    // undefined global here would be a button that silently does nothing.
    if (disk) disk.onclick = () => {
      if (typeof window.xnautOpenWorktreeManager === 'function') window.xnautOpenWorktreeManager();
      if (typeof window.xnautHousekeeperScan === 'function') window.xnautHousekeeperScan();
    };
  }

  async function refresh(footer, btn) {
    if (btn) btn.classList.add('spin');
    // Several MAX accounts get a labelled block each; one (the usual case)
    // renders exactly as before, with no label and one request.
    let accounts = [];
    try { accounts = (await invoke('max_accounts')) || []; } catch (e) { console.warn('[usage] max_accounts:', e); }
    const wanted = accounts.length > 1 ? accounts : [null];
    const results = await Promise.allSettled([
      ...wanted.map((a) => invoke('max_usage', { account: a })),
      invoke('codex_usage'),
      invoke('codex_spend', { limit: 1 }),
      // One statvfs. Deliberately does not walk a directory, so it costs
      // nothing to poll beside the usage calls.
      invoke('housekeeper_disk'),
    ]);
    const disk = results.pop();
    const spend = results.pop();
    const codex = results.pop();
    if (codex.status === 'rejected') console.warn('[usage] codex_usage:', codex.reason);
    if (spend.status === 'rejected') console.warn('[usage] codex_spend:', spend.reason);
    render(
      footer,
      results.map((r, i) => {
        if (r.status === 'rejected') console.warn('[usage] max_usage:', r.reason);
        return {
          label: wanted.length > 1 ? wanted[i] : '',
          usage: r.status === 'fulfilled' ? r.value : null,
          err: r.status === 'rejected' ? r.reason : null,
        };
      }),
      codex.status === 'fulfilled' ? codex.value : null,
      codex.status === 'rejected' ? codex.reason : null,
      spend.status === 'fulfilled' ? spend.value : null,
      disk.status === 'fulfilled' ? disk.value : null,
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
