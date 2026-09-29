// Conversation data belongs to the user, not a particular preview bundle ID.
// Keep synchronous UI reads, serialize native writes, and retain the local copy
// on errors. The durable store rejects stale concurrent writers explicitly.
(function () {
  'use strict';
  const cache = new Map();
  const revisions = new Map();
  const failed = new Set();
  const allowed = key => ['xnaut-agent-threads:v1','xnaut-chat-sessions','xnaut-librarian-threads-migrated'].includes(key)
    || ['xnaut-chat-history:','xnaut-chat-model:','xnaut-chat-title:'].some(prefix => key.startsWith(prefix));
  let loading, native = false, queue = Promise.resolve();
  function report(error) {
    console.error('[conversation-storage]', error);
    let banner = document.getElementById('conversation-storage-error');
    if (!banner) {
      banner = document.createElement('div'); banner.id = 'conversation-storage-error'; banner.setAttribute('role','alert');
      banner.style.cssText = 'position:fixed;bottom:28px;left:16px;right:16px;z-index:99999;background:#4b2727;color:#fff;padding:12px;border:1px solid #ed776c;border-radius:8px';
      document.body.appendChild(banner);
    }
    banner.textContent = `Conversation storage: ${error}. Your browser copy is retained. Do not close this window until the storage issue is resolved.`;
  }
  function snapshot() {
    const values = {};
    for (let i=0;i<localStorage.length;i++) { const key=localStorage.key(i); if (allowed(key)) values[key]=localStorage.getItem(key); }
    return values;
  }
  function ready() {
    if (loading) return loading;
    loading = (async () => {
      const legacy = snapshot();
      const pending = {};
      for (let i=0;i<localStorage.length;i++) {
        const key=localStorage.key(i);
        if (key.startsWith('xnaut-conversation-pending:')) pending[key.slice('xnaut-conversation-pending:'.length)]=localStorage.getItem(key);
      }
      if (!window.__TAURI__?.core?.invoke) return;
      try {
        const records = await window.__TAURI__.core.invoke('conversation_store_load',{legacy, pending});
        if (!records || typeof records !== 'object' || Array.isArray(records)) throw new Error('Native conversation storage returned no record map');
        for (const [key,record] of Object.entries(records)) {
          if (!allowed(key)) continue;
          cache.set(key,record.value); revisions.set(key,record.revision);
          // Browser cache is best effort, never the only durable copy.
          try { if (record.value == null) localStorage.removeItem(key); else localStorage.setItem(key,record.value); } catch (_) { /* native store owns this copy */ }
        }
        native = true;
        for (const [key,value] of Object.entries(pending)) {
          // The backend has kept this interrupted write as a recovery snapshot.
          if (localStorage.getItem('xnaut-conversation-pending:' + key) === value) localStorage.removeItem('xnaut-conversation-pending:' + key);
        }
      } catch (error) { report(error); throw error; }
    })();
    return loading;
  }
  function save(key,value) {
    if (!allowed(key)) { if (value == null) localStorage.removeItem(key); else localStorage.setItem(key,value); return; }
    const pendingValue = JSON.stringify({value,at:Date.now()});
    try { localStorage.setItem('xnaut-conversation-pending:' + key,pendingValue); } catch (error) { report(error); }
    cache.set(key,value);
    try { if (value==null) localStorage.removeItem(key); else localStorage.setItem(key,value); } catch (error) { if (!native) report(error); }
    queue = queue.then(async () => {
      try {
        await ready();
        if (!native || failed.has(key)) return;
        const result=await window.__TAURI__.core.invoke('conversation_store_put',{key,value,revision:revisions.get(key)||0});
        if (!result || !Number.isInteger(result.revision)) throw new Error('Conversation save was not acknowledged');
        revisions.set(key,result.revision);
        if (localStorage.getItem('xnaut-conversation-pending:' + key) === pendingValue) localStorage.removeItem('xnaut-conversation-pending:' + key);
      } catch (error) { failed.add(key); report(error); }
    });
  }
  window.xnautConversationStorage = {
    ready, flush: () => queue,
    getItem: key => allowed(key) && cache.has(key) ? cache.get(key) : localStorage.getItem(key),
    setItem: (key,value) => save(key,String(value)), removeItem: key => save(key,null),
    keys: () => [...new Set([...Object.keys(snapshot()), ...cache.keys()])].filter(key => cache.has(key) ? cache.get(key)!=null : true),
  };
})();
