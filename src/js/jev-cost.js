// xNaut receipts only. Jev execution is not enabled by displaying this card.
(() => {
  let value = null, error = '', pending = false;
  const cards = new Set();
  function paint(card) {
    card.hidden = !value?.configured && !value?.calls && !error;
    card.querySelector('[data-jev-price]').textContent = error ? 'Unavailable'
      : value?.unknownCalls ? '$' + Number(value.costUsd || 0).toFixed(5) + ' + unknown'
      : value?.unpricedCalls ? '$' + value.costUsd.toFixed(5) + ' + unpriced'
      : '$' + Number(value?.costUsd || 0).toFixed(5);
    card.querySelector('[data-jev-detail]').textContent = error || (value?.calls
      ? `${value.month} · ${value.calls} calls · ${value.inputTokens.toLocaleString()} input tokens`
      : 'No xNaut calls recorded.');
  }
  async function refresh() {
    if (pending) return;
    pending = true;
    try {
      const next = await window.__TAURI__.core.invoke('jev_usage');
      if (next && typeof next.configured === 'boolean') { value = next; error = ''; }
    } catch (_) { error = 'Usage unavailable; last total is not current.'; }
    finally {
      pending = false;
      for (const ref of cards) { const card = ref.deref(); if (card) paint(card); else cards.delete(ref); }
    }
  }
  function mountCard(parent) {
    const card = document.createElement('div'); card.className = 'obs-card obs-jev-cost'; card.hidden = true;
    card.innerHTML = '<span class="k">Jev · xNaut this month</span><div class="obs-big small"><b data-jev-price></b><span>USD est.</span></div><span class="obs-hint" data-jev-detail></span><details><summary class="obs-hint">Usage scope &amp; pricing</summary><p class="obs-hint">Only recorded xNaut requests on this device. Other tools and account charges are excluded. Configure tool selection under Observatory → Decisions. Calls without usage receipts remain unknown. Jev 1.13: $0.042 per million input tokens; output is free. Unknown models remain unpriced.</p></details>';
    parent.appendChild(card); cards.add(new WeakRef(card)); paint(card); void refresh(); return card;
  }
  window.xnautJevCost = { mountCard, refresh };
  setInterval(() => { if (Array.from(cards).some(ref => ref.deref()?.isConnected)) void refresh(); }, 30000);
})();
