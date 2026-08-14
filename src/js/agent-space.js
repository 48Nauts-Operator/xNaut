// Agent Space — identity-first agent library, bounded conversations, creation,
// and settings. Runtime output remains in the existing terminal tabs.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const listen = (...args) => window.__TAURI__.event.listen(...args);
  const THREADS_KEY = 'xnaut-agent-threads:v1';
  const SHARED_CONTEXT_KEY = 'xnaut-portable-agent-context:v1';
  const MAX_THREADS = 12;
  const MAX_MESSAGES = 80;
  const panes = new Map();

  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[character]));
  const handleOf = (value) => String(value || '').trim().replace(/^@/, '').toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64);
  const nowIso = () => new Date().toISOString();

  function loadSharedContext() {
    try {
      const value = JSON.parse(localStorage.getItem(SHARED_CONTEXT_KEY) || '[]');
      return Array.isArray(value) ? value : [];
    } catch (_) { return []; }
  }

  function saveSharedMessage(message) {
    const current = loadSharedContext();
    const next = current.filter((item) => item.id !== message.id).concat(message).slice(-40);
    try { localStorage.setItem(SHARED_CONTEXT_KEY, JSON.stringify(next)); } catch (_) {}
  }

  function sharedContextText() {
    return loadSharedContext().map((message) => {
      const speaker = message.role === 'user' ? 'User' : (message.agent ? `@${message.agent}` : 'Agent');
      return `${speaker}: ${String(message.text || '').trim()}`;
    }).filter((line) => !/:\s*$/.test(line)).join('\n\n').slice(-24000);
  }

  function portableHandoff(profile, thread) {
    const controlHistory = window.xnautGetChatHistory
      ? window.xnautGetChatHistory('control-center:nautbot')
      : [];
    const controlText = (controlHistory || []).slice(-18).map((message) =>
      `${message.role === 'user' ? 'User' : 'NautBot'}: ${String(message.display || message.content || '').trim()}`
    ).filter((line) => !/:\s*$/.test(line)).join('\n\n');
    const specialistText = sharedContextText();
    const conversation = [controlText, specialistText].filter(Boolean).join('\n\n');
    if (!conversation) return '';
    return [
      'PORTABLE XNAUT CONVERSATION HANDOFF',
      'Continue the same conversation and work. Do not restart discovery or ask for context already present here.',
      `Active responder: @${profile.handle}`,
      `Active project/worktree: ${profile.default_project || '(not assigned)'}`,
      `Thread: ${thread.title || 'Untitled thread'}`,
      '',
      conversation,
    ].join('\n').slice(-30000);
  }

  window.xnautSharedAgentContextText = sharedContextText;

  function decodeTerminalBytes(encoded) {
    const binary = atob(String(encoded || ''));
    return new TextDecoder('utf-8').decode(Uint8Array.from(binary, (character) => character.charCodeAt(0)));
  }

  function encodeTerminalInput(value) {
    const bytes = new TextEncoder().encode(value);
    let binary = '';
    bytes.forEach((byte) => { binary += String.fromCharCode(byte); });
    return btoa(binary);
  }

  function terminalMirror(prompt, onChange) {
    let raw = '';
    let terminal = null;
    let host = null;
    if (window.Terminal) {
      terminal = new window.Terminal({ cols:120, rows:40, scrollback:1200, convertEol:true, disableStdin:true });
      host = document.createElement('div');
      host.setAttribute('aria-hidden', 'true');
      host.style.cssText = 'position:fixed;left:-10000px;top:0;width:960px;height:640px;visibility:hidden;pointer-events:none';
      document.body.appendChild(host);
      terminal.open(host);
    }
    const readable = () => {
      let value = raw;
      if (terminal) {
        const buffer = terminal.buffer.active;
        const lines = [];
        for (let index = 0; index < buffer.length; index += 1) {
          const line = buffer.getLine(index);
          if (line) lines.push(line.translateToString(true).trimEnd());
        }
        value = lines.join('\n');
      } else {
        value = value
          .replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '')
          .replace(/\x1b\[[0-?]*[ -\/]*[@-~]/g, '');
      }
      const marker = String(prompt || '').trim();
      const markerIndex = marker ? value.lastIndexOf(marker) : -1;
      if (markerIndex >= 0) value = value.slice(markerIndex + marker.length);
      return value.split('\n')
        .filter((line) => !/^\s*(esc to|shift\+tab|ctrl\+|tokens?:|context:|working \([^)]+\))\b/i.test(line))
        .join('\n').replace(/\n{4,}/g, '\n\n\n').trim().slice(-24000);
    };
    return {
      push(value) {
        if (!value) return;
        raw = (raw + value).slice(-262144);
        if (terminal) terminal.write(value, () => onChange(readable()));
        else onChange(readable());
      },
      dispose() {
        if (terminal) terminal.dispose();
        if (host) host.remove();
      },
    };
  }

  function loadThreads() {
    try {
      const value = JSON.parse(localStorage.getItem(THREADS_KEY) || '{}');
      return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
    } catch (_) { return {}; }
  }

  function saveThreads(value) {
    try { localStorage.setItem(THREADS_KEY, JSON.stringify(value)); } catch (_) { /* quota */ }
    window.dispatchEvent(new CustomEvent('xnaut:agent-threads-changed'));
  }

  function threadsFor(handle) {
    const all = loadThreads();
    return Array.isArray(all[handle]) ? all[handle] : [];
  }

  function writeThread(handle, thread) {
    const all = loadThreads();
    const current = Array.isArray(all[handle]) ? all[handle] : [];
    const next = [thread, ...current.filter((item) => item.id !== thread.id)]
      .sort((left, right) => String(right.updated_at).localeCompare(String(left.updated_at)))
      .slice(0, MAX_THREADS);
    all[handle] = next;
    saveThreads(all);
    return thread;
  }

  function newThread(handle, title) {
    const stamp = nowIso();
    return writeThread(handle, {
      id: `${handle}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      title: String(title || 'New thread').trim().slice(0, 64) || 'New thread',
      created_at: stamp,
      updated_at: stamp,
      messages: [],
      session_id: null,
    });
  }

  function updateThread(handle, id, updater) {
    const found = threadsFor(handle).find((item) => item.id === id) || newThread(handle, 'New thread');
    const next = updater({ ...found, messages: Array.isArray(found.messages) ? found.messages.slice() : [] }) || found;
    next.updated_at = nowIso();
    next.messages = next.messages.slice(-MAX_MESSAGES);
    return writeThread(handle, next);
  }

  function announceProfilesChanged(profile) {
    window.dispatchEvent(new CustomEvent('xnaut:agent-profiles-changed', { detail: profile || null }));
    if (window.xnautSidebarRefresh) window.xnautSidebarRefresh();
  }

  function ensureStyles() {
    if (document.getElementById('agent-space-styles')) return;
    const style = document.createElement('style');
    style.id = 'agent-space-styles';
    style.textContent = `
      .agent-space { --as-accent:var(--agent-thinking,#f5b840); display:flex; flex-direction:row; flex:1 1 auto;
        min-width:0; min-height:0; color:var(--text-primary,#e8e8ec); background:var(--bg-primary,#101014);
        font-family:var(--font-sans,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); }
      .agent-space * { box-sizing:border-box; }
      .as-stage { position:relative; display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0; }
      .asl { display:flex; flex:0 0 230px; width:230px; min-height:0; flex-direction:column; overflow:hidden;
        border-right:1px solid var(--border-color,var(--border,#303038)); background:var(--editor-surface,#18181d); }
      .asl-head { display:flex; align-items:center; justify-content:space-between; min-height:52px; padding:10px 14px;
        border-bottom:1px solid var(--border-color,var(--border,#303038)); color:var(--text-secondary,#8c8c96); font-size:10px; font-weight:720; letter-spacing:.12em; text-transform:uppercase; }
      .asl-add { display:grid; place-items:center; width:27px; height:27px; padding:0; border:1px solid var(--border-color,#373740); border-radius:7px;
        color:var(--as-accent); background:var(--bg-tertiary,#24242a); font:18px/1 var(--font-sans,sans-serif); cursor:pointer; }
      .asl-list { flex:1 1 auto; min-height:0; overflow-y:auto; padding:7px 6px 16px; }
      .asl-agent { position:relative; display:flex; align-items:center; gap:9px; padding:8px; border-radius:7px; cursor:pointer; }
      .asl-agent:hover,.asl-thread:hover { background:rgba(255,255,255,.05); }.asl-agent.selected { background:rgba(255,255,255,.075); box-shadow:inset 2px 0 0 var(--agent-accent,#f5b840); }
      .asl-avatar { display:grid; place-items:center; width:30px; height:30px; flex:0 0 auto; border-radius:8px; color:#fff; background:var(--agent-accent,#666); font-size:9px; font-weight:750; }
      .asl-copy { flex:1 1 auto; min-width:0; }.asl-name { overflow:hidden; color:var(--text-primary,#e7e7eb); font-size:12px; font-weight:620; text-overflow:ellipsis; white-space:nowrap; }
      .asl-meta { display:flex; align-items:center; gap:4px; margin-top:2px; overflow:hidden; color:var(--text-secondary,#777781); font-size:9px; white-space:nowrap; }
      .asl-dot { width:6px; height:6px; flex:0 0 auto; border-radius:50%; background:#62626c; }.asl-dot.working { background:#f5b840; }.asl-dot.permission,.asl-dot.blocked { background:#ff5f56; }
      .asl-more { width:21px; height:21px; border:0; border-radius:5px; color:var(--text-secondary,#7c7c86); background:transparent; cursor:pointer; opacity:0; }
      .asl-agent:hover .asl-more,.asl-more:focus { opacity:1; }.asl-more:hover { color:var(--text-primary,#eee); background:rgba(255,255,255,.08); }
      .asl-threads { margin:0 5px 7px 39px; border-left:1px solid var(--border-color,#303038); }.asl-thread { padding:5px 9px; overflow:hidden; color:var(--text-secondary,#85858f); font-size:10px; text-overflow:ellipsis; white-space:nowrap; cursor:pointer; }
      .asl-thread.selected { color:var(--text-primary,#e4e4e9); }.asl-thread.new { color:var(--as-accent); }
      .as-head { display:flex; align-items:center; gap:12px; min-height:58px; padding:9px 22px;
        border-bottom:1px solid var(--border-color,var(--border,#303038)); background:var(--editor-surface,#18181d); }
      .as-avatar { display:grid; place-items:center; width:34px; height:34px; border-radius:9px; flex:0 0 auto;
        color:#fff; background:var(--profile-accent,#f5b840); font-size:13px; font-weight:750; }
      .as-title { min-width:0; flex:1; } .as-title-row { display:flex; gap:8px; align-items:baseline; }
      .as-title h1 { margin:0; color:var(--text-primary,#f3f3f6); font-size:15px; font-weight:680; }
      .as-handle,.as-subtle { color:var(--text-secondary,#92929d); font-size:12px; }
      .as-status { display:flex; align-items:center; gap:6px; margin-top:3px; color:var(--text-secondary,#92929d); font-size:11px; }
      .as-status-dot { width:6px; height:6px; border-radius:50%; background:#71717a; }
      .as-status-dot.working { background:#4da3ff; box-shadow:0 0 0 3px rgba(77,163,255,.12); }
      .as-status-dot.attention { background:#ff5f56; }
      .as-button { min-height:30px; padding:6px 12px; border:1px solid var(--border-color,var(--border,#383840));
        border-radius:7px; color:var(--text-primary,#e8e8ec); background:var(--bg-tertiary,#24242a); font:inherit;
        font-size:12px; cursor:pointer; }
      .as-button:hover { border-color:#5a5a64; background:#2b2b32; }
      .as-button.primary { border-color:var(--as-accent); background:var(--as-accent); color:#17140b; font-weight:700; }
      .as-button.danger { color:#ff8b84; border-color:rgba(255,95,86,.45); background:rgba(255,95,86,.08); }
      .as-button:disabled { opacity:.5; cursor:default; }
      .as-body { flex:1 1 auto; min-height:0; overflow-y:auto; }
      .as-thread { display:flex; flex-direction:column; min-height:100%; }
      .as-messages { width:min(760px,calc(100% - 44px)); margin:0 auto; padding:34px 0 128px; display:flex; flex-direction:column; gap:18px; }
      .as-empty { margin:auto; max-width:520px; padding:80px 24px; color:var(--text-secondary,#92929d); text-align:center; }
      .as-empty h2 { color:var(--text-primary,#ededf1); font-size:22px; margin:0 0 8px; }
      .as-message { position:relative; padding-left:28px; color:var(--text-primary,#e8e8ec); line-height:1.55; font-size:13px; }
      .as-message::before { position:absolute; left:0; top:1px; font-size:11px; font-weight:750; color:var(--text-secondary,#92929d); }
      .as-message.user::before { content:'YOU'; } .as-message.agent::before { content:'AG'; color:var(--as-accent); }
      .as-message-text { white-space:pre-wrap; overflow-wrap:anywhere; }
      .as-action { display:flex; gap:9px; align-items:center; padding:10px 12px; border:1px solid var(--border-color,#303038);
        border-radius:8px; background:var(--editor-surface,#19191e); color:var(--text-secondary,#9b9ba5); font-size:11px; }
      .as-action strong { color:var(--text-primary,#e8e8ec); font-weight:620; }
      .as-composer-wrap { position:absolute; left:0; right:0; bottom:0; padding:16px 22px 18px;
        background:linear-gradient(transparent,var(--bg-primary,#101014) 22%); }
      .as-composer { display:flex; gap:8px; width:min(780px,100%); margin:0 auto; padding:8px;
        border:1px solid var(--border-color,#373740); border-radius:11px; background:var(--editor-surface,#1b1b20);
        box-shadow:0 14px 38px rgba(0,0,0,.28); }
      .as-composer textarea { flex:1; min-height:42px; max-height:130px; resize:none; padding:10px 11px; border:0; outline:0;
        color:var(--text-primary,#eeeeF2); background:transparent; font:inherit; font-size:13px; }
      .as-composer textarea::placeholder,.as-input::placeholder { color:#73737e; }
      .as-send { width:36px; height:36px; align-self:flex-end; border:0; border-radius:8px; background:var(--as-accent);
        color:#17140b; font-size:18px; cursor:pointer; }
      .as-form-page { width:min(880px,calc(100% - 48px)); margin:0 auto; padding:34px 0 80px; }
      .as-form-intro h1 { margin:0; color:var(--text-primary,#f1f1f4); font-size:24px; }
      .as-form-intro p { margin:7px 0 28px; color:var(--text-secondary,#92929d); font-size:13px; }
      .as-grid { display:grid; grid-template-columns:minmax(0,1.5fr) minmax(230px,.8fr); gap:28px; align-items:start; }
      .as-card { padding:18px; border:1px solid var(--border-color,#34343c); border-radius:10px; background:var(--editor-surface,#18181d); }
      .as-field { display:flex; flex-direction:column; gap:6px; margin-bottom:16px; }
      .as-field label,.as-section-label { color:var(--text-secondary,#9b9ba5); font-size:10px; font-weight:700; letter-spacing:.08em; text-transform:uppercase; }
      .as-input { width:100%; padding:9px 10px; border:1px solid var(--border-color,#3a3a43); border-radius:7px;
        outline:0; color:var(--text-primary,#ededf1); background:var(--input-bg,#222228); font:inherit; font-size:13px; }
      textarea.as-input { min-height:94px; resize:vertical; line-height:1.45; }
      .as-input:focus { border-color:var(--as-accent); box-shadow:0 0 0 2px color-mix(in srgb,var(--as-accent) 18%,transparent); }
      .as-help { color:var(--text-secondary,#858590); font-size:11px; }
      .as-inline { display:grid; grid-template-columns:1fr 1fr; gap:12px; }
      .as-preview { position:sticky; top:20px; } .as-preview .as-avatar { width:44px; height:44px; border-radius:12px; }
      .as-preview-name { margin-top:14px; color:var(--text-primary,#f1f1f4); font-size:18px; font-weight:690; }
      .as-preview-tagline { min-height:40px; margin-top:12px; color:var(--text-secondary,#a2a2ab); line-height:1.45; font-size:12px; }
      .as-meta { display:grid; gap:8px; margin-top:20px; padding-top:16px; border-top:1px solid var(--border-color,#303038); }
      .as-meta-row { display:flex; justify-content:space-between; gap:10px; color:var(--text-secondary,#92929d); font-size:11px; }
      .as-meta-row span:last-child { color:var(--text-primary,#ddddE3); text-align:right; }
      .as-chips { display:flex; flex-wrap:wrap; gap:7px; }
      .as-chip { padding:5px 9px; border:1px solid var(--border-color,#3b3b44); border-radius:99px; color:var(--text-secondary,#a0a0aa);
        background:transparent; font:inherit; font-size:11px; cursor:pointer; }
      .as-chip.selected { border-color:var(--as-accent); color:var(--text-primary,#eeeef2); background:rgba(245,184,64,.10); }
      .as-actions { display:flex; justify-content:space-between; gap:10px; margin-top:22px; }
      .as-actions-right { display:flex; gap:8px; margin-left:auto; }
      .as-error { min-height:17px; margin-top:10px; color:#ff8b84; font-size:12px; }
      .as-menu { position:fixed; z-index:10020; min-width:150px; padding:4px; border:1px solid #3b3b43; border-radius:8px;
        background:#222228; box-shadow:0 10px 30px rgba(0,0,0,.45); }
      .as-menu button { display:block; width:100%; padding:7px 9px; border:0; border-radius:5px; color:#e7e7eb; background:transparent;
        text-align:left; font:inherit; font-size:12px; cursor:pointer; }
      .as-menu button:hover { background:rgba(255,255,255,.08); }
      @media (max-width:900px) { .asl { flex-basis:200px; width:200px; } }
      @media (max-width:760px) { .asl { display:none; }.as-grid { grid-template-columns:1fr; } .as-preview { position:static; } .as-inline { grid-template-columns:1fr; } }
    `;
    document.head.appendChild(style);
  }

  function initials(profile) {
    return String(profile && profile.display_name || '?').split(/\s+/).filter(Boolean).slice(0, 2)
      .map((part) => part[0].toUpperCase()).join('');
  }

  function sessionFor(profile, sessions) {
    return (sessions || []).filter((session) => session.agent_id === profile.handle)
      .sort((left, right) => Number(right.last_output_at_ms || right.started_at_ms || 0) - Number(left.last_output_at_ms || left.started_at_ms || 0))[0] || null;
  }

  function libraryMarkup(profiles, sessions, selectedHandle, selectedThreadId) {
    return `<aside class="asl" aria-label="Agent Library"><div class="asl-head"><span>Agents</span><button class="asl-add" data-library-new aria-label="New Agent" title="New Agent">+</button></div><div class="asl-list">${profiles.map((profile) => {
      const session = sessionFor(profile, sessions);
      const status = session && session.status || 'idle';
      const selected = profile.handle === selectedHandle;
      const threads = selected ? threadsFor(profile.handle) : [];
      return `<div class="asl-agent ${selected ? 'selected' : ''}" data-library-agent="${esc(profile.handle)}" style="--agent-accent:${esc(profile.accent_color || '#666')}"><span class="asl-avatar">${esc(initials(profile))}</span><span class="asl-copy"><span class="asl-name">${esc(profile.display_name)}</span><span class="asl-meta"><span class="asl-dot ${esc(status)}"></span><span>@${esc(profile.handle)}</span><span>· ${esc(status === 'idle' ? 'Ready' : status)}</span></span></span><button class="asl-more" data-library-more aria-label="Actions for ${esc(profile.display_name)}">•••</button></div>${selected ? `<div class="asl-threads">${threads.slice(0,5).map((thread) => `<div class="asl-thread ${thread.id === selectedThreadId ? 'selected' : ''}" data-library-thread="${esc(thread.id)}">${esc(thread.title || 'Untitled thread')}</div>`).join('')}<div class="asl-thread new" data-library-new-thread>+ New thread</div></div>` : ''}`;
    }).join('') || '<div class="as-help" style="padding:12px">No agents yet.</div>'}</div></aside>`;
  }

  function openLibraryMenu(event, profile) {
    event.preventDefault(); event.stopPropagation();
    document.querySelector('.as-menu[data-library-menu]')?.remove();
    const menu = document.createElement('div');
    menu.className = 'as-menu'; menu.dataset.libraryMenu = '1';
    const rect = event.currentTarget && event.currentTarget.getBoundingClientRect ? event.currentTarget.getBoundingClientRect() : null;
    menu.style.left = `${event.clientX || (rect && rect.right) || 20}px`; menu.style.top = `${event.clientY || (rect && rect.bottom) || 20}px`;
    menu.innerHTML = `<button data-edit>Edit / Settings</button><button data-duplicate>Duplicate</button><button data-assign>Assign project</button>${profile.handle === 'nautbot' ? '' : '<button data-delete style="color:#ff6b63">Delete…</button>'}`;
    document.body.appendChild(menu);
    const close = () => menu.remove();
    menu.querySelector('[data-edit]').onclick = () => { close(); window.xnautOpenAgentSettings(profile.handle); };
    menu.querySelector('[data-duplicate]').onclick = async () => {
      const newHandle = prompt(`Duplicate @${profile.handle} as:`, `${profile.handle}-copy`); if (!newHandle) return close();
      try { const duplicate = await invoke('agent_profile_duplicate', { handle:profile.handle, newHandle, displayName:`${profile.display_name} Copy` }); announceProfilesChanged(duplicate); close(); window.xnautOpenAgentSpace(duplicate.handle); }
      catch (error) { console.error('[agent-space] duplicate failed:', error); close(); }
    };
    menu.querySelector('[data-assign]').onclick = async () => {
      const project = prompt('Default project path:', profile.default_project || ''); if (project == null) return close();
      try { const saved = await invoke('agent_profile_update', { handle:profile.handle, profile:{ ...profile, default_project:project.trim() || null } }); announceProfilesChanged(saved); }
      catch (error) { console.error('[agent-space] assign project failed:', error); } close();
    };
    const deleteButton = menu.querySelector('[data-delete]');
    if (deleteButton) deleteButton.onclick = async () => {
      if (!confirm(`Delete ${profile.display_name} (@${profile.handle})?`)) return;
      try { await invoke('agent_profile_delete', { handle:profile.handle, rel:null }); announceProfilesChanged(); close(); window.xnautOpenAgentSpace(); }
      catch (error) { console.error('[agent-space] delete failed:', error); close(); }
    };
    setTimeout(() => document.addEventListener('mousedown', close, { once:true }), 0);
  }

  function wireLibrary(pane, profiles, selectedHandle) {
    const selected = profiles.find((profile) => profile.handle === selectedHandle);
    const add = pane.querySelector('[data-library-new]'); if (add) add.onclick = () => window.xnautOpenNewAgent();
    pane.querySelectorAll('[data-library-agent]').forEach((row) => {
      const profile = profiles.find((item) => item.handle === row.dataset.libraryAgent); if (!profile) return;
      row.onclick = () => window.xnautOpenAgentSpace(profile.handle);
      row.oncontextmenu = (event) => openLibraryMenu(event, profile);
      row.querySelector('[data-library-more]').onclick = (event) => openLibraryMenu(event, profile);
    });
    if (!selected) return;
    pane.querySelectorAll('[data-library-thread]').forEach((row) => { row.onclick = () => window.xnautOpenAgentSpace(selected.handle, row.dataset.libraryThread); });
    const fresh = pane.querySelector('[data-library-new-thread]'); if (fresh) fresh.onclick = () => window.xnautOpenAgentSpace(selected.handle, null, true);
  }

  function profilePayload(values, original) {
    const skills = Array.from(values.skills || []).map((skill) => `skill:${skill}`);
    const existingCapabilities = (original && original.capabilities || []).filter((value) => !String(value).startsWith('skill:'));
    return {
      handle: handleOf(values.handle),
      display_name: String(values.display_name || '').trim(),
      tagline: String(values.tagline || '').trim().slice(0, 72),
      purpose: String(values.purpose || '').trim(),
      runtime_id: String(values.runtime_id || '').trim(),
      provider: String(values.provider || 'global').trim(),
      model: String(values.model || '').trim(),
      reasoning_effort: String(values.reasoning_effort || '').trim(),
      execution: values.execution === 'sandbox' ? 'sandbox' : 'local',
      role: String(values.role || 'coding-agent').trim(),
      capabilities: Array.from(new Set(existingCapabilities.concat(skills))),
      notifications: values.notifications !== false,
      accent_color: String(values.accent_color || '#f5b840'),
      default_project: values.default_project || null,
      created_at: original && original.created_at || '',
      updated_at: original && original.updated_at || '',
    };
  }

  async function renderThread(pane, options) {
    const profiles = ((await invoke('agent_profile_list').catch(() => [])) || []).filter((item) => item.handle !== 'nautbot');
    const sessions = (await invoke('agent_sessions_list').catch(() => [])) || [];
    const profile = profiles.find((item) => item.handle === handleOf(options.handle)) || profiles[0];
    if (!profile) {
      pane.innerHTML = '<div class="as-empty"><h2>No agents yet.</h2><p>Create the first agent identity to start a conversation.</p><button class="as-button primary" data-new>Create agent</button></div>';
      pane.querySelector('[data-new]').onclick = () => window.xnautOpenNewAgent();
      return;
    }
    const recent = threadsFor(profile.handle);
    let thread = options.newThread ? null : (recent.find((item) => item.id === options.threadId) || recent[0]);
    if (!thread) thread = newThread(profile.handle, 'New thread');
    const session = sessionFor(profile, sessions);
    let sessionId = thread.session_id || session && session.session_id || null;
    const status = session && session.status || 'idle';
    pane.style.setProperty('--profile-accent', profile.accent_color || '#f5b840');
    pane.style.setProperty('--as-accent', profile.accent_color || '#f5b840');
    pane.innerHTML = `${libraryMarkup(profiles, sessions, profile.handle, thread.id)}<div class="as-stage">
      <header class="as-head">
        <div class="as-avatar">${esc(initials(profile))}</div>
        <div class="as-title"><div class="as-title-row"><h1>${esc(profile.display_name)}</h1><span class="as-handle">@${esc(profile.handle)}</span></div>
          <div class="as-status"><span class="as-status-dot ${esc(status)}"></span><span>${esc(status === 'idle' ? 'Ready' : status)}</span>${session ? '<span>· terminal attached</span>' : ''}</div></div>
        <button class="as-button" data-terminal aria-label="Open terminal" title="Open terminal" ${sessionId ? '' : 'hidden'}>&gt;_</button>
        <button class="as-button" data-settings>Settings</button>
      </header>
      <div class="as-body as-thread">
        <div class="as-messages" data-messages></div>
        <div class="as-composer-wrap"><div class="as-composer">
          <textarea data-compose rows="1" placeholder="Message @${esc(profile.handle)}…" aria-label="Message @${esc(profile.handle)}"></textarea>
          <button class="as-send" data-send aria-label="Send message">↑</button>
        </div></div>
      </div></div>`;

    const messages = pane.querySelector('[data-messages]');
    const paintMessages = () => {
      const items = Array.isArray(thread.messages) ? thread.messages : [];
      if (!items.length) {
        messages.innerHTML = `<div class="as-empty"><h2>Talk to ${esc(profile.display_name)}.</h2><p>${esc(profile.tagline || profile.purpose)}</p></div>`;
        return;
      }
      messages.innerHTML = items.map((message) => message.kind === 'action'
        ? `<div class="as-action"><strong>${esc(message.label || 'Started')}</strong><span>${esc(message.detail || '')}</span><span style="margin-left:auto">${esc(new Date(message.at).toLocaleTimeString([], { hour:'2-digit', minute:'2-digit' }))}</span>${message.session_id ? `<button class="as-button" data-open-session="${esc(message.session_id)}">Terminal</button>` : ''}</div>`
        : `<div class="as-message ${message.role === 'user' ? 'user' : 'agent'}" data-message-id="${esc(message.id)}"><div class="as-message-text">${esc(message.text)}</div></div>`
      ).join('');
      messages.querySelectorAll('[data-open-session]').forEach((button) => {
        button.onclick = () => window.xnautOpenAgentSession && window.xnautOpenAgentSession(button.dataset.openSession, profile.display_name);
      });
      messages.scrollTop = messages.scrollHeight;
    };
    paintMessages();

    const composer = pane.querySelector('[data-compose]');
    const send = pane.querySelector('[data-send]');
    const terminalButton = pane.querySelector('[data-terminal]');
    const showTerminal = (nextSessionId) => {
      sessionId = nextSessionId || sessionId;
      if (!terminalButton || !sessionId) return;
      terminalButton.hidden = false;
      terminalButton.dataset.sessionId = sessionId;
    };
    showTerminal(sessionId);

    const turnCleanups = [];
    pane._agentSpaceCleanup = () => {
      turnCleanups.splice(0).forEach((cleanup) => { try { cleanup(); } catch (_) {} });
    };
    const updateAgentMessage = (messageId, text) => {
      if (!text) return;
      thread = updateThread(profile.handle, thread.id, (next) => {
        const message = next.messages.find((item) => item.id === messageId);
        if (message) message.text = text;
        return next;
      });
      saveSharedMessage({ id:messageId, role:'assistant', agent:profile.handle, text, at:nowIso() });
      paintMessages();
    };
    const mirrorSessionTurn = async (nextSessionId, messageId, prompt) => {
      const mirror = terminalMirror(prompt, (text) => updateAgentMessage(messageId, text));
      let stopped = false;
      const subscriptions = [];
      let settleTimer = null;
      const stop = () => {
        if (stopped) return;
        stopped = true;
        if (settleTimer) clearTimeout(settleTimer);
        subscriptions.forEach((subscription) => Promise.resolve(subscription).then((unlisten) => {
          try { unlisten(); } catch (_) {}
        }).catch(() => {}));
        mirror.dispose();
        send.disabled = false;
      };
      turnCleanups.push(stop);
      subscriptions.push(listen(`terminal-output:${nextSessionId}`, (event) => {
        if (!stopped && event && event.payload && event.payload.data) mirror.push(decodeTerminalBytes(event.payload.data));
      }));
      subscriptions.push(listen(`terminal-closed:${nextSessionId}`, stop));
      subscriptions.push(listen('agent-status-changed', (event) => {
        const state = event && event.payload;
        if (!state || state.session_id !== nextSessionId) return;
        if (['idle', 'waiting', 'done', 'blocked', 'permission', 'interrupted'].includes(state.status)) stop();
      }));
      // The PTY can emit before agent_profile_launch returns its id. Replay the
      // bounded backend tail after listeners are attached, then follow live.
      const snapshot = await invoke('terminal_output_snapshot', { sessionId:nextSessionId }).catch(() => '');
      if (!stopped && snapshot) mirror.push(decodeTerminalBytes(snapshot));
      const current = await invoke('agent_sessions_list').catch(() => []);
      const currentState = (current || []).find((item) => item.session_id === nextSessionId);
      if (currentState && ['idle', 'waiting', 'done', 'blocked', 'permission', 'interrupted'].includes(currentState.status)) stop();
      else settleTimer = setTimeout(stop, 10 * 60 * 1000);
    };

    const submit = async () => {
      const text = composer.value.trim();
      if (!text || send.disabled) return;
      send.disabled = true;
      const handoff = portableHandoff(profile, thread);
      const runtimePrompt = handoff
        ? `${handoff}\n\nLATEST USER REQUEST\n${text}`
        : text;
      const userMessageId = `m-${Date.now()}`;
      const firstUser = !(thread.messages || []).some((message) => message.role === 'user');
      thread = updateThread(profile.handle, thread.id, (next) => {
        next.title = firstUser ? text.replace(/\s+/g, ' ').slice(0, 48) : next.title;
        next.messages.push({ id:userMessageId, role:'user', text, at:nowIso() });
        return next;
      });
      saveSharedMessage({ id:userMessageId, role:'user', text, at:nowIso() });
      composer.value = '';
      paintMessages();
      let mirroring = false;
      try {
        const liveSessions = await invoke('agent_sessions_list').catch(() => []);
        let active = (liveSessions || []).find((item) => item.session_id === sessionId
          && !['done', 'interrupted'].includes(item.status));
        if (!active) {
          const home = await invoke('get_home_directory');
          const response = await invoke('agent_profile_launch', { req: {
            handle: profile.handle,
            worktree_path: profile.default_project || home,
            prompt: runtimePrompt,
            cols: null,
            rows: null,
          } });
          sessionId = response.session_id;
          active = { session_id:sessionId, status:'working' };
          announceProfilesChanged(profile);
        }
        showTerminal(active.session_id);
        const messageId = `a-${Date.now()}`;
        thread = updateThread(profile.handle, thread.id, (next) => {
          next.session_id = active.session_id;
          next.messages.push({ id:messageId, role:'agent', text:'Working…', session_id:active.session_id, at:nowIso() });
          return next;
        });
        paintMessages();
        await mirrorSessionTurn(active.session_id, messageId, runtimePrompt);
        mirroring = true;
        // A continuing turn is written into the already-attached runtime. A
        // fresh launch received the prompt in agent_profile_launch above.
        if (active.session_id === sessionId && (liveSessions || []).some((item) => item.session_id === sessionId)) {
          const payload = `\x1b[200~${runtimePrompt}\x1b[201~\r`;
          await invoke('write_to_terminal', { sessionId, data:encodeTerminalInput(payload) });
        }
      } catch (error) {
        thread = updateThread(profile.handle, thread.id, (next) => {
          next.messages.push({ id: `e-${Date.now()}`, role: 'agent', text: `Could not start: ${String(error)}`, at: nowIso() });
          return next;
        });
        paintMessages();
      } finally { if (!mirroring) send.disabled = false; }
    };
    send.onclick = submit;
    composer.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(); }
    });
    pane.querySelector('[data-settings]').onclick = () => window.xnautOpenAgentSettings(profile.handle);
    if (terminalButton) terminalButton.onclick = () => {
      const target = terminalButton.dataset.sessionId || sessionId;
      if (target) window.xnautOpenAgentSession(target, profile.display_name);
    };
    messages.addEventListener('contextmenu', (event) => {
      const message = event.target.closest('[data-message-id]');
      if (!message) return;
      event.preventDefault();
      const record = (thread.messages || []).find((item) => item.id === message.dataset.messageId);
      const menu = document.createElement('div');
      menu.className = 'as-menu'; menu.style.left = `${event.clientX}px`; menu.style.top = `${event.clientY}px`;
      menu.innerHTML = '<button data-copy>Copy</button><button data-branch>Start a thread</button>';
      document.body.appendChild(menu);
      const close = () => menu.remove();
      menu.querySelector('[data-copy]').onclick = () => { navigator.clipboard && navigator.clipboard.writeText(record.text); close(); };
      menu.querySelector('[data-branch]').onclick = () => {
        const branch = newThread(profile.handle, String(record.text || '').slice(0, 48));
        branch.messages.push({ ...record, id: `m-${Date.now()}` }); writeThread(profile.handle, branch);
        close(); window.xnautOpenAgentSpace(profile.handle, branch.id);
      };
      setTimeout(() => document.addEventListener('mousedown', close, { once:true }), 0);
    });
    wireLibrary(pane, profiles, profile.handle);
  }

  async function renderProfileForm(pane, options) {
    const editing = options.mode === 'settings';
    const [profiles, runtimes, availableSkills, sessions] = await Promise.all([
      invoke('agent_profile_list').catch(() => []),
      invoke('agent_list').catch(() => []),
      invoke('skill_list').catch(() => []),
      invoke('agent_sessions_list').catch(() => []),
    ]);
    const original = editing ? (profiles || []).find((item) => item.handle === handleOf(options.handle)) : null;
    if (editing && !original) { pane.innerHTML = '<div class="as-empty"><h2>Agent not found.</h2></div>'; return; }
    const profile = original || profilePayload({
      handle:'', display_name:'', tagline:'', purpose:'', runtime_id:(runtimes[0] && runtimes[0].id) || '',
      provider:'global', model:'', reasoning_effort:'', execution:'local', role:'coding-agent', skills:[], notifications:true,
    });
    const selectedSkills = new Set((profile.capabilities || []).filter((item) => String(item).startsWith('skill:')).map((item) => String(item).slice(6)));
    const modelCatalog = window.xnautModelCatalog ? window.xnautModelCatalog.all() : [];
    const modelOptions = modelCatalog.slice();
    if (profile.model && !modelOptions.some((item) => item.id === profile.model && item.provider === profile.provider)) {
      modelOptions.unshift({ id:profile.model, name:profile.model, provider:profile.provider });
    }
    const providers = Array.from(new Set(['global', profile.provider, ...modelCatalog.map((item) => item.provider)].filter(Boolean)));
    const libraryProfiles = (profiles || []).filter((item) => item.handle !== 'nautbot');
    pane.innerHTML = `${libraryMarkup(libraryProfiles, sessions || [], original && original.handle, null)}<div class="as-stage"><div class="as-body"><form class="as-form-page" data-form>
      <div class="as-form-intro"><h1>${editing ? 'Agent settings.' : 'Create a new agent.'}</h1>
        <p>${editing ? 'Identity, runtime, and permissions for this agent.' : 'Give the agent a durable identity, then choose how it runs.'}</p></div>
      <div class="as-grid"><div class="as-card">
        <div class="as-inline"><label class="as-field"><span>Name</span><input class="as-input" name="display_name" required value="${esc(profile.display_name)}" placeholder="Builder"></label>
          <label class="as-field"><span>@Handle</span><input class="as-input" name="handle" required value="${esc(profile.handle)}" ${profile.handle === 'nautbot' ? 'readonly' : ''} placeholder="builder"><small class="as-help">Unique · letters, numbers, - or _</small></label></div>
        <label class="as-field"><span>Tagline</span><input class="as-input" name="tagline" maxlength="72" value="${esc(profile.tagline)}" placeholder="Turns clear product intent into working software."></label>
        <label class="as-field"><span>Purpose</span><textarea class="as-input" name="purpose" required placeholder="What should this agent own?">${esc(profile.purpose)}</textarea></label>
        <div class="as-inline"><label class="as-field"><span>Runtime</span><select class="as-input" name="runtime_id">${(runtimes || []).map((runtime) => `<option value="${esc(runtime.id)}" ${runtime.id === profile.runtime_id ? 'selected' : ''} ${runtime.available === false && runtime.id !== profile.runtime_id ? 'disabled' : ''}>${esc(runtime.label)}${runtime.available === false ? ' · unavailable' : ''}</option>`).join('')}</select></label>
          <label class="as-field"><span>Compute</span><select class="as-input" name="execution"><option value="local" ${profile.execution !== 'sandbox' ? 'selected' : ''}>Local</option><option value="sandbox" ${profile.execution === 'sandbox' ? 'selected' : ''}>Sandbox</option></select></label></div>
        <div class="as-inline"><label class="as-field"><span>Provider</span><select class="as-input" name="provider">${providers.map((provider) => `<option value="${esc(provider)}" ${provider === profile.provider ? 'selected' : ''}>${esc(provider)}</option>`).join('')}</select></label>
          <label class="as-field"><span>Model</span><select class="as-input" name="model"><option value="">Runtime default</option>${modelOptions.map((model) => `<option data-provider="${esc(model.provider)}" value="${esc(model.id)}" ${model.id === profile.model && model.provider === profile.provider ? 'selected' : ''}>${esc(model.name || model.id)}</option>`).join('')}</select></label></div>
        <label class="as-field"><span>Reasoning effort</span><select class="as-input" name="reasoning_effort"><option value="" ${!profile.reasoning_effort ? 'selected' : ''}>Model default</option>${['low','medium','high','xhigh'].map((effort) => `<option value="${effort}" ${profile.reasoning_effort === effort ? 'selected' : ''}>${effort}</option>`).join('')}</select></label>
        <label class="as-field"><span>Role</span><input class="as-input" name="role" required value="${esc(profile.role)}"></label>
        <div class="as-field"><span class="as-section-label">Skills</span><div class="as-chips" data-skills>${(availableSkills || []).slice(0, 24).map((skill) => `<button type="button" class="as-chip ${selectedSkills.has(skill) ? 'selected' : ''}" data-skill="${esc(skill)}">${esc(skill)}</button>`).join('') || '<span class="as-help">No installed skills found.</span>'}</div></div>
        <label class="as-field"><span>Accent</span><input class="as-input" name="accent_color" type="color" value="${esc(profile.accent_color || '#f5b840')}"></label>
        <label class="as-field" style="flex-direction:row;align-items:center"><input name="notifications" type="checkbox" ${profile.notifications !== false ? 'checked' : ''}><span>Notify me when this agent needs attention</span></label>
        <div class="as-error" data-error></div>
        <div class="as-actions">${editing && profile.handle !== 'nautbot' ? '<button type="button" class="as-button danger" data-delete>Delete agent</button>' : '<span></span>'}<div class="as-actions-right"><button type="button" class="as-button" data-cancel>Cancel</button><button class="as-button primary" type="submit">${editing ? 'Save changes' : 'Create agent'}</button></div></div>
      </div><aside class="as-card as-preview"><div class="as-avatar" data-preview-avatar>${esc(initials(profile) || 'AG')}</div><div class="as-preview-name" data-preview-name>${esc(profile.display_name || 'New agent')}</div><div class="as-handle" data-preview-handle>@${esc(profile.handle || 'handle')}</div><div class="as-preview-tagline" data-preview-tagline>${esc(profile.tagline || 'A short line explaining when to call this agent.')}</div><div class="as-meta"><div class="as-meta-row"><span>Runtime</span><span data-preview-runtime>${esc(profile.runtime_id || 'Choose one')}</span></div><div class="as-meta-row"><span>Compute</span><span data-preview-execution>${esc(profile.execution || 'local')}</span></div><div class="as-meta-row"><span>Model</span><span data-preview-model>${esc(profile.model || 'Runtime default')}</span></div></div></aside></div>
    </form></div></div>`;

    const form = pane.querySelector('[data-form]');
    const skills = new Set(selectedSkills);
    const refreshPreview = () => {
      const values = new FormData(form);
      const displayName = values.get('display_name') || 'New agent';
      pane.querySelector('[data-preview-name]').textContent = displayName;
      pane.querySelector('[data-preview-handle]').textContent = `@${handleOf(values.get('handle')) || 'handle'}`;
      pane.querySelector('[data-preview-tagline]').textContent = values.get('tagline') || 'A short line explaining when to call this agent.';
      pane.querySelector('[data-preview-runtime]').textContent = values.get('runtime_id') || 'Choose one';
      pane.querySelector('[data-preview-execution]').textContent = values.get('execution') || 'local';
      pane.querySelector('[data-preview-model]').textContent = values.get('model') || 'Runtime default';
      pane.querySelector('[data-preview-avatar]').textContent = String(displayName).split(/\s+/).filter(Boolean).slice(0,2).map((part) => part[0].toUpperCase()).join('') || 'AG';
    };
    form.addEventListener('input', refreshPreview);
    pane.querySelectorAll('[data-skill]').forEach((button) => button.onclick = () => {
      const skill = button.dataset.skill;
      if (skills.has(skill)) skills.delete(skill); else skills.add(skill);
      button.classList.toggle('selected', skills.has(skill));
    });
    pane.querySelector('[data-cancel]').onclick = () => editing ? window.xnautOpenAgentSpace(profile.handle) : window.xnautOpenAgentSpace();
    form.onsubmit = async (event) => {
      event.preventDefault();
      const values = Object.fromEntries(new FormData(form).entries());
      values.notifications = form.elements.notifications.checked;
      values.skills = skills;
      const payload = profilePayload(values, original);
      const errorEl = pane.querySelector('[data-error]'); errorEl.textContent = '';
      if (!payload.handle || !payload.display_name || !payload.purpose || !payload.runtime_id) { errorEl.textContent = 'Name, handle, purpose, and runtime are required.'; return; }
      if ((profiles || []).some((item) => item.handle === payload.handle && (!original || item.handle !== original.handle))) { errorEl.textContent = `@${payload.handle} is already in use.`; return; }
      try {
        const saved = editing
          ? await invoke('agent_profile_update', { handle: original.handle, profile: payload })
          : await invoke('agent_profile_create', { profile: payload });
        announceProfilesChanged(saved);
        window.xnautOpenAgentSpace(saved.handle);
      } catch (error) { errorEl.textContent = String(error); }
    };
    const deleteButton = pane.querySelector('[data-delete]');
    if (deleteButton) deleteButton.onclick = async () => {
      if (!confirm(`Delete ${profile.display_name} (@${profile.handle})? Conversation history remains local.`)) return;
      try { await invoke('agent_profile_delete', { handle: profile.handle, rel: null }); announceProfilesChanged(); window.xnautOpenAgentSpace(); }
      catch (error) { pane.querySelector('[data-error]').textContent = String(error); }
    };
    wireLibrary(pane, libraryProfiles, original && original.handle);
  }

  async function createAgentSpacePanel(tabId, parent, options) {
    ensureStyles();
    const label = `agent-space-${tabId}`;
    const pane = document.createElement('section');
    pane.className = 'agent-space';
    pane.dataset.agentSpace = '1';
    pane.style.flex = '1';
    parent.appendChild(pane);
    let current = { ...(options || {}) };
    const render = async () => {
      if (typeof pane._agentSpaceCleanup === 'function') pane._agentSpaceCleanup();
      pane.innerHTML = '<div class="as-empty">Loading agent space…</div>';
      try {
        if (current.mode === 'new' || current.mode === 'settings') await renderProfileForm(pane, current);
        else await renderThread(pane, current);
      } catch (error) { pane.innerHTML = `<div class="as-empty"><h2>Agent Space could not open.</h2><p>${esc(error)}</p></div>`; }
    };
    const entry = { kind:'agent-space', label, pane, updateOptions(next) { current = { ...(next || {}) }; render(); }, dispose() {
      if (typeof pane._agentSpaceCleanup === 'function') pane._agentSpaceCleanup();
    } };
    panes.set(label, entry);
    await render();
    return entry;
  }

  window.xnautAgentThreadsFor = threadsFor;
  window.xnautCreateAgentSpacePanel = createAgentSpacePanel;
  window.xnautOpenAgentSpace = (handle, threadId, newThreadRequested) => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab('Agent Space', 'xnautCreateAgentSpacePanel', { mode:'thread', handle:handleOf(handle), threadId:threadId || null, newThread:!!newThreadRequested });
  };
  window.xnautOpenNewAgent = () => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab('New Agent', 'xnautCreateAgentSpacePanel', { mode:'new' });
  };
  window.xnautOpenAgentSettings = (handle) => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab('Agent Settings', 'xnautCreateAgentSpacePanel', { mode:'settings', handle:handleOf(handle) });
  };
  // Compatibility for existing links that opened the retired Agent Father UI.
  window.xnautOpenAgentFather = () => window.xnautOpenNewAgent();
})();
