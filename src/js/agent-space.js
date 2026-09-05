// Agent Space — identity-first agent library, bounded conversations, creation,
// and settings. Coding CLIs run in background PTYs; only their structured
// conversation events are rendered here. Raw terminal output is opt-in.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const listen = (...args) => window.__TAURI__.event.listen(...args);
  const THREADS_KEY = 'xnaut-agent-threads:v1';

  // The Librarian used to live in the right pane with its own conversation
  // store. It is an agent now, so its history comes with it — a feature that
  // moves and leaves the old conversations stranded has taken something away.
  // Runs once; the old keys are left untouched so nothing is destroyed if this
  // turns out to be wrong.
  const LIBRARIAN_MIGRATED = 'xnaut-librarian-threads-migrated';
  function migrateLibrarianConversations() {
    try {
      if (localStorage.getItem(LIBRARIAN_MIGRATED) === '1') return;
      const vault = localStorage.getItem('xnaut-vault:last') || 'work';
      const archived = JSON.parse(localStorage.getItem('xnaut-vault-conversations:' + vault) || '[]');
      const current = JSON.parse(localStorage.getItem('xnaut-chat-history:vault:' + vault) || '[]');
      const conversations = (Array.isArray(archived) ? archived : []).slice();
      if (Array.isArray(current) && current.length) {
        conversations.push({ title: 'Current', messages: current, at: new Date().toISOString() });
      }
      if (!conversations.length) { localStorage.setItem(LIBRARIAN_MIGRATED, '1'); return; }

      const all = JSON.parse(localStorage.getItem(THREADS_KEY) || '{}');
      const existing = Array.isArray(all.librarian) ? all.librarian : [];
      const brought = conversations.map((conversation, index) => {
        const messages = (conversation.messages || conversation || [])
          .filter((message) => message && message.content)
          .map((message, position) => ({
            id: `mig-${index}-${position}`,
            role: message.role === 'user' ? 'user' : 'agent',
            text: String(message.content),
            at: conversation.at || new Date().toISOString(),
          }));
        const title = conversation.title
          || (messages.find((message) => message.role === 'user') || {}).text
          || 'Librarian conversation';
        return {
          id: `librarian-migrated-${index}`,
          title: String(title).replace(/\s+/g, ' ').slice(0, 48),
          created_at: conversation.at || new Date().toISOString(),
          updated_at: conversation.at || new Date().toISOString(),
          messages,
        };
      }).filter((thread) => thread.messages.length);

      all.librarian = existing.concat(brought.filter((thread) =>
        !existing.some((kept) => kept.id === thread.id)));
      localStorage.setItem(THREADS_KEY, JSON.stringify(all));
      localStorage.setItem(LIBRARIAN_MIGRATED, '1');
      console.log(`[agent-space] brought ${brought.length} Librarian conversations across`);
    } catch (error) {
      console.warn('[agent-space] Librarian migration skipped:', error);
    }
  }
  migrateLibrarianConversations();
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
    // This thread's own turns, last: they are the most relevant context, and
    // the tail is what survives the 30k slice below. Without them a harness
    // switch handed the new CLI everything except the conversation it was
    // meant to continue. XNAUT-150.
    const threadText = ((thread && thread.messages) || []).filter((message) => message.role)
      .slice(-24)
      .map((message) => `${message.role === 'user' ? 'User' : `@${profile.handle}`}: ${String(message.text || '').trim()}`)
      .filter((line) => !/:\s*$/.test(line) && !/:\s*(Working…|No answer came back\.)$/.test(line))
      .join('\n\n');
    const conversation = [controlText, specialistText, threadText].filter(Boolean).join('\n\n');
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

  function decodeTerminalBytes(encoded, decoder, stream = false) {
    const binary = atob(String(encoded || ''));
    const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
    return (decoder || new TextDecoder('utf-8')).decode(bytes, { stream });
  }

  function structuredTurn(onText, onConversationId) {
    let pending = '';
    let streamed = '';
    const responses = [];
    const seen = new Set();
    const publish = (value) => {
      const text = String(value || '').trim();
      if (!text || responses.includes(text)) return;
      responses.push(text);
      onText(responses.join('\n\n').slice(-24000));
    };
    const consume = (rawLine) => {
      const line = String(rawLine || '').replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '')
        .replace(/\x1b\[[0-?]*[ -\/]*[@-~]/g, '').trim();
      if (!line || seen.has(line)) return;
      let event;
      try { event = JSON.parse(line); } catch (_) { return; }
      seen.add(line);
      const id = event.session_id || event.thread_id;
      if (id) onConversationId(String(id));

      // Claude stream-json: assistant.message.content[] and result.result.
      if (event.type === 'assistant' && event.message && Array.isArray(event.message.content)) {
        publish(event.message.content.filter((item) => item && item.type === 'text').map((item) => item.text).join(''));
      } else if (event.type === 'result' && !responses.length && !streamed.trim() && typeof event.result === 'string') {
        publish(event.result);
      }

      // Gemini stream-json: assistant message chunks carry plain content.
      if (event.type === 'message' && event.role === 'assistant') {
        if (event.delta) {
          streamed += String(event.content || '');
          if (streamed.trim()) onText(responses.concat(streamed.trim()).join('\n\n').slice(-24000));
        } else {
          publish(event.content);
        }
      }

      // Codex exec --json: thread.started and completed agent_message items.
      if (event.type === 'item.completed' && event.item && event.item.type === 'agent_message') {
        publish(event.item.text);
      }
      if ((event.type === 'error' || event.type === 'turn.failed') && !responses.length) {
        const detail = event.message || event.error && event.error.message || 'The agent could not complete this turn.';
        publish(detail);
      }
    };
    return {
      push(value) {
        pending += String(value || '').replace(/\r/g, '');
        const lines = pending.split('\n');
        pending = lines.pop() || '';
        lines.forEach(consume);
      },
      flush() { if (pending.trim()) consume(pending); pending = ''; },
      hasResponse() { return responses.length > 0 || !!streamed.trim(); },
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

  function allThreadsFor(handle) {
    const all = loadThreads();
    return Array.isArray(all[handle]) ? all[handle] : [];
  }

  function threadsFor(handle) {
    return allThreadsFor(handle).filter((thread) => !thread.archived_at);
  }

  function archivedThreadsFor(handle) {
    return allThreadsFor(handle).filter((thread) => !!thread.archived_at);
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
    const found = allThreadsFor(handle).find((item) => item.id === id) || newThread(handle, 'New thread');
    const next = updater({ ...found, messages: Array.isArray(found.messages) ? found.messages.slice() : [] }) || found;
    next.updated_at = nowIso();
    next.messages = next.messages.slice(-MAX_MESSAGES);
    return writeThread(handle, next);
  }

  function deleteThread(handle, id) {
    const all = loadThreads();
    all[handle] = (Array.isArray(all[handle]) ? all[handle] : []).filter((thread) => thread.id !== id);
    saveThreads(all);
  }

  // In-app confirm/prompt now live in dialogs.js, which loads before app.js
  // and also replaces the native alert(). This module was where they started,
  // so it keeps the names its call sites already use.
  const promptDialog = (message, value, actionLabel) => window.xnautPromptDialog(message, value, actionLabel);
  const confirmDialog = (message, actionLabel) => window.xnautConfirmDialog(message, actionLabel);

  function deleteArchivedThreads(handle) {
    const all = loadThreads();
    const current = Array.isArray(all[handle]) ? all[handle] : [];
    const removed = current.filter((thread) => !!thread.archived_at).length;
    all[handle] = current.filter((thread) => !thread.archived_at);
    saveThreads(all);
    return removed;
  }

  // NautBot is the master and orchestrator: always first in the list, never
  // deletable. Everything else keeps its own order.
  // Threads stay collapsed until asked for: an agent with a dozen threads
  // otherwise buries every other agent in the list.
  function threadsOpen(handle) {
    try { return localStorage.getItem('xnaut-as-threads-open:' + handle) === '1'; } catch (_) { return false; }
  }
  function toggleThreads(handle) {
    try {
      if (threadsOpen(handle)) localStorage.removeItem('xnaut-as-threads-open:' + handle);
      else localStorage.setItem('xnaut-as-threads-open:' + handle, '1');
    } catch (_) {}
  }

  function pinNautbotFirst(profiles) {
    const list = Array.isArray(profiles) ? profiles.slice() : [];
    const index = list.findIndex((item) => item && item.handle === 'nautbot');
    if (index > 0) list.unshift(list.splice(index, 1)[0]);
    return list;
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
      /* One column until there is something to show beside the conversation,
         then two — the same class swap Cockpit uses for its artifact pane. */
      .as-stage { position:relative; display:grid; grid-template-columns:minmax(0,1fr); flex:1 1 auto;
        min-width:0; min-height:0; }
      .as-stage.split { grid-template-columns:minmax(380px,1fr) minmax(420px,1.05fr); }
      .as-stage.split.split-full { grid-template-columns:0 minmax(0,1fr); }
      .as-stage.split.split-full .as-conv { overflow:hidden; }
      .as-conv { display:flex; flex-direction:column; min-width:0; min-height:0; }
      .as-split { display:flex; min-width:0; min-height:0; border-left:1px solid var(--border-color,#26262c); }
      .as-split > * { flex:1 1 auto; min-width:0; }
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
      /* A profile the store merge had something to say about. Brand yellow, the
         same colour every other "read this" marker uses; the text is the row's
         title attribute. */
      .asl-note { color:#f5b840; font-weight:700; }
      .asl-meta { display:flex; align-items:center; gap:4px; margin-top:2px; overflow:hidden; color:var(--text-secondary,#777781); font-size:9px; white-space:nowrap; }
      .asl-dot { width:6px; height:6px; flex:0 0 auto; border-radius:50%; background:#62626c; }.asl-dot.working { background:#f5b840; }.asl-dot.permission,.asl-dot.blocked { background:#ff5f56; }
      .asl-more { width:21px; height:21px; border:0; border-radius:5px; color:var(--text-secondary,#7c7c86); background:transparent; cursor:pointer; opacity:0; }
      .asl-agent:hover .asl-more,.asl-more:focus { opacity:1; }.asl-more:hover { color:var(--text-primary,#eee); background:rgba(255,255,255,.08); }
      .asl-threads { margin:0 5px 7px 39px; border-left:1px solid var(--border-color,#303038); }.asl-thread { display:flex; align-items:center; gap:5px; padding:5px 9px; overflow:hidden; color:var(--text-secondary,#85858f); font-size:10px; white-space:nowrap; cursor:pointer; }
      .asl-thread-label { min-width:0; flex:1; overflow:hidden; text-overflow:ellipsis; }.asl-thread.selected { color:var(--text-primary,#e4e4e9); }.asl-thread.new { color:var(--as-accent); }.asl-thread.archived { opacity:.62; }
      .asl-thread-more { width:20px; height:20px; padding:0; border:0; border-radius:4px; color:inherit; background:transparent; cursor:pointer; opacity:0; }
      .asl-thread:hover .asl-thread-more,.asl-thread-more:focus { opacity:1; }.asl-thread-more:hover { background:rgba(255,255,255,.08); }
      .asl-caret { border:0; background:transparent; color:var(--text-secondary,#8a8a94); font-size:9px; cursor:pointer; padding:0 4px; }
      .asl-archive-head { display:flex; align-items:center; justify-content:space-between; padding:7px 9px 3px; color:var(--text-secondary,#666670); font-size:8px; font-weight:700; letter-spacing:.08em; text-transform:uppercase; }
      .asl-archive-clear { border:none; background:transparent; color:#ff6b63; font:inherit; font-size:8px; letter-spacing:.08em; text-transform:uppercase; cursor:pointer; padding:0; opacity:.8; }
      .asl-archive-clear:hover { opacity:1; }
      .as-head { display:flex; align-items:center; gap:12px; min-height:58px; padding:9px 22px;
        border-bottom:1px solid var(--border-color,var(--border,#303038)); background:var(--editor-surface,#18181d); }
      .as-avatar { display:grid; place-items:center; width:34px; height:34px; border-radius:9px; flex:0 0 auto;
        color:#fff; background:var(--profile-accent,#f5b840); font-size:13px; font-weight:750; }
      .as-title { min-width:0; flex:1; } .as-title-row { display:flex; gap:8px; align-items:baseline; }
      .as-title h1 { margin:0; color:var(--text-primary,#f3f3f6); font-size:15px; font-weight:680; }
      .as-handle,.as-subtle { color:var(--text-secondary,#92929d); font-size:12px; }
      .as-status { display:flex; align-items:center; gap:6px; margin-top:3px; color:var(--text-secondary,#92929d); font-size:11px; }
      .as-status-dot { width:6px; height:6px; border-radius:50%; background:#71717a; }
      .as-harness { flex:0 0 auto; padding:5px 7px; border:1px solid var(--border-color,#303038); border-radius:7px;
        color:var(--text-secondary,#92929d); background:var(--editor-surface,#18181d); font-size:11px; cursor:pointer; }
      .as-harness:hover { color:var(--text-primary,#e4e4e9); }
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
      .as-thread { display:flex; flex-direction:column; }
      .as-messages { width:min(760px,calc(100% - 44px)); margin:0 auto; padding:34px 0 20px; display:flex; flex-direction:column; gap:18px; }
      .as-empty { margin:auto; max-width:520px; padding:80px 24px; color:var(--text-secondary,#92929d); text-align:center; }
      .as-empty h2 { color:var(--text-primary,#ededf1); font-size:22px; margin:0 0 8px; }
      .as-message { position:relative; padding-left:28px; color:var(--text-primary,#e8e8ec); line-height:1.55; font-size:13px; }
      .as-message::before { position:absolute; left:0; top:1px; font-size:11px; font-weight:750; color:var(--text-secondary,#92929d); }
      .as-message.user::before { content:'YOU'; } .as-message.agent::before { content:'AG'; color:var(--as-accent); }
      .as-message-text { white-space:pre-wrap; overflow-wrap:anywhere; }
      .as-receipt { margin-top:5px; font-family:ui-monospace,Menlo,monospace; font-size:10px; letter-spacing:.02em; color:var(--text-dim,#8b919c); }
      .as-receipt.none { color:#e0a33a; }
      .as-chip-icon { display:inline-grid; place-items:center; width:15px; height:15px; margin-right:5px; vertical-align:-3px; }
      .as-chip-icon svg { width:13px; height:13px; }
      .as-chip-icon .plg-mono { width:13px; height:13px; border-radius:4px; font-size:8px; }
      .as-plug-backdrop { position:fixed; inset:0; z-index:1150; display:flex; align-items:center; justify-content:center;
        background:rgba(0,0,0,.55); backdrop-filter:blur(2px); }
      .as-plug { display:flex; flex-direction:column; width:min(940px, 92vw); height:min(680px, 84vh);
        border:1px solid var(--border-color,#303038); border-radius:14px; background:var(--bg-secondary,#17171c);
        box-shadow:0 24px 60px rgba(0,0,0,.5); overflow:hidden; }
      .as-plug-head { display:flex; align-items:center; gap:10px; padding:16px 18px 10px; }
      .as-plug-head h2 { margin:0; flex:1; color:var(--text-primary,#e8e8ec); font-size:16px; font-weight:640; }
      .as-plug-back, .as-plug-x { border:0; background:transparent; color:var(--text-secondary,#8a8a94); font:inherit; font-size:15px; cursor:pointer; }
      .as-plug-back:hover, .as-plug-x:hover { color:var(--text-primary,#e8e8ec); }
      .as-plug-bar { display:flex; align-items:center; gap:6px; padding:0 18px 12px; }
      .as-plug-tab { padding:5px 11px; border:0; border-radius:7px; background:transparent; color:var(--text-secondary,#8a8a94);
        font:inherit; font-size:12px; cursor:pointer; }
      .as-plug-tab.on { background:rgba(255,255,255,.08); color:var(--text-primary,#e8e8ec); }
      .as-plug-search { width:240px; padding:6px 10px; border:1px solid var(--border-color,#303038); border-radius:8px;
        background:var(--bg-primary,#0a0a0f); color:var(--text-primary,#e8e8ec); font:inherit; font-size:12px; }
      .as-plug-body { flex:1 1 auto; min-height:0; overflow-y:auto; padding:0 18px 12px; }
      .as-plug-group { padding:14px 0 8px; color:var(--text-secondary,#7a7a84); font-size:10px; font-weight:700;
        letter-spacing:.1em; text-transform:uppercase; }
      .as-plug-grid { display:grid; grid-template-columns:repeat(2, minmax(0,1fr)); gap:8px; }
      .as-plug-row { display:flex; align-items:center; gap:11px; padding:11px 12px; border:1px solid transparent;
        border-radius:10px; background:rgba(255,255,255,.02); cursor:pointer; }
      .as-plug-row:hover { border-color:var(--border-color,#303038); background:rgba(255,255,255,.045); }
      .as-plug-icon { display:grid; place-items:center; width:34px; height:34px; flex:0 0 auto; border-radius:9px; background:rgba(255,255,255,.06); }
      .as-plug-icon svg { width:20px; height:20px; }
      .as-plug-icon.lg { width:52px; height:52px; border-radius:13px; }
      .as-plug-icon.lg svg { width:30px; height:30px; }
      .as-plug-copy { display:flex; flex-direction:column; gap:2px; min-width:0; flex:1 1 auto; }
      .as-plug-name { display:flex; align-items:center; gap:8px; color:var(--text-primary,#e8e8ec); font-size:13px; font-weight:600; }
      .as-plug-desc, .as-plug-run { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .as-plug-desc { color:var(--text-secondary,#8a8a94); font-size:11px; }
      .as-plug-run { color:#5f5f68; font-family:var(--font-mono,monospace); font-size:9px; }
      .as-plug-src { color:var(--as-accent,#f5b840); font-size:10px; font-weight:500; text-decoration:none; }
      .as-plug-src.muted { color:#5f5f68; }
      .as-plug-add { flex:0 0 auto; padding:5px 14px; border:1px solid var(--border-color,#3a3a43); border-radius:7px;
        background:transparent; color:var(--text-primary,#e8e8ec); font:inherit; font-size:11px; cursor:pointer; }
      .as-plug-add:hover { background:rgba(255,255,255,.06); }
      .as-plug-add.solid { border-color:var(--as-accent,#f5b840); background:var(--as-accent,#f5b840); color:#0a0a0f; font-weight:600; }
      .as-plug-connected { flex:0 0 auto; color:#4ade80; font-size:11px; font-weight:500; }
      .as-plug-check { display:grid; place-items:center; flex:0 0 auto; width:20px; height:20px; border-radius:5px;
        background:#22c55e; color:#0a0a0f; font-size:12px; font-weight:800; line-height:1; }
      .as-plug-check.lg { width:26px; height:26px; border-radius:7px; font-size:15px; }
      .as-plug-problem { width:100%; margin-top:8px; padding:8px 10px; border:1px solid #4a2320; border-radius:8px;
        background:rgba(239,68,68,.08); color:#f4a9a3; font-size:11px; line-height:1.5; }
      .as-plug-fields { display:none; }
      .as-plug-fields.open, .as-plug-row .as-plug-fields { display:flex; flex-wrap:wrap; gap:6px; width:100%; margin-top:8px; }
      .as-plug-input { flex:1 1 180px; min-width:0; padding:6px 9px; border:1px solid var(--border-color,#303038); border-radius:7px;
        background:var(--bg-primary,#0a0a0f); color:var(--text-primary,#e8e8ec); font:inherit; font-size:11px; }
      .as-plug-save { padding:6px 13px; border:0; border-radius:7px; background:var(--as-accent,#f5b840); color:#0a0a0f;
        font:inherit; font-size:11px; font-weight:600; cursor:pointer; }
      .as-plug-detail { display:flex; flex-direction:column; }
      .as-plug-detail-head { display:flex; align-items:center; gap:13px; padding:6px 0 4px; }
      .as-plug-detail-desc { margin:8px 0 0; color:var(--text-secondary,#a0a0aa); font-size:12px; line-height:1.65; }
      .as-plug-note { margin-top:10px; padding:9px 11px; border:1px solid #3a3220; border-radius:8px;
        background:rgba(245,184,64,.06); color:#d8c79a; font-size:11px; line-height:1.55; }
      .as-plug-panel { padding:11px 12px; border:1px solid var(--border-color,#303038); border-radius:10px; background:rgba(255,255,255,.02); }
      .as-plug-panel code { color:#bec5ce; font-family:var(--font-mono,monospace); font-size:11px; overflow-wrap:anywhere; }
      .as-plug-skill { display:flex; gap:10px; padding:7px 0; border-bottom:1px solid #1c1c22; font-size:11px; }
      .as-plug-skill:last-child { border-bottom:0; }
      .as-plug-skill strong { flex:0 0 150px; color:var(--text-primary,#e8e8ec); font-weight:600; }
      .as-plug-skill span { color:var(--text-secondary,#8a8a94); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .as-plug-muted { color:var(--text-secondary,#7a7a84); font-size:11px; line-height:1.55; }
      .as-plug-empty { padding:40px; text-align:center; color:var(--text-secondary,#8a8a94); font-size:12px; }
      .as-plug-foot { padding:11px 18px; border-top:1px solid var(--border-color,#26262c); color:var(--text-secondary,#7a7a84); font-size:11px; }
      .as-artifactcard { display:flex; align-items:center; gap:11px; margin:4px 0 0 28px; padding:11px 12px;
        border:1px solid var(--as-accent,#f5b840); border-radius:11px; background:rgba(245,184,64,.06); }
      .as-artifactcard-mark { display:grid; place-items:center; width:30px; height:30px; flex:0 0 auto;
        border-radius:8px; background:rgba(245,184,64,.16); color:var(--as-accent,#f5b840); font-size:14px; }
      .as-authcard { display:flex; align-items:center; gap:11px; margin:4px 0 0 28px; padding:11px 12px;
        border:1px solid var(--border-color,#303038); border-radius:11px; background:rgba(255,255,255,.03); }
      .as-build { display:flex; flex-direction:column; gap:8px; margin-top:11px; padding:11px; border:1px solid var(--border-color,#303038);
        border-radius:9px; background:rgba(245,184,64,.05); }
      .as-build-row { display:flex; align-items:center; gap:8px; }
      .as-build-input { flex:1 1 auto; min-width:0; padding:7px 9px; border:1px solid var(--border-color,#303038); border-radius:7px;
        color:var(--text-primary,#e8e8ec); background:var(--bg-primary,#0a0a0f); font:inherit; font-size:12px; }
      .as-build-note { color:var(--text-secondary,#8a8a94); font-size:11px; }
      .as-action { display:flex; gap:9px; align-items:center; padding:10px 12px; border:1px solid var(--border-color,#303038);
        border-radius:8px; background:var(--editor-surface,#19191e); color:var(--text-secondary,#9b9ba5); font-size:11px; }
      .as-action strong { color:var(--text-primary,#e8e8ec); font-weight:620; }
      /* The composer is a SIBLING of the scrolling list, not a child of it.
         Absolute took it out of the flow and the list scrolled underneath, so
         the newest line hid behind it; sticky inside the scroller then got
         clipped at the window edge. As a plain flex row after the scroller it
         cannot overlap anything and needs no padding kept in sync. */
      .as-composer-wrap { flex:0 0 auto; padding:14px 22px 18px; background:var(--bg-primary,#101014); }
      .as-composer { display:flex; gap:8px; width:min(780px,100%); margin:0 auto; padding:8px;
        border:1px solid var(--border-color,#373740); border-radius:11px; background:var(--editor-surface,#1b1b20);
        box-shadow:0 14px 38px rgba(0,0,0,.28); }
      .as-composer textarea { flex:1; min-height:42px; max-height:130px; resize:none; padding:10px 11px; border:0; outline:0;
        color:var(--text-primary,#eeeeF2); background:transparent; font:inherit; font-size:13px; }
      .as-composer textarea::placeholder,.as-input::placeholder { color:#73737e; }
      .as-send { width:36px; height:36px; align-self:flex-end; border:0; border-radius:8px; background:var(--as-accent);
        color:#17140b; font-size:18px; cursor:pointer; }
      .as-mic { width:36px; height:36px; align-self:flex-end; display:flex; align-items:center; justify-content:center;
        border:0; border-radius:8px; background:transparent; color:var(--text-secondary,#92929d); cursor:pointer; }
      .as-mic:hover { color:var(--text-primary,#eeeeF2); background:var(--bg-primary,#101014); }
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
      .as-tabs { display:flex; align-items:center; gap:4px; margin:0 0 14px; padding:3px; border:1px solid var(--border-color,#2a2a2f);
        border-radius:9px; background:var(--bg-secondary,#141419); width:fit-content; }
      .as-tab { padding:6px 14px; border:0; border-radius:7px; background:transparent; color:var(--text-secondary,#a0a0aa);
        font:inherit; font-size:12px; cursor:pointer; }
      .as-tab:hover { color:var(--text-primary,#e0e0e0); }
      .as-tab.as-tab-on { background:var(--as-accent); color:#0a0a0f; font-weight:600; }
      .as-tabpane[hidden] { display:none; }
      .as-tiles { display:grid; grid-template-columns:repeat(auto-fit,minmax(320px,1fr)); gap:10px; }
      .as-tile { border:1px solid var(--border-color,#2a2a2f); border-radius:10px; background:var(--bg-secondary,#141419);
        padding:12px 14px; display:flex; flex-direction:column; gap:4px; }
      .as-tile-head { display:flex; align-items:center; gap:10px; cursor:pointer; }
      .as-tile-name { font-size:13px; font-weight:600; color:var(--text-primary,#e0e0e0); flex:1; }
      .as-tile-state { font-family:var(--font-mono,monospace); font-size:9px; letter-spacing:.06em; text-transform:uppercase;
        color:var(--as-accent); }
      .as-tile-sub { font-size:11px; color:#7a7a84; cursor:pointer; }
      .as-tile-body { display:flex; flex-direction:column; gap:10px; margin-top:10px;
        padding-top:10px; border-top:1px solid #24242b; }
      .as-tile-body[hidden] { display:none; }
      .as-enf { font-family:var(--font-mono,monospace); font-size:9px; letter-spacing:.05em; text-transform:uppercase;
        border-radius:999px; padding:1px 7px; border:1px solid currentColor; }
      .as-enf.enforced { color:#10b981; }
      .as-enf.advisory { color:#a0a0a0; }
      .as-foundation { border:1px solid var(--border-color,#2a2a2f); border-radius:9px; margin-bottom:14px; overflow:hidden; }
      .as-foundation-head { display:flex; align-items:center; gap:9px; padding:10px 12px; cursor:pointer;
        background:var(--bg-secondary,#141419); }
      .as-foundation-caret { color:var(--text-secondary,#a0a0aa); font-size:10px; }
      .as-foundation-title { font-size:12px; font-weight:600; color:var(--text-primary,#e0e0e0); }
      .as-foundation-badge { font-family:var(--font-mono,monospace); font-size:10px; color:#0a0a0f; background:var(--as-accent);
        border-radius:999px; padding:1px 7px; }
      .as-foundation-ro { font-size:10px; color:var(--text-secondary,#a0a0aa); border:1px solid var(--border-color,#2a2a2f);
        border-radius:999px; padding:1px 7px; }
      .as-foundation-note { margin-left:auto; font-size:10px; color:var(--text-secondary,#7a7a84); }
      .as-foundation-body { margin:0; padding:12px 14px; max-height:280px; overflow:auto; white-space:pre-wrap;
        font-family:var(--font-mono,monospace); font-size:11px; line-height:1.55; color:var(--text-secondary,#a0a0aa);
        background:var(--bg-primary,#0a0a0f); border-top:1px solid var(--border-color,#2a2a2f); }
      .as-prompt { min-height:220px; font-family:var(--font-mono,monospace); font-size:12px; line-height:1.55; }
      .as-actions { display:flex; justify-content:space-between; gap:10px; margin-top:22px; }
      .as-actions-right { display:flex; gap:8px; margin-left:auto; }
      .as-error { min-height:17px; margin-top:10px; color:#ff8b84; font-size:12px; }
      .as-menu { position:fixed; z-index:10020; min-width:150px; padding:4px; border:1px solid #3b3b43; border-radius:8px;
        background:#222228; box-shadow:0 10px 30px rgba(0,0,0,.45); }
      .as-menu button { display:block; width:100%; padding:7px 9px; border:0; border-radius:5px; color:#e7e7eb; background:transparent;
        text-align:left; font:inherit; font-size:12px; cursor:pointer; }
      .as-menu button:hover { background:rgba(255,255,255,.08); }
      .as-project-overlay { position:absolute; z-index:80; inset:0; display:flex; align-items:center; justify-content:center;
        padding:24px; background:rgba(5,7,10,.74); backdrop-filter:blur(2px); }
      .as-project-dialog { width:min(470px,100%); padding:20px; border:1px solid var(--border-color,#41414a); border-radius:11px;
        background:var(--editor-surface,#1c1c21); box-shadow:0 24px 70px rgba(0,0,0,.55); }
      .as-project-dialog h2 { margin:0 0 7px; color:var(--text-primary,#f1f1f4); font-size:18px; }
      .as-project-dialog p { margin:0 0 18px; color:var(--text-secondary,#9a9aa4); font-size:12px; line-height:1.5; }
      .as-project-choices { display:grid; grid-template-columns:1fr 1fr; gap:10px; }
      .as-project-choice { padding:14px; border:1px solid var(--border-color,#3b3b44); border-radius:9px; color:var(--text-primary,#ececf0);
        background:var(--bg-tertiary,#24242a); text-align:left; font:inherit; cursor:pointer; }
      .as-project-choice strong,.as-project-choice span { display:block; }.as-project-choice span { margin-top:5px; color:var(--text-secondary,#92929d); font-size:11px; }
      .as-project-choice:hover { border-color:var(--as-accent); }.as-project-actions { display:flex; justify-content:flex-end; gap:8px; margin-top:18px; }
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

  // What reconciliation did to the profile store, keyed by handle. A merge that
  // silently declines to do something is the seed-once bug wearing a merge's
  // clothes: the store says one thing, the app does another, and nobody can
  // tell. Refreshed beside the profile list, shown on the row as a tooltip.
  let libraryNotes = {};
  async function refreshLibraryNotes() {
    const notes = (await invoke('agent_profile_notes').catch(() => [])) || [];
    libraryNotes = {};
    for (const note of notes) {
      if (!note || !note.agent_id) continue;
      libraryNotes[note.agent_id] = libraryNotes[note.agent_id]
        ? `${libraryNotes[note.agent_id]}; ${note.message}` : note.message;
    }
  }

  function libraryMarkup(profiles, sessions, selectedHandle, selectedThreadId) {
    return `<aside class="asl" aria-label="Agent Library"><div class="asl-head"><span>Agents</span><button class="asl-add" data-library-new aria-label="New Agent" title="New Agent">+</button></div><div class="asl-list">${profiles.map((profile) => {
      const session = sessionFor(profile, sessions);
      const status = session && session.status || 'idle';
      const selected = profile.handle === selectedHandle;
      const threads = selected ? threadsFor(profile.handle) : [];
      const archived = selected ? archivedThreadsFor(profile.handle) : [];
      const threadRow = (thread, archivedThread = false) => `<div class="asl-thread ${thread.id === selectedThreadId ? 'selected' : ''} ${archivedThread ? 'archived' : ''}" data-library-thread="${esc(thread.id)}"><span class="asl-thread-label">${esc(thread.title || 'Untitled thread')}</span><button class="asl-thread-more" data-thread-more aria-label="Actions for ${archivedThread ? 'archived ' : ''}thread ${esc(thread.title || 'Untitled thread')}">•••</button></div>`;
      const note = libraryNotes[profile.handle] || '';
      return `<div class="asl-agent ${selected ? 'selected' : ''}" data-library-agent="${esc(profile.handle)}" style="--agent-accent:${esc(profile.accent_color || '#666')}"${note ? ` title="${esc(note)}"` : ''}><span class="asl-avatar">${esc(initials(profile))}</span><span class="asl-copy"><span class="asl-name">${esc(profile.display_name)}${note ? ' <span class="asl-note" aria-label="Store note">·</span>' : ''}</span><span class="asl-meta"><span class="asl-dot ${esc(status)}"></span><span>@${esc(profile.handle)}</span><span>· ${esc(status === 'idle' ? 'Ready' : status)}</span></span></span><button class="asl-caret" data-threads-toggle="${esc(profile.handle)}" aria-label="Show threads for ${esc(profile.display_name)}">${selected && threadsOpen(profile.handle) ? '▾' : '▸'}</button><button class="asl-more" data-library-more aria-label="Actions for ${esc(profile.display_name)}">•••</button></div>${selected && threadsOpen(profile.handle) ? `<div class="asl-threads">${threads.slice(0,8).map((thread) => threadRow(thread)).join('')}<div class="asl-thread new" data-library-new-thread>+ New thread</div>${archived.length ? (() => { let archivedOpen = false; try { archivedOpen = localStorage.getItem('xnaut-as-archived-open:' + profile.handle) === '1'; } catch (_) {} return `<div class="asl-archive-head" data-archived-toggle title="Show or hide archived threads"><span>${archivedOpen ? '▾' : '▸'} Archived · ${archived.length}</span><button class="asl-archive-clear" data-archived-clear title="Delete all archived threads">Delete all…</button></div>${archivedOpen ? archived.slice(0,5).map((thread) => threadRow(thread, true)).join('') : ''}`; })() : ''}</div>` : ''}`;
    }).join('') || '<div class="as-help" style="padding:12px">No agents yet.</div>'}</div></aside>`;
  }

  function openLibraryMenu(event, profile) {
    event.preventDefault(); event.stopPropagation();
    document.querySelector('.as-menu[data-library-menu]')?.remove();
    const menu = document.createElement('div');
    menu.className = 'as-menu'; menu.dataset.libraryMenu = '1';
    const rect = event.currentTarget && event.currentTarget.getBoundingClientRect ? event.currentTarget.getBoundingClientRect() : null;
    menu.innerHTML = `<button data-edit>Edit / Settings</button><button data-duplicate>Duplicate</button><button data-assign>Assign project</button>${profile.handle === 'nautbot' ? '' : '<button data-delete style="color:#ff6b63">Delete…</button>'}`;
    document.body.appendChild(menu);
    // Placed after mounting: the helper measures the menu to keep it on screen,
    // and it divides by the interface zoom, which a raw clientX does not.
    window.xnautPlaceAtClick(menu, event.clientX || (rect && rect.right) || 20, event.clientY || (rect && rect.bottom) || 20);
    const close = () => menu.remove();
    menu.querySelector('[data-edit]').onclick = () => { close(); window.xnautOpenAgentSettings(profile.handle); };
    menu.querySelector('[data-duplicate]').onclick = async () => {
      const newHandle = await promptDialog(`Duplicate @${profile.handle} as:`, `${profile.handle}-copy`, 'Duplicate'); if (!newHandle) return close();
      try { const duplicate = await invoke('agent_profile_duplicate', { handle:profile.handle, newHandle, displayName:`${profile.display_name} Copy` }); announceProfilesChanged(duplicate); close(); window.xnautOpenAgentSpace(duplicate.handle); }
      catch (error) { console.error('[agent-space] duplicate failed:', error); close(); }
    };
    menu.querySelector('[data-assign]').onclick = async () => {
      const project = await promptDialog('Default project path:', profile.default_project || '', 'Save'); if (project == null) return close();
      try { const saved = await invoke('agent_profile_update', { handle:profile.handle, profile:{ ...profile, default_project:project.trim() || null } }); announceProfilesChanged(saved); }
      catch (error) { console.error('[agent-space] assign project failed:', error); } close();
    };
    const deleteButton = menu.querySelector('[data-delete]');
    if (deleteButton) deleteButton.onclick = async () => {
      if (!await confirmDialog(`Delete ${esc(profile.display_name)} (@${esc(profile.handle)})?`, 'Delete')) return;
      try { await invoke('agent_profile_delete', { handle:profile.handle, rel:null }); announceProfilesChanged(); close(); window.xnautOpenAgentSpace(); }
      catch (error) { console.error('[agent-space] delete failed:', error); close(); }
    };
    setTimeout(() => document.addEventListener('mousedown', (click) => {
      if (!menu.contains(click.target)) close();
    }, { once:true }), 0);
  }

  function openThreadMenu(event, profile, thread, selectedThreadId) {
    event.preventDefault(); event.stopPropagation();
    document.querySelector('.as-menu[data-thread-menu]')?.remove();
    const menu = document.createElement('div');
    menu.className = 'as-menu'; menu.dataset.threadMenu = '1';
    const rect = event.currentTarget && event.currentTarget.getBoundingClientRect ? event.currentTarget.getBoundingClientRect() : null;
    menu.innerHTML = `<button data-archive>${thread.archived_at ? 'Restore' : 'Archive'}</button><button data-delete style="color:#ff6b63">Delete…</button>`;
    document.body.appendChild(menu);
    window.xnautPlaceAtClick(menu, event.clientX || (rect && rect.right) || 20, event.clientY || (rect && rect.bottom) || 20);
    const close = () => menu.remove();
    menu.querySelector('[data-archive]').onclick = () => {
      updateThread(profile.handle, thread.id, (next) => {
        if (next.archived_at) delete next.archived_at;
        else next.archived_at = nowIso();
        return next;
      });
      close();
      window.xnautOpenAgentSpace(profile.handle, selectedThreadId === thread.id ? null : selectedThreadId);
    };
    menu.querySelector('[data-delete]').onclick = async () => {
      close();
      if (!await confirmDialog(`Delete thread “${esc(thread.title || 'Untitled thread')}” permanently?`, 'Delete')) return;
      deleteThread(profile.handle, thread.id);
      if (window.xnautNotify) window.xnautNotify('Thread deleted', thread.title || 'Untitled thread');
      window.xnautOpenAgentSpace(profile.handle, selectedThreadId === thread.id ? null : selectedThreadId);
    };
    setTimeout(() => document.addEventListener('mousedown', (click) => {
      if (!menu.contains(click.target)) close();
    }, { once:true }), 0);
  }

  function wireLibrary(pane, profiles, selectedHandle) {
    const selected = profiles.find((profile) => profile.handle === selectedHandle);
    const add = pane.querySelector('[data-library-new]'); if (add) add.onclick = () => window.xnautOpenNewAgent();
    pane.querySelectorAll('[data-library-agent]').forEach((row) => {
      const profile = profiles.find((item) => item.handle === row.dataset.libraryAgent); if (!profile) return;
      row.onclick = () => window.xnautOpenAgentSpace(profile.handle);
      const caret = row.querySelector('[data-threads-toggle]');
      if (caret) caret.onclick = (event) => {
        event.stopPropagation();
        const handle = caret.dataset.threadsToggle;
        if (handle !== selectedHandle) { toggleThreads(handle); window.xnautOpenAgentSpace(handle); return; }
        toggleThreads(handle);
        window.xnautOpenAgentSpace(handle);
      };
      row.oncontextmenu = (event) => openLibraryMenu(event, profile);
      row.querySelector('[data-library-more]').onclick = (event) => openLibraryMenu(event, profile);
    });
    if (!selected) return;
    const selectedThreadId = pane.querySelector('[data-library-thread].selected')?.dataset.libraryThread || null;
    pane.querySelectorAll('[data-library-thread]').forEach((row) => {
      const thread = allThreadsFor(selected.handle).find((item) => item.id === row.dataset.libraryThread);
      row.onclick = () => window.xnautOpenAgentSpace(selected.handle, row.dataset.libraryThread);
      row.oncontextmenu = (event) => thread && openThreadMenu(event, selected, thread, selectedThreadId);
      const more = row.querySelector('[data-thread-more]');
      if (more) more.onclick = (event) => thread && openThreadMenu(event, selected, thread, selectedThreadId);
    });
    const fresh = pane.querySelector('[data-library-new-thread]'); if (fresh) fresh.onclick = () => window.xnautOpenAgentSpace(selected.handle, null, true);
    const clearArchived = pane.querySelector('[data-archived-clear]');
    if (clearArchived) clearArchived.onclick = async (event) => {
      event.stopPropagation();
      const count = archivedThreadsFor(selected.handle).length;
      if (!count) return;
      if (!await confirmDialog(`Delete all ${count} archived thread${count === 1 ? '' : 's'} for @${esc(selected.handle)} permanently?`, 'Delete all')) return;
      const removed = deleteArchivedThreads(selected.handle);
      if (window.xnautNotify) window.xnautNotify('Archived threads deleted', `${removed} removed`);
      window.xnautOpenAgentSpace(selected.handle, selectedThreadId);
    };
    const archiveHead = pane.querySelector('[data-archived-toggle]');
    if (archiveHead) archiveHead.onclick = (event) => {
      if (event.target.closest('[data-archived-clear]')) return;
      const key = 'xnaut-as-archived-open:' + selected.handle;
      try {
        if (localStorage.getItem(key) === '1') localStorage.removeItem(key);
        else localStorage.setItem(key, '1');
      } catch (_) {}
      window.xnautOpenAgentSpace(selected.handle, selectedThreadId);
    };
  }

  // A blocking modal on first contact is the wrong shape: the answer is nearly
  // always "a new folder named after the work", and being interrogated before
  // every first message reads as an obstacle. One prompt, a sensible default
  // path, saved on the profile so it is asked exactly once.
  async function quickProject(profile, existing) {
    let home = '';
    try { home = await invoke('get_home_directory'); } catch (_) { home = ''; }
    const suggestion = existing
      ? `${home}/`
      : `${home}/xnaut-projects/${profile.handle || 'project'}`;
    const answer = await promptDialog(
      existing ? 'Path to the existing project folder' : 'Create a new project folder at',
      suggestion,
      existing ? 'Use this folder' : 'Create'
    );
    if (!answer || !answer.trim()) return null;
    try {
      const path = await invoke('agent_project_prepare', { path: answer.trim(), newProject: !existing });
      const saved = await invoke('agent_profile_update', { handle: profile.handle, profile: { ...profile, default_project: path } });
      Object.assign(profile, saved || { default_project: path });
      return path;
    } catch (error) {
      alert(String(error));
      return null;
    }
  }

  function chooseProjectContext(pane, profile) {
    return new Promise((resolve) => {
      const stage = pane.querySelector('.as-stage');
      if (!stage) return resolve(null);
      const overlay = document.createElement('div');
      overlay.className = 'as-project-overlay';
      const finish = (value) => { overlay.remove(); resolve(value || null); };
      const renderPath = (newProject) => {
        overlay.innerHTML = `<form class="as-project-dialog" data-project-form><h2>${newProject ? 'Create the local project.' : 'Connect the existing project.'}</h2>
          <p>The agent CLI will start inside this folder. It will read and write code only from this project context.</p>
          <label class="as-field"><span class="as-section-label">Local project path</span><input class="as-input" data-project-path required placeholder="/Users/you/Projects/honey-zurich"></label>
          <div class="as-error" data-project-error></div><div class="as-project-actions"><button type="button" class="as-button" data-project-back>Back</button><button type="button" class="as-button" data-project-cancel>Cancel</button><button type="submit" class="as-button primary">${newProject ? 'Create and continue' : 'Use this project'}</button></div></form>`;
        const form = overlay.querySelector('[data-project-form]');
        const input = overlay.querySelector('[data-project-path]');
        overlay.querySelector('[data-project-back]').onclick = renderQuestion;
        overlay.querySelector('[data-project-cancel]').onclick = () => finish(null);
        form.onsubmit = async (event) => {
          event.preventDefault();
          const error = overlay.querySelector('[data-project-error]');
          const submit = form.querySelector('[type="submit"]');
          submit.disabled = true; error.textContent = '';
          try {
            const path = await invoke('agent_project_prepare', { path:input.value.trim(), newProject });
            const saved = await invoke('agent_profile_update', {
              handle:profile.handle,
              profile:{ ...profile, default_project:path },
            });
            Object.assign(profile, saved || { default_project:path });
            announceProfilesChanged(profile);
            finish(path);
          } catch (problem) {
            error.textContent = String(problem);
            submit.disabled = false;
          }
        };
        setTimeout(() => input.focus(), 0);
      };
      const renderQuestion = () => {
        overlay.innerHTML = `<div class="as-project-dialog" role="dialog" aria-modal="true" aria-label="Choose project context"><h2>Is this a new project?</h2>
          <p>${esc(profile.display_name)} needs one explicit local project folder before the coding CLI can start.</p>
          <div class="as-project-choices"><button class="as-project-choice" data-project-new><strong>Yes, new project</strong><span>Create the folder and start there.</span></button><button class="as-project-choice" data-project-existing><strong>No, existing project</strong><span>Connect a folder already on this Mac.</span></button></div>
          <div class="as-project-actions"><button class="as-button" data-project-cancel>Cancel</button></div></div>`;
        overlay.querySelector('[data-project-new]').onclick = () => renderPath(true);
        overlay.querySelector('[data-project-existing]').onclick = () => renderPath(false);
        overlay.querySelector('[data-project-cancel]').onclick = () => finish(null);
      };
      renderQuestion();
      stage.appendChild(overlay);
    });
  }

  function profilePayload(values, original) {
    const skills = Array.from(values.skills || []).map((skill) => `skill:${skill}`);
    // Collaborators ride in capabilities the same way skills do, so a handoff
    // allowlist is one field on the profile rather than a second store.
    const collabs = Array.from(values.collabs || []).map((handle) => `collab:${handle}`);
    const existingCapabilities = (original && original.capabilities || [])
      .filter((value) => !String(value).startsWith('skill:') && !String(value).startsWith('collab:'));
    return {
      handle: handleOf(values.handle),
      display_name: String(values.display_name || '').trim(),
      tagline: String(values.tagline || '').trim().slice(0, 72),
      purpose: String(values.purpose || '').trim(),
      runtime_id: String(values.runtime_id || '').trim(),
      provider: String(values.provider || 'global').trim(),
      model: String(values.model || '').trim(),
      // Separate from `model` on purpose: that one becomes --model on a CLI,
      // this one is the chat route and is the only one that needs tool calls.
      chat_model: String(values.chat_model || '').trim(),
      reasoning_effort: String(values.reasoning_effort || '').trim(),
      execution: values.execution === 'sandbox' ? 'sandbox' : 'local',
      role: String(values.role || 'coding-agent').trim(),
      capabilities: Array.from(new Set(existingCapabilities.concat(skills, collabs))),
      policy: values.policy || (original && original.policy) || undefined,
      notifications: values.notifications !== false,
      accent_color: String(values.accent_color || '#f5b840'),
      default_project: values.default_project || null,
      created_at: original && original.created_at || '',
      updated_at: original && original.updated_at || '',
    };
  }

  async function renderThread(pane, options) {
    const profiles = pinNautbotFirst((await invoke('agent_profile_list').catch(() => [])) || []);
    await refreshLibraryNotes();
    const sessions = (await invoke('agent_sessions_list').catch(() => [])) || [];
    const runtimes = (await invoke('agent_list').catch(() => [])) || [];
    const profile = profiles.find((item) => item.handle === handleOf(options.handle)) || profiles[0];
    if (!profile) {
      pane.innerHTML = '<div class="as-empty"><h2>No agents yet.</h2><p>Create the first agent identity to start a conversation.</p><button class="as-button primary" data-new>Create agent</button></div>';
      pane.querySelector('[data-new]').onclick = () => window.xnautOpenNewAgent();
      return;
    }
    const recent = threadsFor(profile.handle);
    // Reuse an existing EMPTY thread before creating another one. Eager
    // creation stacked identical "New thread" rows, and the auto-create on
    // remount resurrected the look of a thread the user had just deleted —
    // which read as "delete does not work".
    const emptyExisting = recent.find((item) => !(Array.isArray(item.messages) && item.messages.length));
    let thread = options.newThread ? emptyExisting : (recent.find((item) => item.id === options.threadId) || recent[0]);
    if (!thread) thread = emptyExisting || newThread(profile.handle, 'New thread');
    const session = sessionFor(profile, sessions);
    // The thread's harness, not the profile's: XNAUT-150 switches one
    // conversation without moving every other thread of the same agent.
    let threadRuntime = thread.runtime_id || profile.runtime_id;
    let sessionId = thread.session_id || session && session.session_id || null;
    const status = session && session.status || 'idle';
    pane.style.setProperty('--profile-accent', profile.accent_color || '#f5b840');
    pane.style.setProperty('--as-accent', profile.accent_color || '#f5b840');
    pane.innerHTML = `${libraryMarkup(profiles, sessions, profile.handle, thread.id)}<div class="as-stage" data-stage>
      <div class="as-conv">
      <header class="as-head">
        <div class="as-avatar">${esc(initials(profile))}</div>
        <div class="as-title"><div class="as-title-row"><h1>${esc(profile.display_name)}</h1><span class="as-handle">@${esc(profile.handle)}</span></div>
          <div class="as-status"><span class="as-status-dot ${esc(status)}"></span><span>${esc(status === 'idle' ? 'Ready' : status)}</span>${session ? '<span>· terminal attached</span>' : ''}</div></div>
        <select class="as-harness" data-harness title="Harness this thread runs under" aria-label="Harness for this thread">${(runtimes.length ? runtimes : [{ id: profile.runtime_id, label: profile.runtime_id }]).map((runtime) => `<option value="${esc(runtime.id)}" ${runtime.id === threadRuntime ? 'selected' : ''} ${runtime.available === false && runtime.id !== threadRuntime ? 'disabled' : ''}>${esc(runtime.label || runtime.id)}${runtime.available === false ? ' · not installed' : ''}</option>`).join('')}</select>
        <button class="as-button" data-terminal aria-label="Open terminal" title="Open terminal" ${sessionId ? '' : 'hidden'}>&gt;_</button>
        <button class="as-button" data-project-new title="${profile.default_project ? esc(profile.default_project) : 'No project set — a build will ask'}" aria-label="Project folder">${profile.default_project ? '📁' : '📂'}</button>
        <button class="as-button" data-canvas title="Canvas" aria-label="Canvas" hidden>▦</button>
        <button class="as-button" data-attach title="Plugins for this agent" aria-label="Plugins">+</button>
        <button class="as-button" data-settings>Settings</button>
      </header>
      <div class="as-body as-thread">
        <div class="as-messages" data-messages></div>
      </div>
      <div class="as-composer-wrap"><div class="as-composer">
        <textarea data-compose rows="1" placeholder="Message @${esc(profile.handle)}…" aria-label="Message @${esc(profile.handle)}"></textarea>
        <button class="as-mic" data-dictate title="Dictate message" aria-label="Dictate message">${window.xnautDictationMicSvg || ''}</button>
        <button class="as-send" data-send aria-label="Send message">↑</button>
      </div></div>
      </div>
      <aside class="as-split" data-split hidden></aside>
      </div>`;

    const messages = pane.querySelector('[data-messages]');
    // .as-messages never scrolls — .as-body is the one with overflow-y:auto —
    // so setting scrollTop on the list was a silent no-op and the newest reply
    // sat behind the composer. Stay pinned to the bottom unless he has
    // scrolled up to read something.
    const scroller = () => messages.closest('.as-body') || messages.parentElement;
    let stick = true;
    let programmatic = false;
    const scrollToEnd = () => {
      const box = scroller();
      if (!box || !stick) return;
      // Two frames, not one: the first lands after the new content is laid
      // out, the second catches a height that grew again while we were
      // scrolling (a streamed reply does exactly that).
      programmatic = true;
      const settle = () => { box.scrollTop = box.scrollHeight; };
      requestAnimationFrame(() => {
        settle();
        requestAnimationFrame(() => {
          settle();
          // One late pass for layout that lands after paint — a webfont
          // swapping in, or a long reply reflowing — then hand control back.
          setTimeout(() => { settle(); programmatic = false; }, 60);
        });
      });
    };
    // The build handshake. An agent that judges a request to need a coding
    // harness does not start one: it asks WHERE. The worktree is not optional
    // — an agent must never run in the checkout the owner has open.
    const buildCard = (message) => {
      if (!message.build_task || message.build_started) return '';
      return `<div class="as-build" data-build="${esc(message.id)}">
        <div class="as-build-row"><input class="as-build-input" data-build-path value="${esc(thread.workspace || profile.default_project || '')}" placeholder="/path/to/the/repository" spellcheck="false">
          <button class="as-button" data-build-pick>Choose…</button></div>
        <div class="as-build-row"><button class="as-button primary" data-build-go>Open worktree &amp; build</button>
          <span class="as-build-note">A worktree under .worktrees/ keeps this run out of your checkout.</span></div>
      </div>`;
    };

    const wireBuildCards = () => {
      messages.querySelectorAll('[data-build]').forEach((card) => {
        const record = (thread.messages || []).find((item) => item.id === card.dataset.build);
        const input = card.querySelector('[data-build-path]');
        card.querySelector('[data-build-pick]').onclick = async () => {
          const picked = await chooseProjectContext(pane, profile);
          if (picked) input.value = picked;
        };
        card.querySelector('[data-build-go]').onclick = async () => {
          const repo = String(input.value || '').trim();
          if (!repo) { input.focus(); return; }
          const go = card.querySelector('[data-build-go]');
          go.disabled = true; go.textContent = 'Opening worktree…';
          try {
            const workspace = await invoke('agent_build_workspace', {
              handle: profile.handle, repoPath: repo, task: record.build_task,
            });
            thread = updateThread(profile.handle, thread.id, (next) => {
              const item = next.messages.find((entry) => entry.id === record.id);
              if (item) item.build_started = true;
              next.workspace = workspace;
              return next;
            });
            paintMessages();
            await submit(record.build_task, workspace);
          } catch (error) {
            go.disabled = false; go.textContent = 'Open worktree & build';
            updateAgentMessage(record.id, `${record.text}\n\nCould not open a worktree: ${String(error)}`);
          }
        };
      });
    };

    // Reading history must not be yanked away by a streaming answer.
    const watchScroll = () => {
      const box = scroller();
      if (!box || box.dataset.stickWired) return;
      box.dataset.stickWired = '1';
      box.addEventListener('scroll', () => {
        // Ignore our OWN scrolling. Treating it as "he scrolled up" was why
        // the view stopped following: one mid-flight event set stick=false
        // and every later paint skipped the scroll.
        if (programmatic) return;
        stick = box.scrollHeight - box.scrollTop - box.clientHeight < 120;
      });
    };

    /// What the turn actually did, under what it said.
    ///
    /// An answer is prose and cannot be checked; a tool list can. Three test
    /// runs were lost to an agent describing work it had not performed, and
    /// "did nothing" is the line that would have caught every one of them
    /// on sight, which is why it is shown rather than hidden when empty.
    const receiptLine = (message) => {
      if (!message || message.role === 'user' || !Array.isArray(message.tools_used)) return '';
      const tools = message.tools_used.map((t) => String(t).split('(')[0]);
      const unique = [...new Set(tools)];
      const text = unique.length ? unique.join(', ') : 'did nothing';
      return `<div class="as-receipt${unique.length ? '' : ' none'}">${esc(text)}</div>`;
    };

    const paintMessages = () => {
      watchScroll();
      const items = Array.isArray(thread.messages) ? thread.messages : [];
      if (!items.length) {
        messages.innerHTML = `<div class="as-empty"><h2>Talk to ${esc(profile.display_name)}.</h2><p>${esc(profile.tagline || profile.purpose)}</p></div>`;
        return;
      }
      messages.innerHTML = items.map((message) => message.kind === 'document'
        ? `<div class="as-artifactcard">
            <span class="as-artifactcard-mark">📄</span>
            <span class="as-plug-copy"><span class="as-plug-name">${esc(message.title || 'Document')}</span>
              <span class="as-plug-desc">Document · ${esc(String(message.count || 0))} words</span></span>
            <button class="as-plug-add" data-open-document>Open ⤢</button>
          </div>`
        : message.kind === 'canvas'
        ? `<div class="as-artifactcard">
            <span class="as-artifactcard-mark">▦</span>
            <span class="as-plug-copy"><span class="as-plug-name">${esc(message.title || 'Canvas')}</span>
              <span class="as-plug-desc">Diagram · ${esc(String(message.count || 0))} boxes</span></span>
            <button class="as-plug-add" data-open-canvas>Open ⤢</button>
          </div>`
        : message.kind === 'auth'
        ? `<div class="as-authcard" data-authcard="${esc(message.plugin.id)}">
            <span class="as-plug-icon">${window.xnautPluginIconFor ? window.xnautPluginIconFor(message.plugin) : ''}</span>
            <span class="as-plug-copy"><span class="as-plug-name">${esc(message.plugin.name)}</span>
              <span class="as-plug-desc">${esc(message.plugin.description || '')}</span></span>
            <button class="as-plug-add solid" data-authorize="${esc(message.plugin.id)}">Authorize</button>
          </div>`
        : message.kind === 'action'
        ? `<div class="as-action"><strong>${esc(message.label || 'Started')}</strong><span>${esc(message.detail || '')}</span><span style="margin-left:auto">${esc(new Date(message.at).toLocaleTimeString([], { hour:'2-digit', minute:'2-digit' }))}</span>${message.session_id ? `<button class="as-button" data-open-session="${esc(message.session_id)}">Terminal</button>` : ''}</div>`
        : `<div class="as-message ${message.role === 'user' ? 'user' : 'agent'}" data-message-id="${esc(message.id)}"><div class="as-message-text">${esc(message.text)}</div>${receiptLine(message)}${buildCard(message)}</div>`
      ).join('');
      wireBuildCards();
      messages.querySelectorAll('[data-open-document]').forEach((button) => {
        button.onclick = () => { if (window.__xnautOpenDocumentSplit) window.__xnautOpenDocumentSplit(); };
      });
      messages.querySelectorAll('[data-open-canvas]').forEach((button) => {
        button.onclick = () => { if (window.__xnautOpenCanvasSplit) window.__xnautOpenCanvasSplit(); };
      });
      messages.querySelectorAll('[data-authorize]').forEach((button) => {
        // Straight into the field that is missing — the point of the card is
        // that the sign-in happens HERE, not after a hunt through settings.
        button.onclick = () => openPlugins(button.dataset.authorize);
      });
      messages.querySelectorAll('[data-open-session]').forEach((button) => {
        button.onclick = () => window.xnautOpenAgentSession && window.xnautOpenAgentSession(button.dataset.openSession, profile.display_name);
      });
      scrollToEnd();
    };
    paintMessages();

    // The compute quick pane registers itself but nothing invoked it (XNAUT-144
    // gap). It sits HERE and not at the end of the function on purpose: a throw
    // in any later wiring step used to leave the right pane blank, which is
    // indistinguishable from the pane being broken.
    if (window.xnautRightPaneOpenAgent) window.xnautRightPaneOpenAgent(profile);

    const composer = pane.querySelector('[data-compose]');
    const send = pane.querySelector('[data-send]');
    // The mic used to exist only in the chat pane, so dictation was invisible
    // in the composer he actually types into (XNAUT-187).
    const dictate = pane.querySelector('[data-dictate]');
    if (dictate && window.xnautAttachDictation) {
      window.xnautAttachDictation(dictate, (text) => {
        window.xnautDictationAppend(composer, text);
      });
    }
    const terminalButton = pane.querySelector('[data-terminal]');
    const showTerminal = (nextSessionId) => {
      sessionId = nextSessionId || sessionId;
      if (!terminalButton || !sessionId) return;
      terminalButton.hidden = false;
      terminalButton.dataset.sessionId = sessionId;
    };
    showTerminal(sessionId);
    // A session from a previous window is still running: reveal the terminal
    // button so it can be reattached, instead of hiding it as if nothing were.
    (async () => {
      if (sessionId) return;
      const alive = await invoke('agent_session_alive', { handle: profile.handle }).catch(() => false);
      if (alive && terminalButton) terminalButton.hidden = false;
    })();

    const turnCleanups = [];
    pane._agentSpaceCleanup = () => {
      turnCleanups.splice(0).forEach((cleanup) => { try { cleanup(); } catch (_) {} });
    };
    // XNAUT-251: keep the receipt with the message it belongs to, so it
    // survives a repaint and can be read afterwards.
    const recordReceipt = (messageId, tools) => {
      thread = updateThread(profile.handle, thread.id, (next) => {
        const message = next.messages.find((item) => item.id === messageId);
        if (message) message.tools_used = tools;
        return next;
      });
      paintMessages();
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
    // XNAUT-66: the run lives in a zellij session that outlives the app, so the
    // PTY is only a viewport and its bytes carry zellij's chrome. The script
    // tees the CLI's own stdout to a file; this reads that, by offset, so a
    // long run is tailed rather than re-parsed on every tick.
    const captureRunFile = async (outputPath, messageId, zellijSession) => {
      const parser = structuredTurn(
        (text) => updateAgentMessage(messageId, text),
        (conversationId) => {
          thread = updateThread(profile.handle, thread.id, (next) => {
            next.conversation_id = conversationId;
            return next;
          });
        }
      );
      let offset = 0;
      let stopped = false;
      let guardProbe = '';
      const stop = () => { stopped = true; send.disabled = false; };
      turnCleanups.push(stop);
      const deadline = Date.now() + 30 * 60 * 1000;
      while (!stopped) {
        let chunk = null;
        try { chunk = await invoke('agent_run_output', { path: outputPath, offset }); }
        catch (_) { chunk = null; }
        if (chunk) {
          offset = chunk.next_offset;
          if (chunk.text) {
            guardProbe = (guardProbe + chunk.text).slice(-2048);
            // Two ways NautGate ends a run mid-flight, and from here they look
            // the same: the CLI keeps retrying into a wall until the session is
            // stopped. The launch binding is minted per launch with a fixed TTL
            // and held only in NautGate's memory, so a restart or six hours of
            // uptime invalidates it. xNAUT cannot renew one: re-registering
            // mints a different URL and the running process's env is already set.
            const halt = [
              ['nautgate_max_guard_paused',
                'NautGate paused this Claude session because it reached the configured Max-plan allowance. The background agent was stopped to prevent retries. Resume or authorize the session in Max Guard before continuing.'],
              ['invalid_or_expired_max_launch',
                'This session\'s NautGate launch binding expired, or NautGate restarted, so Claude was calling a route that no longer exists. The background agent was stopped to prevent retries. Start a new session to get a fresh binding.'],
            ].find(([needle]) => guardProbe.includes(needle));
            if (halt) {
              if (zellijSession) {
                await invoke('zellij_delete_session', { name: zellijSession }).catch(() => {});
              }
              updateAgentMessage(messageId, halt[1]);
              stop();
              return;
            }
            parser.push(chunk.text);
          }
          if (chunk.finished) {
            parser.flush();
            if (!parser.hasResponse()) {
              // The CLI's own words, not a shrug. codex refusing to start
              // outside a git repo said so on stderr while the chat said
              // "open Terminal", which nobody does.
              updateAgentMessage(messageId, chunk.error_tail
                ? `The run produced no answer. It said:\n\n${chunk.error_tail}`
                : 'The run finished without a conversational response. Open Terminal to see what it did.');
            }
            stop();
            return;
          }
        }
        if (Date.now() > deadline) { parser.flush(); stop(); return; }
        await new Promise((resolve) => setTimeout(resolve, 400));
      }
    };

    const captureStructuredTurn = async (nextSessionId, messageId) => {
      const liveDecoder = new TextDecoder('utf-8');
      const parser = structuredTurn(
        (text) => updateAgentMessage(messageId, text),
        (conversationId) => {
          thread = updateThread(profile.handle, thread.id, (next) => {
            next.conversation_id = conversationId;
            return next;
          });
        }
      );
      let stopped = false;
      let finishing = false;
      const subscriptions = [];
      let settleTimer = null;
      const stop = () => {
        if (stopped) return;
        stopped = true;
        if (settleTimer) clearTimeout(settleTimer);
        subscriptions.forEach((subscription) => Promise.resolve(subscription).then((unlisten) => {
          try { unlisten(); } catch (_) {}
        }).catch(() => {}));
        send.disabled = false;
      };
      const finish = async () => {
        if (finishing || stopped) return;
        finishing = true;
        const finalSnapshot = await invoke('terminal_output_snapshot', { sessionId:nextSessionId }).catch(() => '');
        if (finalSnapshot) parser.push(decodeTerminalBytes(finalSnapshot));
        parser.push(liveDecoder.decode());
        parser.flush();
        if (!parser.hasResponse()) {
          updateAgentMessage(messageId, 'The agent finished without a conversational response. Open Terminal for diagnostics.');
        }
        stop();
      };
      turnCleanups.push(stop);
      subscriptions.push(listen(`terminal-output:${nextSessionId}`, (event) => {
        if (!stopped && event && event.payload && event.payload.data) {
          const chunk = decodeTerminalBytes(event.payload.data, liveDecoder, true);
          parser.push(chunk);
          // Same output, second reader: a trigger set in Settings covers this
          // pane too, not just the tabs that own an xterm (XNAUT-199).
          if (window.xnautCheckTriggers) window.xnautCheckTriggers(chunk);
        }
      }));
      subscriptions.push(listen(`terminal-closed:${nextSessionId}`, finish));
      subscriptions.push(listen('agent-status-changed', (event) => {
        const state = event && event.payload;
        if (!state || state.session_id !== nextSessionId) return;
        if (['idle', 'waiting', 'done', 'blocked', 'permission', 'interrupted'].includes(state.status)) finish();
      }));
      // The PTY can emit before agent_profile_launch returns its id. Replay the
      // bounded backend tail after listeners are attached, then follow JSONL.
      const snapshot = await invoke('terminal_output_snapshot', { sessionId:nextSessionId }).catch(() => '');
      if (!stopped && snapshot) parser.push(decodeTerminalBytes(snapshot));
      const current = await invoke('agent_sessions_list').catch(() => []);
      const currentState = (current || []).find((item) => item.session_id === nextSessionId);
      if (currentState && ['idle', 'waiting', 'done', 'blocked', 'permission', 'interrupted'].includes(currentState.status)) finish();
      else settleTimer = setTimeout(finish, 10 * 60 * 1000);
    };

    // The (+) opens the Plugins modal: Marketplace and Yours, the shape André
    // pointed at. The Admin page in the sidebar stays what it is — the place a
    // plugin is configured in full. This is the fast path: find one, add it,
    // and it is connected to THIS agent.
    const openPlugins = async (focusId) => {
      let catalog = (await invoke('plugin_catalog').catch(() => [])) || [];
      const overlay = document.createElement('div');
      overlay.className = 'as-plug-backdrop';
      let tab = 'marketplace';
      let query = '';
      let expanded = focusId || null; // id whose credential fields are open
      let detail = focusId || null; // id opened in the detail view
      const skillCatalog = (await invoke('skill_catalog', { project: null }).catch(() => [])) || [];
      const held = () => new Set((profile.capabilities || [])
        .filter((item) => String(item).startsWith('plugin:')).map((item) => String(item).slice(7)));
      const icon = (plugin) => (window.xnautPluginIconFor ? window.xnautPluginIconFor(plugin) : '');
      const blockedBy = (plugin) => {
        if (plugin.transport === 'http' && !String(plugin.url || '').trim()) return 'needs its URL';
        if (plugin.transport === 'stdio' && !String(plugin.command || '').trim()) return 'needs a command';
        for (const key of plugin.required_env || []) {
          if (!String((plugin.env || {})[key] || '').trim()) return `needs ${key}`;
        }
        return null;
      };

      let failure = null; // { id, message } shown in the row that failed
      // Write the values, verify it starts, switch it on and hand it over —
      // one backend call. Doing it as save-then-grant from here had two
      // failure modes and two half-applied states, and an alert() for
      // anything that went wrong, which is how a typed token went missing.
      const connectPlugin = async (plugin, values, url) => {
        failure = null;
        try {
          await invoke('plugin_connect', {
            id: plugin.id, values: values || {}, url: url || null, agent: profile.handle,
          });
          catalog = (await invoke('plugin_catalog').catch(() => catalog)) || catalog;
          const saved = await invoke('agent_profile_get', { handle: profile.handle }).catch(() => null);
          if (saved) Object.assign(profile, saved);
          announceProfilesChanged(profile);
          return true;
        } catch (error) {
          failure = { id: plugin.id, message: String(error) };
          return false;
        }
      };
      const grant = async (id, on, skills = []) => {
        const set = held();
        if (on) set.add(id); else set.delete(id);
        // A plugin's own skills travel with it: the connector is the tools, the
        // skill is when to reach for them.
        const keepSkills = (profile.capabilities || []).filter((item) => String(item).startsWith('skill:'));
        const withSkills = on
          ? Array.from(new Set(keepSkills.concat(skills.map((name) => `skill:${name}`))))
          : keepSkills;
        const capabilities = (profile.capabilities || [])
          .filter((item) => !String(item).startsWith('plugin:') && !String(item).startsWith('skill:'))
          .concat(withSkills)
          .concat(Array.from(set).map((item) => `plugin:${item}`));
        const saved = await invoke('agent_profile_update', { handle: profile.handle, profile: { ...profile, capabilities } });
        Object.assign(profile, saved || { capabilities });
        announceProfilesChanged(profile);
      };

      const rowMarkup = (plugin) => {
        const on = held().has(plugin.id);
        const blocked = blockedBy(plugin);
        const state = on && plugin.enabled ? '<span class="as-plug-check" title="Connected" aria-label="Connected">✓</span>'
          : blocked ? `<button class="as-plug-add" data-add="${esc(plugin.id)}">Add</button>`
          : `<button class="as-plug-add" data-add="${esc(plugin.id)}">Add</button>`;
        const problem = failure && failure.id === plugin.id
          ? `<div class="as-plug-problem">${esc(failure.message)}</div>` : '';
        const fields = expanded === plugin.id ? `<div class="as-plug-fields">
            ${(plugin.transport === 'http' && !String(plugin.url || '').trim())
              ? `<input class="as-plug-input" data-key="url" placeholder="https://…/mcp" value="${esc(plugin.url || '')}">` : ''}
            ${(plugin.required_env || []).map((key) => `<input class="as-plug-input" data-key="env:${esc(key)}" placeholder="${esc(key)}" value="${esc((plugin.env || {})[key] || '')}">`).join('')}
            <button class="as-plug-save" data-save="${esc(plugin.id)}">Connect</button>
          </div>` : '';
        return `<div class="as-plug-row" data-row="${esc(plugin.id)}" data-open="${esc(plugin.id)}">
          <span class="as-plug-icon">${icon(plugin)}</span>
          <span class="as-plug-copy"><span class="as-plug-name">${esc(plugin.name)}
            ${plugin.docs_url ? `<a class="as-plug-src" href="${esc(plugin.docs_url)}" target="_blank" rel="noreferrer" title="${esc(plugin.docs_url)}">source ↗</a>` : ''}</span>
            <span class="as-plug-desc">${esc(blocked && expanded !== plugin.id ? `${plugin.description} · ${blocked}` : plugin.description)}</span>
            <span class="as-plug-run">${esc(plugin.transport === 'http' ? (plugin.url || 'http endpoint') : [plugin.command].concat(plugin.args || []).join(' '))}</span></span>
          ${state}${fields}${problem}</div>`;
      };

      const detailMarkup = (plugin) => {
        const on = held().has(plugin.id);
        const blocked = blockedBy(plugin);
        const connector = plugin.transport === 'http'
          ? (plugin.url || 'no endpoint yet')
          : [plugin.command].concat(plugin.args || []).join(' ');
        const skills = (plugin.skills || []).map((name) => {
          const known = skillCatalog.find((skill) => skill.name === name);
          return `<div class="as-plug-skill"><strong>${esc(name)}</strong><span>${esc(known && known.description || 'Not installed yet — it arrives with the plugin.')}</span></div>`;
        }).join('');
        return `<div class="as-plug-detail">
          <div class="as-plug-detail-head">
            <span class="as-plug-icon lg">${icon(plugin)}</span>
            <span class="as-plug-copy"><span class="as-plug-name">${esc(plugin.name)}</span>
              ${plugin.docs_url ? `<a class="as-plug-src" href="${esc(plugin.docs_url)}" target="_blank" rel="noreferrer">View source ↗</a>` : '<span class="as-plug-src muted">No source link</span>'}</span>
            ${on && plugin.enabled ? '<span class="as-plug-check lg" title="Connected" aria-label="Connected">✓</span>' : `<button class="as-plug-add solid" data-add="${esc(plugin.id)}">Add</button>`}
          </div>
          <p class="as-plug-detail-desc">${esc(plugin.description)}</p>
          ${plugin.note ? `<div class="as-plug-note">${esc(plugin.note)}</div>` : ''}
          <div class="as-plug-group">Connector</div>
          <div class="as-plug-panel"><code>${esc(connector)}</code></div>
          <div class="as-plug-group">Skills</div>
          <div class="as-plug-panel">${skills || '<span class="as-plug-muted">No skills bundled. The connector gives an agent the tools; a skill would tell it when to reach for them.</span>'}</div>
          ${blocked ? `<div class="as-plug-group">Connect</div><div class="as-plug-fields open">
            ${(plugin.transport === 'http' && !String(plugin.url || '').trim())
              ? `<input class="as-plug-input" data-key="url" placeholder="https://…/mcp" value="${esc(plugin.url || '')}">` : ''}
            ${(plugin.required_env || []).map((key) => `<input class="as-plug-input" data-key="env:${esc(key)}" placeholder="${esc(key)}" value="${esc((plugin.env || {})[key] || '')}">`).join('')}
            <button class="as-plug-save" data-save="${esc(plugin.id)}">Connect</button></div>` : ''}
        </div>`;
      };

      const paint = () => {
        const term = query.trim().toLowerCase();
        const matches = (plugin) => !term || `${plugin.name} ${plugin.description} ${plugin.category}`.toLowerCase().includes(term);
        const mine = held();
        const rows = catalog.filter(matches).filter((plugin) => (tab === 'yours' ? mine.has(plugin.id) : true));
        let body = '';
        if (tab === 'yours') {
          body = rows.length
            ? `<div class="as-plug-group">Installed</div><div class="as-plug-grid">${rows.map(rowMarkup).join('')}</div>`
            : '<div class="as-plug-empty">Nothing handed to this agent yet. Open Marketplace and add one.</div>';
        } else {
          const groups = [];
          for (const category of Array.from(new Set(rows.map((plugin) => plugin.category || 'Other')))) {
            const group = rows.filter((plugin) => (plugin.category || 'Other') === category);
            groups.push(`<div class="as-plug-group">${esc(category)}</div><div class="as-plug-grid">${group.map(rowMarkup).join('')}</div>`);
          }
          body = groups.join('') || '<div class="as-plug-empty">Nothing matches that.</div>';
        }
        const open = detail && catalog.find((item) => item.id === detail);
        if (open) {
          overlay.innerHTML = `<div class="as-plug" role="dialog" aria-label="${esc(open.name)}">
            <div class="as-plug-head"><button class="as-plug-back" data-back aria-label="Back">‹</button><h2>${esc(open.name)}</h2>
              <button class="as-plug-x" data-close aria-label="Close">✕</button></div>
            <div class="as-plug-body">${detailMarkup(open)}</div></div>`;
          wire();
          return;
        }
        overlay.innerHTML = `<div class="as-plug" role="dialog" aria-label="Plugins for @${esc(profile.handle)}">
          <div class="as-plug-head"><h2>Plugins</h2><button class="as-plug-x" data-close aria-label="Close">✕</button></div>
          <div class="as-plug-bar">
            <button class="as-plug-tab ${tab === 'marketplace' ? 'on' : ''}" data-tab="marketplace">Marketplace</button>
            <button class="as-plug-tab ${tab === 'yours' ? 'on' : ''}" data-tab="yours">Yours</button>
            <span style="flex:1"></span>
            <input class="as-plug-search" data-search placeholder="Search plugins" value="${esc(query)}" aria-label="Search plugins">
          </div>
          <div class="as-plug-body">${body}</div>
          <div class="as-plug-foot">Adding connects it to <strong>@${esc(profile.handle)}</strong>. Manage every plugin in the Plugins library.</div>
        </div>`;
        wire();
      };

      const wire = () => {
        overlay.querySelector('[data-close]').onclick = () => overlay.remove();
        const back = overlay.querySelector('[data-back]');
        if (back) back.onclick = () => { detail = null; paint(); };
        overlay.querySelectorAll('[data-tab]').forEach((button) => {
          button.onclick = () => { tab = button.dataset.tab; paint(); };
        });
        overlay.querySelectorAll('[data-open]').forEach((row) => {
          row.onclick = (event) => {
            if (event.target.closest('button, a, input')) return;
            detail = row.dataset.open; paint();
          };
        });
        const search = overlay.querySelector('[data-search]');
        if (search) search.oninput = () => { query = search.value; const at = search.selectionStart; paint();
          const next = overlay.querySelector('[data-search]'); next.focus(); next.setSelectionRange(at, at); };
        overlay.querySelectorAll('[data-add]').forEach((button) => {
          button.onclick = async () => {
            const plugin = catalog.find((item) => item.id === button.dataset.add);
            // Missing credential: ask for it HERE rather than sending him to
            // another page to come back from.
            if (blockedBy(plugin)) { expanded = plugin.id; paint(); return; }
            button.disabled = true; button.textContent = 'Connecting…';
            await connectPlugin(plugin);
            paint();
          };
        });
        overlay.querySelectorAll('[data-save]').forEach((button) => {
          button.onclick = async () => {
            const plugin = catalog.find((item) => item.id === button.dataset.save);
            // The inputs sit inside [data-row] in the list and outside it in
            // the detail view, so scope to whichever exists.
            const scope = overlay.querySelector(`[data-row="${plugin.id}"]`) || overlay;
            const values = {};
            let url = null;
            scope.querySelectorAll('[data-key]').forEach((input) => {
              const key = input.dataset.key;
              if (key.startsWith('env:')) values[key.slice(4)] = input.value;
              else if (key === 'url') url = input.value;
              else if (key.startsWith('header:')) values[key.slice(7)] = input.value;
            });
            button.disabled = true; button.textContent = 'Connecting…';
            const ok = await connectPlugin(plugin, values, url);
            if (ok) expanded = null; else expanded = plugin.id;
            paint();
          };
        });
      };

      paint();
      overlay.onclick = (event) => { if (event.target === overlay) overlay.remove(); };
      document.addEventListener('keydown', function escape(event) {
        if (event.key === 'Escape') { overlay.remove(); document.removeEventListener('keydown', escape); }
      });
      document.body.appendChild(overlay);
    };
    // The canvas is a split of the MAIN screen, next to the conversation —
    // the way Cockpit, Claude Desktop and ChatGPT show what they just made.
    // It was in the right rail first, which is 300px of chrome meant for
    // status, not for a diagram anyone has to read.
    activePaneCleanups.splice(0).forEach((cleanup) => { try { cleanup(); } catch (_) {} });
    const paneCleanups = activePaneCleanups;
    const stage = pane.querySelector('[data-stage]');
    const split = pane.querySelector('[data-split]');
    const canvasButton = pane.querySelector('[data-canvas]');
    let canvasPane = null;

    // The split holds ONE artifact at a time — the thing just made. Cockpit
    // does the same: a document or a canvas, never a stack of panes competing
    // for the same half of the screen. The cards in the thread bring the other
    // one back.
    let splitKind = null;
    // Closed means CLOSED. An artifact appearing again must not reopen a pane
    // he shut — the card in the thread is how it comes back. Remembered per
    // agent, so it survives switching away and back.
    const dismissKey = `xnaut-as-split-dismissed:${profile.handle}`;
    const dismissed = () => { try { return localStorage.getItem(dismissKey) === '1'; } catch (_) { return false; } };
    const setDismissed = (value) => { try { localStorage.setItem(dismissKey, value ? '1' : '0'); } catch (_) {} };
    const closeSplit = () => {
      setDismissed(true);
      if (canvasPane && canvasPane.dispose) canvasPane.dispose();
      canvasPane = null;
      splitKind = null;
      split.innerHTML = '';
      split.hidden = true;
      stage.classList.remove('split', 'split-full');
    };
    const closeCanvas = closeSplit;
    const openSplit = (kind, byHand) => {
      // Opening it deliberately (the card, the header button) clears the
      // dismissal; the agent redrawing does not.
      if (byHand) setDismissed(false);
      else if (dismissed()) return;
      const factory = kind === 'document' ? window.xnautCreateDocumentPane : window.xnautCreateCanvasPane;
      if (!factory) return;
      if (splitKind === kind) return;
      if (canvasPane && canvasPane.dispose) canvasPane.dispose();
      split.innerHTML = '';
      split.hidden = false;
      stage.classList.add('split');
      splitKind = kind;
      canvasPane = factory(profile.handle, split, {
        onFullScreen: () => stage.classList.toggle('split-full'),
        onClose: closeSplit,
        project: profile.default_project ? String(profile.default_project).split('/').pop() : 'xNAUT',
        author: `${profile.display_name} (@${profile.handle})`,
      });
    };
    const openCanvas = (byHand) => openSplit('canvas', byHand);
    const openDocument = (byHand) => openSplit('document', byHand);
    const canvasHasContent = async () => {
      const canvas = await invoke('canvas_get', { key: profile.handle }).catch(() => null);
      return !!(canvas && (canvas.nodes || []).length);
    };
    canvasHasContent().then((has) => {
      if (canvasButton) canvasButton.hidden = !has;
      if (has) openCanvas();
    });
    if (canvasButton) canvasButton.onclick = () => (canvasPane ? closeSplit() : openCanvas(true));
    // The agent drew something: show it without being asked.
    window.__xnautOpenCanvasSplit = () => openCanvas(true);
    window.__xnautOpenDocumentSplit = () => openDocument(true);
    const documentChanged = window.__TAURI__.event.listen('document-changed', async (event) => {
      if (!event || !event.payload || event.payload.key !== profile.handle) return;
      openDocument();
      const written = await invoke('document_get', { key: profile.handle }).catch(() => null);
      if (!written || !String(written.content || '').trim()) return;
      const last = (thread.messages || []).at(-1);
      if (last && last.kind === 'document' && last.title === written.title) return;
      thread = updateThread(profile.handle, thread.id, (next) => {
        next.messages.push({ id:`doc-${Date.now()}`, kind:'document', title:written.title,
          count:String(written.content).split(/\s+/).filter(Boolean).length, at:nowIso() });
        return next;
      });
      paintMessages();
    });
    paneCleanups.push(() => Promise.resolve(documentChanged).then((off) => { try { off(); } catch (_) {} }).catch(() => {}));
    const canvasChanged = window.__TAURI__.event.listen('canvas-changed', async (event) => {
      if (!event || !event.payload || event.payload.key !== profile.handle) return;
      if (canvasButton) canvasButton.hidden = false;
      openCanvas();
      // A card in the thread, so the drawing can be re-opened later without
      // hunting for it — the same affordance Cockpit puts under its answer.
      const canvas = await invoke('canvas_get', { key: profile.handle }).catch(() => null);
      if (!canvas || !(canvas.nodes || []).length) return;
      const last = (thread.messages || []).at(-1);
      if (last && last.kind === 'canvas' && last.title === canvas.title) return;
      thread = updateThread(profile.handle, thread.id, (next) => {
        next.messages.push({ id:`canvas-${Date.now()}`, kind:'canvas', title:canvas.title,
          count:(canvas.nodes || []).length, at:nowIso() });
        return next;
      });
      paintMessages();
    });
    paneCleanups.push(() => Promise.resolve(canvasChanged).then((off) => { try { off(); } catch (_) {} }).catch(() => {}));
    paneCleanups.push(() => {
      // Tearing the pane down on re-render is not him closing it.
      if (canvasPane && canvasPane.dispose) canvasPane.dispose();
      canvasPane = null;
      splitKind = null;
    });

    const attachButton = pane.querySelector('[data-attach]');
    if (attachButton) attachButton.onclick = () => openPlugins();
    // A plugin the agent tried to connect that wants a login: the card goes
    // into the thread, the way a person expects to be asked. One listener for
    // the module, pointed at whichever thread is open — registering it per
    // render leaked a subscription every time he clicked an agent.
    authTarget = {
      handle: profile.handle,
      append: (plugin) => {
        thread = updateThread(profile.handle, thread.id, (next) => {
          next.messages.push({ id:`auth-${Date.now()}`, kind:'auth', plugin, at:nowIso() });
          return next;
        });
        paintMessages();
      },
    };

    const projectNew = pane.querySelector('[data-project-new]');
    if (projectNew) projectNew.onclick = async () => {
      // Both choices, asked out loud. This used to be "new folder unless you
      // hold shift", which is a feature nobody finds — connecting a project
      // that already exists is the common case, not the hidden one.
      const chosen = await chooseProjectContext(pane, profile);
      if (chosen) window.xnautOpenAgentSpace(profile.handle, thread.id);
    };

    // A message is a QUESTION until proven otherwise. It goes to the agent's
    // own baseline model — no worktree, no zellij, no coding harness. The
    // harness starts only when the agent says the request needs one and the
    // owner names a repository (see buildHandshake).
    // The placeholders are UI, not conversation. 'Thinking…' was missing from
    // this list, so every turn shipped a trailing assistant message and the
    // Anthropic lane rejected the whole request as a prefill (XNAUT-217).
    const PLACEHOLDERS = new Set(['Working…', 'Thinking…']);
    const chatHistory = () => (thread.messages || [])
      .filter((message) => message.kind !== 'action' && message.text && !PLACEHOLDERS.has(message.text))
      .slice(-16)
      .map((message) => ({ role: message.role === 'user' ? 'user' : 'assistant', content: String(message.text) }));

    const submit = async (buildTask, buildPath) => {
      const text = buildTask || composer.value.trim();
      if (!text) return;
      // The guard is for a second click on Send, NOT for the internal handoff
      // from a chat turn into a build: that call arrives with send already
      // disabled and would otherwise return silently, which looks exactly
      // like a dead button.
      if (!buildTask && send.disabled) return;
      send.disabled = true;

      if (!buildTask) {
        const userMessageId = `m-${Date.now()}`;
        const firstUser = !(thread.messages || []).some((message) => message.role === 'user');
        thread = updateThread(profile.handle, thread.id, (next) => {
          next.title = firstUser ? text.replace(/\s+/g, ' ').slice(0, 48) : next.title;
          next.messages.push({ id: userMessageId, role: 'user', text, at: nowIso() });
          return next;
        });
        saveSharedMessage({ id: userMessageId, role: 'user', text, at: nowIso() });
        composer.value = '';
        const replyId = `a-${Date.now()}`;
        thread = updateThread(profile.handle, thread.id, (next) => {
          next.messages.push({ id: replyId, role: 'agent', text: 'Thinking…', at: nowIso() });
          return next;
        });
        paintMessages();
        try {
          // The Settings page still writes provider credentials to the legacy
          // webview store; the Rust registry only learns about them through
          // this sync. Without it a freshly-entered NautGate token reaches the
          // agent's chat turn as "provider is not configured".
          if (window.xnautSyncChatSettingsFromAiSettings) {
            await window.xnautSyncChatSettingsFromAiSettings().catch(() => false);
          }
          // The answer is painted as it is generated (XNAUT-159). What lands
          // here is PROVISIONAL: `reply` below is authoritative and replaces
          // it, so nothing downstream reads the live text.
          const requestId = `agent-chat-${Date.now()}`;
          let live = '';
          const paintLive = (delta) => {
            live += delta;
            const node = messages.querySelector(
              `[data-message-id="${replyId}"] .as-message-text`,
            );
            // ponytail: textContent, not paintMessages(). A full repaint per
            // token rewires every handler in the thread.
            if (node) node.textContent = live.replace(/^BUILD-REQUEST\n?/, '');
            scrollToEnd();
          };
          const stopStream = await listen('chat://chunk', (event) => {
            const payload = event.payload || {};
            if (payload.requestId !== requestId) return;
            paintLive(String(payload.delta || ''));
          });
          let reply;
          try {
            reply = String(await invoke('agent_chat_turn', {
              handle: profile.handle,
              requestId,
              messages: chatHistory(),
            }) || '').trim();
          } finally {
            try { stopStream(); } catch (_) {}
          }
          if (reply.startsWith('BUILD-REQUEST')) {
            const summary = reply.split('\n').slice(1).join('\n').trim();
            updateAgentMessage(replyId, summary || 'That needs a coding session.');
            // Asked once per thread. A thread that already has a workspace
            // continues in it: re-asking for the repository on every follow-up
            // ("now add sound") is the interrogation this flow exists to end.
            if (thread.workspace) {
              paintMessages();
              await submit(text, thread.workspace);
              return;
            }
            thread = updateThread(profile.handle, thread.id, (next) => {
              const message = next.messages.find((item) => item.id === replyId);
              if (message) message.build_task = text;
              return next;
            });
            paintMessages();
          } else {
            updateAgentMessage(replyId, reply || 'No answer came back.');
          }
        } catch (error) {
          updateAgentMessage(replyId, `Could not answer: ${String(error)}`);
        }
        send.disabled = false;
        return;
      }

      let worktreePath = buildPath || profile.default_project;
      if (!worktreePath) {
        // Not every message is a coding run: a question needs no repository,
        // and interrogating the owner before they can type is an obstacle,
        // not a safety feature. Fall back to the agent's own bounded scratch
        // folder — never home — and let the header button point it at a real
        // project whenever that matters.
        worktreePath = await invoke('agent_scratch_workspace', { handle: profile.handle }).catch(() => null);
        if (!worktreePath) { send.disabled = false; return; }
        // A previous fallback launch may be sitting at a trust prompt in the
        // home directory. Never reuse that broad-scoped session after the user
        // has selected the real project.
        if (sessionId) await invoke('agent_session_interrupt', { sessionId }).catch(() => {});
        sessionId = null;
        thread = updateThread(profile.handle, thread.id, (next) => {
          next.session_id = null;
          next.conversation_id = null;
          return next;
        });
      }
      const handoff = thread.conversation_id ? '' : portableHandoff(profile, thread);
      const runtimePrompt = handoff
        ? `${handoff}\n\nLATEST USER REQUEST\n${text}`
        : text;
      const messageId = `a-${Date.now()}`;
      thread = updateThread(profile.handle, thread.id, (next) => {
        // Where it is building is the one fact a build thread must state.
        // "Working…" with no location is how a run in the wrong directory
        // goes unnoticed until it has written something.
        next.messages.push({ id:`x-${Date.now()}`, kind:'action', label:'Building in', detail:worktreePath, at:nowIso() });
        next.messages.push({ id:messageId, role:'agent', text:'Working…', at:nowIso() });
        return next;
      });
      paintMessages();
      try {
        const response = await invoke('agent_profile_launch', { req: {
          handle: profile.handle,
          worktree_path: worktreePath,
          prompt: runtimePrompt,
          conversation_mode: true,
          conversation_id: thread.conversation_id || null,
          resume: !!thread.conversation_id,
          cols: 200,
          rows: 30,
          runtime_id: threadRuntime,
        } });
        sessionId = response.session_id;
        thread = updateThread(profile.handle, thread.id, (next) => {
          next.session_id = response.session_id;
          if (response.conversation_id) next.conversation_id = response.conversation_id;
          const message = next.messages.find((item) => item.id === messageId);
          if (message) message.session_id = response.session_id;
          return next;
        });
        showTerminal(response.session_id);
        announceProfilesChanged(profile);
        paintMessages();
        if (response.output_path) {
          await captureRunFile(response.output_path, messageId, response.zellij_session);
        }
        else await captureStructuredTurn(response.session_id, messageId);
      } catch (error) {
        updateAgentMessage(messageId, `Could not start: ${String(error)}`);
        send.disabled = false;
      }
    };
    // NEVER `send.onclick = submit`: the click handler is called with the
    // PointerEvent, which lands in submit's first parameter (buildTask) and
    // becomes the prompt. NautBot received the literal string
    // "[object PointerEvent]" and, to its credit, refused to act on it.
    // Enter went through submit() with no arguments and worked, which is why
    // this looked intermittent rather than broken.
    send.onclick = () => submit();
    composer.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(); }
    });
    const harness = pane.querySelector('[data-harness]');
    if (harness) harness.onchange = async () => {
      const next = harness.value;
      if (next === threadRuntime) return;
      // The old CLI's conversation id means nothing to the new one, and its PTY
      // is running the old binary. Drop both. The workdir is derived from the
      // profile, not the session, so it survives untouched. Clearing
      // conversation_id is what makes the next send fall into the
      // portableHandoff branch, which is how the transcript reaches the new
      // harness. XNAUT-150.
      if (sessionId) await invoke('agent_session_interrupt', { sessionId }).catch(() => {});
      sessionId = null;
      threadRuntime = next;
      thread = updateThread(profile.handle, thread.id, (item) => {
        item.runtime_id = next;
        item.session_id = null;
        item.conversation_id = null;
        item.messages.push({ id:`x-${Date.now()}`, kind:'action', label:'Harness switched to', detail:next, at:nowIso() });
        return item;
      });
      if (terminalButton) terminalButton.hidden = true;
      // The composer locks for the duration of a run and is only unlocked by
      // that run finishing. The run we just interrupted never will, so without
      // this the thread is switched and permanently unable to send.
      send.disabled = false;
      paintMessages();
    };
    pane.querySelector('[data-settings]').onclick = () => window.xnautOpenAgentSettings(profile.handle);
    if (terminalButton) terminalButton.onclick = async () => {
      // The run outlives the app, but the PTY watching it does not. A stored
      // session id from a previous launch points at a dead viewport — which is
      // exactly what "it did not attach" looked like. Reattach to the live
      // zellij session first, and only fall back to the old id.
      const attached = await invoke('agent_session_attach', { handle: profile.handle, cols: 120, rows: 30 }).catch(() => null);
      const target = attached || terminalButton.dataset.sessionId || sessionId;
      if (target) {
        showTerminal(target);
        window.xnautOpenAgentSession(target, profile.display_name);
      }
    };
    messages.addEventListener('contextmenu', (event) => {
      const message = event.target.closest('[data-message-id]');
      if (!message) return;
      event.preventDefault();
      const record = (thread.messages || []).find((item) => item.id === message.dataset.messageId);
      const menu = document.createElement('div');
      menu.className = 'as-menu';
      menu.innerHTML = '<button data-copy>Copy</button><button data-branch>Start a thread</button>';
      document.body.appendChild(menu);
      window.xnautPlaceAtClick(menu, event.clientX, event.clientY);
      const close = () => menu.remove();
      menu.querySelector('[data-copy]').onclick = () => { navigator.clipboard && navigator.clipboard.writeText(record.text); close(); };
      menu.querySelector('[data-branch]').onclick = () => {
        const branch = newThread(profile.handle, String(record.text || '').slice(0, 48));
        branch.messages.push({ ...record, id: `m-${Date.now()}` }); writeThread(profile.handle, branch);
        close(); window.xnautOpenAgentSpace(profile.handle, branch.id);
      };
      setTimeout(() => document.addEventListener('mousedown', (click) => {
        if (!menu.contains(click.target)) close();
      }, { once:true }), 0);
    });
    wireLibrary(pane, profiles, profile.handle);
  }

  async function renderProfileForm(pane, options) {
    const editing = options.mode === 'settings';
    const [profiles, runtimes, availableSkills, sessions, pluginCatalog] = await Promise.all([
      invoke('agent_profile_list').catch(() => []),
      invoke('agent_list').catch(() => []),
      invoke('skill_list').catch(() => []),
      invoke('agent_sessions_list').catch(() => []),
      invoke('plugin_catalog').catch(() => []),
    ]);
    await refreshLibraryNotes();
    const original = editing ? (profiles || []).find((item) => item.handle === handleOf(options.handle)) : null;
    if (editing && !original) { pane.innerHTML = '<div class="as-empty"><h2>Agent not found.</h2></div>'; return; }
    const profile = original || profilePayload({
      handle:'', display_name:'', tagline:'', purpose:'', runtime_id:(runtimes[0] && runtimes[0].id) || '',
      provider:'global', model:'', reasoning_effort:'', execution:'local', role:'coding-agent', skills:[], notifications:true,
    });
    const selectedSkills = new Set((profile.capabilities || []).filter((item) => String(item).startsWith('skill:')).map((item) => String(item).slice(6)));
    const selectedCollabs = new Set((profile.capabilities || []).filter((item) => String(item).startsWith('collab:')).map((item) => String(item).slice(7)));
    const modelCatalog = window.xnautModelCatalog ? window.xnautModelCatalog.all() : [];
    const modelOptions = modelCatalog.slice();
    if (profile.model && !modelOptions.some((item) => item.id === profile.model && item.provider === profile.provider)) {
      modelOptions.unshift({ id:profile.model, name:profile.model, provider:profile.provider });
    }
    const providers = Array.from(new Set(['global', profile.provider, ...modelCatalog.map((item) => item.provider)].filter(Boolean)));
    const libraryProfiles = pinNautbotFirst(profiles || []);
    pane.innerHTML = `${libraryMarkup(libraryProfiles, sessions || [], original && original.handle, null)}<div class="as-stage"><div class="as-body"><form class="as-form-page" data-form>
      <div class="as-form-intro"><h1>${editing ? 'Agent settings.' : 'Create a new agent.'}</h1>
        <p>${editing ? 'Identity, runtime, and permissions for this agent.' : 'Give the agent a durable identity, then choose how it runs.'}</p></div>
      <div class="as-tabs" role="tablist">
        <button type="button" class="as-tab as-tab-on" data-tab="setup" role="tab">Setup</button>
        <button type="button" class="as-tab" data-tab="prompt" role="tab">Prompt</button>
        <button type="button" class="as-tab" data-tab="capabilities" role="tab">Capabilities</button>
        <button type="button" class="as-tab" data-tab="collaborators" role="tab">Collaborators</button>
      </div>
      <div class="as-grid"><div class="as-card">
        <div class="as-tabpane" data-tabpane="setup">
        <div class="as-inline"><label class="as-field"><span>Name</span><input class="as-input" name="display_name" value="${esc(profile.display_name)}" placeholder="Builder"></label>
          <label class="as-field"><span>@Handle</span><input class="as-input" name="handle" value="${esc(profile.handle)}" ${profile.handle === 'nautbot' ? 'readonly' : ''} placeholder="builder"><small class="as-help">Unique · letters, numbers, - or _</small></label></div>
        <label class="as-field"><span>Tagline</span><input class="as-input" name="tagline" maxlength="72" value="${esc(profile.tagline)}" placeholder="Turns clear product intent into working software."></label>
        <div class="as-inline"><label class="as-field"><span>Runtime</span><select class="as-input" name="runtime_id">${(runtimes || []).map((runtime) => `<option value="${esc(runtime.id)}" ${runtime.id === profile.runtime_id ? 'selected' : ''} ${runtime.available === false && runtime.id !== profile.runtime_id ? 'disabled' : ''}>${esc(runtime.label)}${runtime.available === false ? ' · unavailable' : ''}</option>`).join('')}</select></label>
          <label class="as-field"><span>Compute</span><select class="as-input" name="execution"><option value="local" ${profile.execution !== 'sandbox' ? 'selected' : ''}>Local</option><option value="sandbox" ${profile.execution === 'sandbox' ? 'selected' : ''}>Sandbox</option></select></label></div>
        <div class="as-inline"><label class="as-field"><span>Provider</span><select class="as-input" name="provider">${providers.map((provider) => `<option value="${esc(provider)}" ${provider === profile.provider ? 'selected' : ''}>${esc(provider)}</option>`).join('')}</select></label>
          <label class="as-field"><span>Model</span><select class="as-input" name="model"><option value="">Runtime default</option>${modelOptions.map((model) => `<option data-provider="${esc(model.provider)}" value="${esc(model.id)}" ${model.id === profile.model && model.provider === profile.provider ? 'selected' : ''}>${esc(model.name || model.id)}</option>`).join('')}</select><small class="as-help">Handed to the runtime CLI as --model.</small></label>
          <label class="as-field"><span>Chat model</span><select class="as-input" name="chat_model"><option value="">Same as Model</option>${modelOptions.map((model) => `<option value="${esc(model.id)}" ${model.id === profile.chat_model ? 'selected' : ''}>${esc(model.name || model.id)}</option>`).join('')}</select><small class="as-help">Used for chat in the app. Only this one has to carry tool calls.</small></label>
          <label class="as-field"><span>Tool calls</span><button type="button" class="as-button" data-toolcheck>Check this route</button><small class="as-help" data-toolcheck-result>Asks the provider whether the chat model can actually run one.</small></label></div>
        <label class="as-field"><span>Reasoning effort</span><select class="as-input" name="reasoning_effort"><option value="" ${!profile.reasoning_effort ? 'selected' : ''}>Model default</option>${['low','medium','high','xhigh'].map((effort) => `<option value="${effort}" ${profile.reasoning_effort === effort ? 'selected' : ''}>${effort}</option>`).join('')}</select></label>
        <label class="as-field"><span>Role</span><input class="as-input" name="role" value="${esc(profile.role)}"></label>
        <label class="as-field"><span>Accent</span><input class="as-input" name="accent_color" type="color" value="${esc(profile.accent_color || '#f5b840')}"></label>
        </div>

        <div class="as-tabpane" data-tabpane="prompt" hidden>
          <div class="as-foundation">
            <div class="as-foundation-head" data-foundation-toggle>
              <span class="as-foundation-caret" data-foundation-caret>▸</span>
              <span class="as-foundation-title">xNAUT Foundation</span>
              <span class="as-foundation-badge" data-foundation-version>…</span>
              <span class="as-foundation-ro">Read-only</span>
              <span class="as-foundation-note">Sits above your instructions</span>
            </div>
            <pre class="as-foundation-body" data-foundation-body hidden>Loading…</pre>
          </div>
          <label class="as-field"><span>Your agent instructions</span>
            <textarea class="as-input as-prompt" name="purpose" placeholder="# Builder&#10;&#10;You are… — persona, goals, and domain rules.">${esc(profile.purpose)}</textarea>
            <small class="as-help">Sits on top of the Foundation above. Define this agent's persona, goals and domain rules here.</small></label>
        </div>

        <div class="as-tabpane" data-tabpane="capabilities" hidden>
          <p class="as-help" style="margin:0 0 12px">Capabilities are modular, inspectable and revocable. Every grant is scoped to this agent.</p>
          <div class="as-tiles">

            <div class="as-tile" data-tile="skills">
              <div class="as-tile-head"><span class="as-tile-name">Skills &amp; instructions</span>
                <span class="as-tile-state" data-tile-state="skills">${selectedSkills.size ? `${selectedSkills.size} on` : 'none'}</span></div>
              <div class="as-tile-sub">Skills · role description · starter actions</div>
              <div class="as-tile-body" hidden>
                <div class="as-chips" data-skills>${(availableSkills || []).slice(0, 40).map((skill) => `<button type="button" class="as-chip ${selectedSkills.has(skill) ? 'selected' : ''}" data-skill="${esc(skill)}">${esc(skill)}</button>`).join('') || '<span class="as-help">No skills yet — add one in the Skills library.</span>'}</div>
                <small class="as-help">Add or edit skills in the Skills library; enable them per agent here.</small>
              </div>
            </div>

            <div class="as-tile" data-tile="plugins">
              <div class="as-tile-head"><span class="as-tile-name">Plugins</span>
                <span class="as-tile-state" data-tile-state="plugins">${(() => {
                  const held = (profile.capabilities || []).filter((item) => String(item).startsWith('plugin:')).length;
                  return held ? `${held} on` : 'none';
                })()}</span></div>
              <div class="as-tile-sub">MCP servers this agent gets in a build run</div>
              <div class="as-tile-body" hidden>
                <div class="as-chips">${(() => {
                  const held = new Set((profile.capabilities || []).filter((item) => String(item).startsWith('plugin:')).map((item) => String(item).slice(7)));
                  const mark = (plugin) => (window.xnautPluginIconFor ? window.xnautPluginIconFor(plugin) : '');
                  const rows = (pluginCatalog || []).filter((plugin) => held.has(plugin.id));
                  return rows.map((plugin) => `<span class="as-chip selected"><span class="as-chip-icon">${mark(plugin)}</span>${esc(plugin.name)}</span>`).join('')
                    || '<span class="as-help">None yet. Use + in the agent header to hand this agent a plugin.</span>';
                })()}</div>
                <small class="as-help">A plugin is configured once in the Plugins library, then handed to an agent with + in its header. A chat turn opens them too, so its tools include theirs; a build run gets them as MCP servers.</small>
              </div>
            </div>

            <div class="as-tile" data-tile="computer">
              <div class="as-tile-head"><span class="as-tile-name">Local computer</span>
                <span class="as-tile-state" data-tile-state="computer">${esc(profile.policy && profile.policy.filesystem || 'workspace-write')}</span></div>
              <div class="as-tile-sub">Files · shell · web · isolated workspace</div>
              <div class="as-tile-body" hidden>
                <div class="as-inline">
                  <label class="as-field"><span>Filesystem</span>
                    <select class="as-input" name="policy_filesystem">
                      <option value="read-only" ${(profile.policy && profile.policy.filesystem) === 'read-only' ? 'selected' : ''}>Read only</option>
                      <option value="workspace-write" ${!profile.policy || profile.policy.filesystem === 'workspace-write' ? 'selected' : ''}>Write inside the project</option>
                      <option value="full" ${(profile.policy && profile.policy.filesystem) === 'full' ? 'selected' : ''}>Full access</option>
                    </select><small class="as-help" data-enf="filesystem"></small></label>
                  <label class="as-field"><span>Network</span>
                    <select class="as-input" name="policy_network">
                      <option value="any" ${!profile.policy || profile.policy.network === 'any' ? 'selected' : ''}>Any</option>
                      <option value="none" ${(profile.policy && profile.policy.network) === 'none' ? 'selected' : ''}>None</option>
                    </select><small class="as-help" data-enf="network"></small></label>
                </div>
                <label class="as-field" style="flex-direction:row;align-items:center;gap:8px"><input type="checkbox" name="policy_shell" ${!profile.policy || profile.policy.shell !== false ? 'checked' : ''}><span>Shell commands</span><small class="as-help" data-enf="shell"></small></label>
                <label class="as-field" style="flex-direction:row;align-items:center;gap:8px"><input type="checkbox" name="policy_web_fetch" ${!profile.policy || profile.policy.web_fetch !== false ? 'checked' : ''}><span>Fetch web pages</span><small class="as-help" data-enf="web_fetch"></small></label>
                <label class="as-field" style="flex-direction:row;align-items:center;gap:8px"><input type="checkbox" name="policy_web_search" ${!profile.policy || profile.policy.web_search !== false ? 'checked' : ''}><span>Web search</span><small class="as-help" data-enf="web_search"></small></label>
                <small class="as-help">A rule marked <b>enforced</b> is a launch flag the CLI itself obeys, so the tool is absent from the run. <b>Advisory</b> means the prompt asks and the agent can still choose.</small>
              </div>
            </div>

            <div class="as-tile" data-tile="triggers">
              <div class="as-tile-head"><span class="as-tile-name">Triggers &amp; automations</span>
                <span class="as-tile-state">open</span></div>
              <div class="as-tile-sub">Schedules · app events · webhooks · approvals</div>
              <div class="as-tile-body" hidden>
                <button type="button" class="as-button" data-open-automations>Open Automations</button>
                <small class="as-help">Automations run agents on a schedule or an event; they live in their own surface.</small>
              </div>
            </div>

          </div>
        </div>

        <div class="as-tabpane" data-tabpane="collaborators" hidden>
          <div class="as-field"><span class="as-section-label">May hand off to</span>
            <div class="as-chips" data-collabs>${libraryProfiles.filter((item) => item.handle !== profile.handle).map((item) => `<button type="button" class="as-chip ${selectedCollabs.has(item.handle) ? 'selected' : ''}" data-collab="${esc(item.handle)}">@${esc(item.handle)}</button>`).join('') || '<span class="as-help">No other agents yet.</span>'}</div>
            <small class="as-help">Named in this agent's prompt as the ones it may hand work to. Advisory: nothing blocks a hand-off, so a determined agent can still ask someone else.</small></div>
          <label class="as-field" style="flex-direction:row;align-items:center"><input name="notifications" type="checkbox" ${profile.notifications !== false ? 'checked' : ''}><span>Notify me when this agent needs attention</span></label>
        </div>
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
      const state = pane.querySelector('[data-tile-state="skills"]');
      if (state) state.textContent = skills.size ? `${skills.size} on` : 'none';
    });
    const collabs = new Set(selectedCollabs);
    pane.querySelectorAll('[data-collab]').forEach((button) => button.onclick = () => {
      const handle = button.dataset.collab;
      if (collabs.has(handle)) collabs.delete(handle); else collabs.add(handle);
      button.classList.toggle('selected', collabs.has(handle));
    });

    // Capability tiles: click the head to expand. Panes keep their state, so
    // opening one does not reset a half-made choice elsewhere.
    pane.querySelectorAll('[data-tile]').forEach((tile) => {
      const head = tile.querySelector('.as-tile-head');
      const sub = tile.querySelector('.as-tile-sub');
      const body = tile.querySelector('.as-tile-body');
      const toggle = () => { if (body) body.hidden = !body.hidden; };
      if (head) head.onclick = toggle;
      if (sub) sub.onclick = toggle;
    });
    const automations = pane.querySelector('[data-open-automations]');
    if (automations) automations.onclick = () => window.xnautAttachAutomationsTab && window.xnautAttachAutomationsTab();

    // Which rows genuinely enforce depends on the runtime, and Rust owns that
    // table — a mirrored copy here would drift into an overclaim.
    const paintEnforcement = async () => {
      const runtime = (form.elements.runtime_id && form.elements.runtime_id.value) || profile.runtime_id || '';
      let table = {};
      try { table = (await invoke('policy_enforcement', { runtimeId: runtime })) || {}; } catch (_) { table = {}; }
      pane.querySelectorAll('[data-enf]').forEach((el) => {
        const level = table[el.dataset.enf] || 'advisory';
        el.innerHTML = `<span class="as-enf ${level}">${level}</span>`;
      });
      const state = pane.querySelector('[data-tile-state="computer"]');
      if (state && form.elements.policy_filesystem) state.textContent = form.elements.policy_filesystem.value;
    };
    paintEnforcement();
    form.addEventListener('change', paintEnforcement);

    // Horizontal second-layer menu. Panes stay in the DOM so a half-typed
    // prompt survives a tab switch; only visibility changes.
    pane.querySelectorAll('[data-tab]').forEach((tab) => tab.onclick = () => {
      const key = tab.dataset.tab;
      pane.querySelectorAll('[data-tab]').forEach((other) => other.classList.toggle('as-tab-on', other === tab));
      pane.querySelectorAll('[data-tabpane]').forEach((paneEl) => { paneEl.hidden = paneEl.dataset.tabpane !== key; });
    });

    // Can this route actually run a tool call? (XNAUT-196)
    //
    // The picker lists every model the gateway reports and none of them say
    // whether a tool call survives the trip. Losing that bet looks like an
    // agent that answers in prose and claims a tool "isn't available", which
    // took four days to trace once already (XNAUT-195). One request settles it.
    (() => {
      const button = pane.querySelector('[data-toolcheck]');
      const result = pane.querySelector('[data-toolcheck-result]');
      if (!button || !result) return;
      button.onclick = async () => {
        const form = button.closest('form') || pane;
        const provider = (form.querySelector('[name="provider"]') || {}).value || '';
        const chosen = (form.querySelector('[name="chat_model"]') || {}).value
          || (form.querySelector('[name="model"]') || {}).value || '';
        if (!chosen) { result.textContent = 'Pick a model first.'; return; }
        button.disabled = true;
        result.textContent = `Asking ${provider || 'the default provider'} about ${chosen}…`;
        try {
          const support = await invoke('model_tool_support', { provider, model: chosen, refresh: true });
          // The upstream's own sentence, verbatim: it is what says whether the
          // fix is a billing page, a key, or a different model.
          result.textContent = support.supported
            ? `✓ ${chosen} can run tool calls on ${provider || 'the default provider'}.`
            : `✗ ${chosen} cannot run tool calls here. ${support.reason}`;
        } catch (error) {
          result.textContent = `Could not check: ${String(error)}`;
        } finally {
          button.disabled = false;
        }
      };
    })();

    // The Foundation is read-only and shared: fetched, never edited here.
    (async () => {
      const body = pane.querySelector('[data-foundation-body]');
      const version = pane.querySelector('[data-foundation-version]');
      const head = pane.querySelector('[data-foundation-toggle]');
      const caret = pane.querySelector('[data-foundation-caret]');
      if (!body || !head) return;
      head.onclick = () => {
        body.hidden = !body.hidden;
        if (caret) caret.textContent = body.hidden ? '▸' : '▾';
      };
      try {
        let hookUrl = null;
        try { hookUrl = await invoke('agent_hooks_url'); } catch (_) { hookUrl = null; }
        const foundation = await invoke('foundation_prompt', { hookUrl });
        if (version) version.textContent = foundation.version || '';
        body.textContent = foundation.text || '';
      } catch (error) {
        body.textContent = 'The foundation prompt could not be loaded.';
        console.error('[agent-space] foundation load failed:', error);
      }
    })();
    pane.querySelector('[data-cancel]').onclick = () => editing ? window.xnautOpenAgentSpace(profile.handle) : window.xnautOpenAgentSpace();
    form.onsubmit = async (event) => {
      event.preventDefault();
      const values = Object.fromEntries(new FormData(form).entries());
      values.notifications = form.elements.notifications.checked;
      values.skills = skills;
      values.collabs = collabs;
      values.policy = {
        filesystem: values.policy_filesystem || 'workspace-write',
        network: values.policy_network || 'any',
        network_hosts: (original && original.policy && original.policy.network_hosts) || [],
        extra_roots: (original && original.policy && original.policy.extra_roots) || [],
        shell: form.elements.policy_shell ? form.elements.policy_shell.checked : true,
        web_fetch: form.elements.policy_web_fetch ? form.elements.policy_web_fetch.checked : true,
        web_search: form.elements.policy_web_search ? form.elements.policy_web_search.checked : true,
      };
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
      if (!await confirmDialog(`Delete ${esc(profile.display_name)} (@${esc(profile.handle)})? Conversation history remains local.`, 'Delete')) return;
      try { await invoke('agent_profile_delete', { handle: profile.handle, rel: null }); announceProfilesChanged(); window.xnautOpenAgentSpace(); }
      catch (error) { pane.querySelector('[data-error]').textContent = String(error); }
    };
    wireLibrary(pane, libraryProfiles, original && original.handle);
  }

  // Which thread a sign-in card should land in. Set by whichever thread is
  // currently rendered. One listener for the module: registering it per render
  // leaked a subscription every time an agent was clicked.
  let authTarget = null;
  // Listeners and panes that belong to the thread currently on screen. Run
  // and cleared on every re-render, or each click on an agent leaves another
  // canvas listener behind.
  let activePaneCleanups = [];
  // An agent asked for the knowledge graph. It opens as a tab, the same one
  // the menu opens, rather than a second viewer nobody maintains.
  if (window.__TAURI__ && window.__TAURI__.event) {
    // An agent asked to watch a zellij session that is already running.
    window.__TAURI__.event.listen('attach-zellij-session', async (event) => {
      const session = event && event.payload && event.payload.session;
      if (!session) return;
      const sessionId = await window.__TAURI__.core
        .invoke('create_terminal_session', { config: { cols: 160, rows: 40, session_name: session } })
        .catch((error) => { console.error('[agent-space] attach failed:', error); return null; });
      if (sessionId && window.xnautAttachAgentTab) {
        window.xnautAttachAgentTab(sessionId.session_id || sessionId, session, session);
      }
    }).catch((error) => console.error('[agent-space] attach listener failed:', error));
    window.__TAURI__.event.listen('open-graph', () => {
      if (window.xnautAttachGraphTab) window.xnautAttachGraphTab({});
    }).catch((error) => console.error('[agent-space] graph listener failed:', error));
  }
  if (window.__TAURI__ && window.__TAURI__.event) {
    window.__TAURI__.event.listen('plugin-needs-auth', (event) => {
      const payload = (event && event.payload) || {};
      if (!payload.plugin || !authTarget || payload.agent_id !== authTarget.handle) return;
      authTarget.append(payload.plugin);
    }).catch((error) => console.error('[agent-space] auth card listener failed:', error));
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
  // xnautPromptDialog / xnautConfirmDialog are owned by dialogs.js now.
  // Re-exporting the local aliases here made them call themselves.
  window.xnautCreateAgentSpacePanel = createAgentSpacePanel;
  // The highlighted agent is the one the main agent icon serves: opening Agent
  // Space with no handle returns to whoever you were last talking to, rather
  // than resetting to the top of the list.
  const ACTIVE_KEY = 'xnaut-as-active-handle';
  function rememberActive(handle) {
    try { if (handle) localStorage.setItem(ACTIVE_KEY, handle); } catch (_) {}
  }
  function lastActive() {
    try { return localStorage.getItem(ACTIVE_KEY) || ''; } catch (_) { return ''; }
  }
  window.xnautOpenAgentSpace = (handle, threadId, newThreadRequested) => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    const chosen = handleOf(handle) || lastActive();
    rememberActive(chosen);
    return window.xnautAttachSingletonPanelTab('Agent Space', 'xnautCreateAgentSpacePanel', { mode:'thread', handle:chosen, threadId:threadId || null, newThread:!!newThreadRequested });
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
