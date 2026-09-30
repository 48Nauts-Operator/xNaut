// Model catalog: fetches each configured provider's LIVE model list (via the
// backend chat_list_provider_models, which hits each provider's /models endpoint),
// caches it, and refreshes once a day at a random time. No more hardcoded/stale
// model ids. Exposes window.xnautModelCatalog + fires 'xnaut-model-catalog-update'.
(function () {
  'use strict';
  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  const KEY = 'xnaut-model-catalog';
  const DAY = 24 * 60 * 60 * 1000;

  let cache = null;
  try { cache = JSON.parse(localStorage.getItem(KEY) || 'null'); } catch (_) { cache = null; }

  let pending = null;
  let error = '';
  const announce = () => window.dispatchEvent(new CustomEvent('xnaut-model-catalog-update'));
  function refresh() {
    if (!pending) pending = fetchCatalog().finally(() => { pending = null; });
    return pending;
  }
  async function fetchCatalog() {
    try {
      const list = await invoke('chat_list_provider_models');
      if (!Array.isArray(list)) throw new Error('Model catalog is unavailable');
      const byProvider = {};
      const flat = [];
      list.forEach((m) => {
        const provider = String(m.provider || m.name || '').trim() || 'unknown';
        const id = String(m.model || m.id || '').trim();
        if (!id) return;
        const label = String(m.label || m.name || id);
        const entry = { id, name: label, provider };
        (byProvider[provider] = byProvider[provider] || []).push(entry);
        flat.push(entry);
      });
      if (Array.isArray(list)) {
        cache = { at: Date.now(), byProvider, flat };
        error = '';
        try { localStorage.setItem(KEY, JSON.stringify(cache)); } catch (_) {}
      }
    } catch (failure) { error = String(failure); /* retain the last successful cache and date */ }
    announce();
    return cache;
  }

  const stale = () => !cache || !cache.at || (Date.now() - cache.at) > DAY;

  window.xnautModelCatalog = {
    all: () => (cache && cache.flat) || [],
    forProvider: (p) => (cache && cache.byProvider && cache.byProvider[String(p)]) || [],
    at: () => (cache && cache.at) || 0,
    error: () => error,
    refresh,
    refreshIfStale: () => (stale() ? refresh() : Promise.resolve(cache)),
  };

  // Check on startup and after sleep/backgrounding. Timers alone can leave a
  // suspended desktop app showing yesterday's catalog when it wakes.
  function start() {
    window.xnautModelCatalog.refreshIfStale();
    const firstDelay = Math.max(60000, Math.floor(Math.random() * DAY));
    setTimeout(function daily() { refresh(); setTimeout(daily, DAY); }, firstDelay);
    window.addEventListener('focus', () => window.xnautModelCatalog.refreshIfStale());
    window.addEventListener('online', () => window.xnautModelCatalog.refreshIfStale());
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) window.xnautModelCatalog.refreshIfStale();
    });
  }
  if (window.__TAURI__ && window.__TAURI__.core) start();
  else setTimeout(start, 3000);
})();
