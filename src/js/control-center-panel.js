// Control Center — the quiet default landing page. It uses real session state
// for feedback and hands open-ended prompts to the existing chat system until
// NautBot's dedicated orchestration layer lands in XNAUT-146.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);

  function ensureStyles() {
    if (document.getElementById('control-center-styles')) return;
    const style = document.createElement('style');
    style.id = 'control-center-styles';
    style.textContent = `
      .control-center { --cc-accent:#f5b840; position:relative; display:flex; flex:1 1 auto; min-width:0; min-height:0;
        overflow:hidden; color:var(--text-primary,#ededf1); background:var(--bg-primary,#101014);
        font-family:var(--font-sans,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); }
      .control-center * { box-sizing:border-box; }
      .cc-stage { width:min(820px,calc(100% - 56px)); margin:auto; padding:48px 0 110px; }
      .cc-date { margin-bottom:14px; color:var(--text-secondary,#858590); font-size:10px; font-weight:720; letter-spacing:.11em; text-transform:uppercase; }
      .cc-greeting { margin:0; color:var(--text-primary,#f2f2f5); font-size:clamp(28px,4vw,42px); font-weight:620; letter-spacing:-.035em; line-height:1.08; }
      .cc-feedback { margin:12px 0 32px; color:var(--text-secondary,#9a9aa4); font-size:14px; }
      .cc-composer { display:flex; gap:9px; padding:10px; border:1px solid var(--border-color,var(--border,#373740)); border-radius:12px;
        background:var(--editor-surface,#1a1a1f); box-shadow:0 18px 50px rgba(0,0,0,.26); }
      .cc-composer textarea { flex:1; min-height:52px; max-height:150px; padding:11px 12px; resize:none; border:0; outline:0;
        color:var(--text-primary,#f0f0f3); background:transparent; font:inherit; font-size:14px; line-height:1.45; }
      .cc-composer textarea::placeholder { color:#74747f; }
      .cc-send { width:40px; height:40px; align-self:flex-end; border:0; border-radius:9px; color:#17140b; background:var(--cc-accent);
        font-size:19px; font-weight:700; cursor:pointer; }.cc-send:hover { filter:brightness(1.07); }.cc-send:disabled { opacity:.5; cursor:default; }
      .cc-composer-meta { display:flex; justify-content:space-between; gap:10px; margin-top:8px; color:var(--text-secondary,#777781); font-size:9px; font-weight:680; letter-spacing:.08em; text-transform:uppercase; }
      .cc-actions { display:flex; flex-wrap:wrap; gap:7px; margin-top:19px; }
      .cc-action { padding:7px 10px; border:1px solid var(--border-color,#35353d); border-radius:99px; color:var(--text-secondary,#aaaab3);
        background:transparent; font:inherit; font-size:11px; cursor:pointer; }.cc-action:hover { border-color:#5a5a64; color:var(--text-primary,#eeeef2); background:rgba(255,255,255,.04); }
      .cc-pulse { display:inline-block; width:6px; height:6px; margin-right:7px; border-radius:50%; background:#6c6c76; vertical-align:1px; }
      .cc-pulse.live { background:#4da3ff; }.cc-pulse.attention { background:#ff5f56; box-shadow:0 0 0 3px rgba(255,95,86,.14); }
      @media (max-width:640px) { .cc-stage { width:calc(100% - 30px); padding-top:30px; }.cc-greeting { font-size:28px; } }
    `;
    document.head.appendChild(style);
  }

  function partOfDay() {
    const hour = new Date().getHours();
    if (hour < 12) return 'morning';
    if (hour < 18) return 'afternoon';
    return 'evening';
  }

  function dateLabel() {
    return new Intl.DateTimeFormat(undefined, { weekday:'long', day:'numeric', month:'long' }).format(new Date());
  }

  function userName() {
    try { return String(localStorage.getItem('xnaut-user-name') || '').trim(); } catch (_) { return ''; }
  }

  function greeting() {
    const name = userName();
    return `Good ${partOfDay()}${name ? `, ${name}` : ''}.`;
  }

  function activityFeedback(sessions) {
    const active = (sessions || []).filter((session) => session && session.agent_id);
    const handles = new Set(active.map((session) => session.agent_id));
    const needsAttention = active.filter((session) => ['permission', 'blocked'].includes(session.status));
    const working = active.filter((session) => session.status === 'working');
    if (needsAttention.length) return { tone:'attention', text:`${needsAttention.length} agent${needsAttention.length === 1 ? '' : 's'} need your attention.` };
    if (working.length) return { tone:'live', text:`${working.length} agent${working.length === 1 ? '' : 's'} working. ${handles.size} online.` };
    if (handles.size) return { tone:'live', text:`${handles.size} agent${handles.size === 1 ? '' : 's'} online. It is quiet.` };
    return { tone:'', text:'No agents running. It is quiet.' };
  }

  function routePrompt(text) {
    const normalized = text.trim().toLowerCase();
    if (/^(create|add|new) (an? )?agent\b/.test(normalized) || normalized === '/new-agent') {
      window.xnautOpenNewAgent && window.xnautOpenNewAgent(); return;
    }
    if (/^(open|show) (the )?(agent space|agents|agent library)\b/.test(normalized) || normalized === '/agents') {
      window.xnautOpenAgentSpace && window.xnautOpenAgentSpace(); return;
    }
    if (/^(open|show) (the )?observatory\b/.test(normalized) || normalized === '/observatory') {
      window.xnautAttachObservatoryTab && window.xnautAttachObservatoryTab(); return;
    }
    if (window.xnautAttachChatTab) {
      window.xnautAttachChatTab({
        title:'NautBot',
        chatKey:`control-center:${Date.now()}`,
        prefill:text,
        autoSend:true,
        systemPromptAppend:'You are NautBot, xNaut\'s local-first guide. Help the user understand and operate xNaut. Prefer reversible actions and explain clearly when an action requires another agent.',
      });
    }
  }

  async function createControlCenterPanel(tabId, parent) {
    ensureStyles();
    const pane = document.createElement('section');
    pane.className = 'control-center'; pane.style.flex = '1'; pane.dataset.controlCenter = '1';
    pane.innerHTML = `<main class="cc-stage"><div class="cc-date" data-date></div><h1 class="cc-greeting" data-greeting></h1><p class="cc-feedback" data-feedback></p>
      <div class="cc-composer"><textarea rows="2" data-compose aria-label="Ask NautBot" placeholder="Ask NautBot, or message any agent…"></textarea><button class="cc-send" data-send aria-label="Send to NautBot">↑</button></div>
      <div class="cc-composer-meta"><span>@ mention · local first</span><span>Enter to send · Shift Enter for a line break</span></div>
      <div class="cc-actions" aria-label="Quick actions"><button class="cc-action" data-action="new-agent">Create an agent</button><button class="cc-action" data-action="agents">Open Agent Space</button><button class="cc-action" data-action="observatory">Open Observatory</button></div></main>`;
    parent.appendChild(pane);
    const date = pane.querySelector('[data-date]');
    const title = pane.querySelector('[data-greeting]');
    const feedback = pane.querySelector('[data-feedback]');
    const compose = pane.querySelector('[data-compose]');
    const send = pane.querySelector('[data-send]');
    const refresh = async () => {
      date.textContent = dateLabel(); title.textContent = greeting();
      const sessions = await invoke('agent_sessions_list').catch(() => []);
      if (!pane.isConnected) return;
      const state = activityFeedback(sessions);
      feedback.innerHTML = `<span class="cc-pulse ${state.tone}"></span>${state.text}`;
    };
    const submit = () => {
      const text = compose.value.trim(); if (!text) return;
      compose.value = ''; routePrompt(text);
    };
    send.onclick = submit;
    compose.onkeydown = (event) => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(); } };
    pane.querySelector('[data-action="new-agent"]').onclick = () => window.xnautOpenNewAgent && window.xnautOpenNewAgent();
    pane.querySelector('[data-action="agents"]').onclick = () => window.xnautOpenAgentSpace && window.xnautOpenAgentSpace();
    pane.querySelector('[data-action="observatory"]').onclick = () => window.xnautAttachObservatoryTab && window.xnautAttachObservatoryTab();
    await refresh();
    const timer = setInterval(refresh, 5000);
    return { kind:'control-center', label:`control-center-${tabId}`, pane, dispose() { clearInterval(timer); } };
  }

  window.xnautCreateControlCenterPanel = createControlCenterPanel;
})();
