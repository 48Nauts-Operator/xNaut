// One native accounting feed for the footer and Observatory. No credentials,
// audio or transcript text cross this boundary. Dollars are usage estimates,
// not an invoice; the provider's final duration replaces the running estimate.
(function () {
  'use strict';
  let summary = null, error = '', pending = false;
  const cards = new Set();
  const money = value => '$' + Number(value || 0).toFixed(3);
  const minutes = value => value < 60 ? value.toFixed(1) + ' min' : (value / 60).toFixed(1) + ' hr';
  const storageKey = 'xnaut-voice-speed-assumptions';
  let speeds = { speech: 149, typing: 60 };
  const valid = value => Number.isFinite(value) && value >= 1 && value <= 500;
  try { const saved = JSON.parse(localStorage.getItem(storageKey)); if (saved && valid(saved.speech) && valid(saved.typing)) speeds = saved; } catch (_) {}
  function savedMinutes(words) { return Math.max(0, Number(words || 0) * (1 / speeds.typing - 1 / speeds.speech)); }
  function sessionPrice() {
    const s = summary?.session;
    if (!s) return '$0.000';
    if (s.ratePerMinute == null) return 'Rate unavailable';
    return (s.finalized ? '' : '~') + money(s.seconds / 60 * s.ratePerMinute);
  }
  function mountFooter(footer) {
    let label = footer.querySelector('[data-voice-cost-footer]');
    if (!label) {
      label = document.createElement('span'); label.dataset.voiceCostFooter = ''; label.style.flexShrink = '0';
      const anchor = footer.querySelector('.uf-spacer'); footer.insertBefore(label, anchor || null);
    }
    label.hidden = !summary?.configured && !error;
    label.textContent = error ? 'Voice cost unavailable' : `Voice ${summary?.active ? 'live' : 'last'} ${sessionPrice()} · month ${summary?.unconfirmedSessions ? '~' : ''}${money(summary?.monthCostUsd)}${summary?.unpricedSessions ? ' + unpriced' : ''}`;
    label.title = error || 'GPT-Live-1: $0.05/min, billed per second while connected, including silence and mute. Backend model charges are separate. ~ means estimated; final provider duration replaces the estimate. Month = sessions started this calendar month, tracked on this device since this feature was enabled.';
  }
  function paintCard(card) {
    card.hidden = !summary?.configured;
    if (card.hidden) return;
    card.querySelector('[data-voice-session-label]').textContent = summary.active ? 'Voice · running cost' : 'Voice · last session';
    card.querySelector('[data-voice-price]').textContent = error ? 'Unavailable' : sessionPrice();
    card.querySelector('[data-voice-saved]').textContent = minutes(savedMinutes(summary.session?.words)) + ' estimated input time saved';
    card.querySelector('[data-voice-month]').textContent = `${summary.month}: ${summary.unconfirmedSessions ? '~' : ''}${money(summary.monthCostUsd)}${summary.unpricedSessions ? ' + unpriced sessions' : ''} · ${minutes(savedMinutes(summary.monthWords))} saved`;
    card.querySelector('[data-voice-detail]').textContent = error || (summary.session
      ? `${summary.session.model} · ${Math.floor(summary.session.seconds)}s connected · ${summary.session.words} spoken words${summary.session.finalized ? ' · final duration' : ' · estimated duration'}`
      : 'Tracking starts with your next voice session.');
    card.querySelector('[data-voice-assumptions]').textContent = `${speeds.speech} speech / ${speeds.typing} typing WPM`;
    for (const key of ['speech', 'typing']) {
      const input = card.querySelector(`[name="${key}"]`); if (document.activeElement !== input) input.value = speeds[key];
    }
  }
  function paint() {
    const footer = document.getElementById('xnaut-usage-footer'); if (footer) mountFooter(footer);
    for (const card of cards) { if (!card.isConnected) cards.delete(card); else paintCard(card); }
  }
  function mountCard(parent) {
    const card = document.createElement('div'); card.className = 'obs-card obs-voice-cost'; card.hidden = true;
    card.innerHTML = `<span class="k" data-voice-session-label>Voice · running cost</span>
      <div class="obs-big small"><b data-voice-price></b><span>USD</span></div>
      <span class="obs-hint" data-voice-saved></span><span class="obs-hint" data-voice-month></span>
      <span class="obs-hint" data-voice-detail></span>
      <details><summary class="obs-hint" data-voice-assumptions></summary>
        <label class="obs-hint">Speech WPM <input class="obs-select" style="width:75px" name="speech" aria-label="Speech words per minute" type="number" min="1" max="500"></label>
        <label class="obs-hint">Typing WPM <input class="obs-select" style="width:75px" name="typing" aria-label="Typing words per minute" type="number" min="1" max="500"></label>
        <p class="obs-hint">Input-time estimate from spoken words and these assumed speeds. Excludes listening, waiting and editing. $0.05/min for GPT-Live-1 includes all connected time, even muted. Backend charges are separate. Monthly totals cover sessions started this month on this device; ~ means duration is not final.</p>
      </details>`;
    card.querySelectorAll('input').forEach(input => {
      input.onchange = () => {
        const value = Number(input.value);
        if (!valid(value)) { input.value = speeds[input.name]; return; }
        speeds[input.name] = value;
        try { localStorage.setItem(storageKey, JSON.stringify(speeds)); } catch (_) {}
        paint();
      };
    });
    parent.appendChild(card); cards.add(card); paintCard(card); refresh(); return card;
  }
  async function refresh() {
    if (pending || !window.__TAURI__?.core?.invoke) return;
    pending = true;
    try {
      const next = await window.__TAURI__.core.invoke('voice_live_usage');
      // Older runtimes and browser fixtures may not expose this command.
      if (!next || typeof next.configured !== 'boolean') return;
      summary = next; error = '';
    } catch (e) { error = String(e); }
    finally { pending = false; paint(); }
  }
  window.xnautVoiceCost = { mountFooter, mountCard, refresh };
  setInterval(refresh, 2000);
  window.addEventListener('focus', refresh);
  refresh();
})();
