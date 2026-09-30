// Personal notes and explicit conversation distillation. No hidden inference:
// a model request happens only when Summarize conversation is pressed.
(function () {
  'use strict';
  const PREFIX = 'xnaut-notebook:';
  const mounts = new Set();
  const uploads = new Map();
  let preferred = null, scheduled = false;
  const storage = () => window.xnautConversationStorage;
  const el = (tag, cls, text) => {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text != null) node.textContent = text;
    return node;
  };
  const button = (text, action) => {
    const node = el('button', 'nb-button', text); node.type = 'button'; node.onclick = action; return node;
  };
  function source() {
    const visible = node => node?.isConnected && node.getClientRects().length && !node.closest('[hidden]');
    const node = visible(preferred) ? preferred : [...document.querySelectorAll('[data-notebook-context]')].find(visible);
    return node?.xnautNotebookContext?.() || null;
  }
  function sync() {
    scheduled = false;
    for (const view of mounts) {
      if (!view.host.isConnected) { mounts.delete(view); continue; }
      view.sync();
    }
  }
  function schedule() {
    if (!scheduled) { scheduled = true; requestAnimationFrame(sync); }
  }
  const main = document.getElementById('terminal-container');
  if (main) new MutationObserver(schedule).observe(main, {childList:true, subtree:true, attributes:true, attributeFilter:['hidden','data-notebook-context']});
  document.addEventListener('focusin', event => {
    const node = event.target.closest('[data-notebook-context]');
    if (node) { preferred = node; schedule(); }
  });
  document.addEventListener('xnaut:chat-history-changed', schedule);
  window.addEventListener('xnaut:agent-threads-changed', schedule);

  function styles() {
    if (document.getElementById('conversation-notebook-styles')) return;
    const style = el('style'); style.id = 'conversation-notebook-styles';
    style.textContent = `
      .nb {display:flex;flex-direction:column;min-height:0;height:100%;color:var(--foreground);font-size:12px}
      .nb-bar {display:flex;gap:6px;flex-wrap:wrap;padding:12px 12px 8px}
      .nb-button {background:var(--muted,#252525);color:var(--foreground,#ddd);border:1px solid var(--border,#393939);border-radius:6px;padding:6px 9px;font:inherit;cursor:pointer}
      .nb-button:disabled {opacity:.45;cursor:default}.nb-button.primary {background:var(--primary,#e9b949);color:#161616;font-weight:650}
      .nb-scope,.nb-status {padding:0 12px 8px;color:var(--muted-foreground,#aaa);overflow-wrap:anywhere;line-height:1.5}
      .nb-status:empty {display:none}.nb-status.error {color:#ef988d}
      .nb-list {overflow:auto;flex:1;min-height:0;padding:4px 12px 16px}
      .nb-empty {padding:28px 8px;line-height:1.7;color:var(--muted-foreground,#aaa)}
      .nb-empty strong {display:block;color:var(--foreground);margin-bottom:6px}
      .nb-card {border:1px solid var(--border,#393939);border-radius:9px;background:var(--card,#202020);padding:12px;margin-bottom:12px}
      .nb-card input[type=text],.nb-card textarea {box-sizing:border-box;width:100%;font:inherit;color:inherit;background:transparent;border:1px solid transparent;border-radius:4px;padding:5px;resize:vertical}
      .nb-card input[type=text] {font-weight:650;font-size:13px;margin-bottom:4px}.nb-card textarea {min-height:160px;line-height:1.6;border-color:var(--border,#444);background:var(--background,#181818)}
      .nb-card input:focus,.nb-card textarea:focus {outline:1px solid var(--primary,#e9b949)}
      .nb-meta {font-size:10px;color:var(--muted-foreground,#aaa);padding:4px 5px;line-height:1.5}
      .nb-actions {display:flex;gap:6px;margin-top:10px}.nb-body {white-space:pre-wrap;overflow-wrap:anywhere;line-height:1.65;padding:4px 5px}
      .nb-body h4 {font-size:12px;margin:12px 0 5px}.nb-body label {display:flex;align-items:flex-start;gap:8px;margin:7px 0;cursor:pointer}
      .nb-body input {margin-top:4px;accent-color:var(--primary,#e9b949)}.nb-body input:checked+span {text-decoration:line-through;opacity:.65}
      .nb-recover {margin:0 12px 8px;max-width:calc(100% - 24px);background:var(--background,#181818);color:inherit;padding:5px}
    `;
    document.head.appendChild(style);
  }

  async function mount(host, root) {
    styles();
    for (const previous of mounts) if (previous.host === host) mounts.delete(previous);
    host.innerHTML = '';
    host.classList.add('nb');
    const bar = el('div','nb-bar'), scopeLabel = el('div','nb-scope'), status = el('div','nb-status'), list = el('div','nb-list');
    status.setAttribute('role','status');
    const recover = el('select','nb-recover'); recover.setAttribute('aria-label','Recovered notebooks'); recover.hidden = true;
    host.append(bar,scopeLabel,recover,status,list);
    let key = '', baseKey = '', context = null, documentData = {notes:[]}, loaded = false;
    const pending = new Set();
    function notify(text, error = false) { status.textContent = text; status.classList.toggle('error',error); }
    async function save(targetKey, data) {
      notify('Saving…');
      try {
        storage().setItem(targetKey, JSON.stringify(data));
        await storage().confirmSaved(targetKey);
        if (root) {
          const snapshot = JSON.parse(JSON.stringify(data));
          const previous = uploads.get(targetKey) || Promise.resolve();
          const upload = previous.catch(() => {}).then(() => window.__TAURI__.core.invoke('repository_notebook_queue', {root, key:targetKey, data:snapshot}));
          uploads.set(targetKey, upload);
          try {
            await upload;
            if (key === targetKey) notify('Saved on this device · queued for repository PR');
          } catch (error) { if (key === targetKey) notify('Saved on this device · repository sync pending: ' + String(error), true); }
          finally { if (uploads.get(targetKey) === upload) uploads.delete(targetKey); }
        } else if (key === targetKey) notify('Saved on this device · select a project to sync');
      } catch (error) { if (key === targetKey) notify('Could not save: ' + String(error), true); }
    }
    function read(target) {
      const value = storage().getItem(target);
      if (!value) return {notes:[]};
      const data = JSON.parse(value);
      if (!data || !Array.isArray(data.notes)) throw new Error('Saved notebook is unreadable. It has been preserved.');
      return data;
    }
    function render(editId) {
      list.replaceChildren();
      if (!documentData.notes.length) {
        const empty = el('div','nb-empty'); empty.append(el('strong','', 'Keep what matters here.'),el('div','', 'Add your own note, or turn this conversation into a short summary, next steps and things to remember.'));
        list.append(empty);
      }
      for (const note of documentData.notes) {
        const targetKey = key, data = documentData;
        const card = el('section','nb-card'), title = el('input'), meta = el('div','nb-meta'), body = el('div','nb-body'), editor = el('textarea'), actions = el('div','nb-actions');
        card.dataset.noteId = note.id;
        title.type = 'text'; title.value = note.title || ''; title.placeholder = 'Note title'; title.setAttribute('aria-label','Note title'); title.maxLength = 160;
        editor.value = note.body || ''; editor.placeholder = 'Write something to remember…\n\n- [ ] A task to do'; editor.setAttribute('aria-label','Note text'); editor.maxLength = 32000;
        meta.textContent = note.kind === 'summary' ? 'AI draft · ' + note.source + ' · ' + new Date(note.created).toLocaleString() : 'Your note';
        let editing = editId === note.id;
        const persist = () => { note.updated = new Date().toISOString(); void save(targetKey,data); };
        function bodyView() {
          body.replaceChildren();
          String(note.body || '').split('\n').forEach((line,index) => {
            const task = /^\s*[-*] \[([ xX])\]\s*(.*)$/.exec(line);
            if (task) {
              const label = el('label'), check = el('input'), text = el('span','',task[2]);
              check.type = 'checkbox'; check.checked = task[1].toLowerCase() === 'x';
              check.onchange = () => {
                const lines = note.body.split('\n'); lines[index] = '- [' + (check.checked ? 'x' : ' ') + '] ' + task[2];
                note.body = lines.join('\n'); editor.value = note.body; persist();
              };
              label.append(check,text); body.append(label);
            } else if (/^#{1,4} /.test(line)) body.append(el('h4','',line.replace(/^#{1,4} /,'')));
            else body.append(el('div','',line || '\u00a0'));
          });
        }
        function mode() { editor.hidden = !editing; body.hidden = editing; edit.textContent = editing ? 'Done' : 'Edit note'; bodyView(); }
        title.oninput = () => { note.title = title.value; persist(); };
        editor.oninput = () => { note.body = editor.value; persist(); };
        const edit = button('Edit note', () => { editing = !editing; mode(); if (editing) editor.focus(); });
        const remove = button('Delete note', async () => {
          if (!await window.xnautConfirmDialog('Delete this note?', 'Delete')) return;
          data.notes = data.notes.filter(item => item.id !== note.id); await save(targetKey,data);
          if (key === targetKey) render();
        });
        actions.append(edit,remove); card.append(title,meta,body,editor,actions); list.append(card); mode();
        if (editing) requestAnimationFrame(() => editor.focus());
      }
    }
    const add = button('+ Add note', () => {
      const note = {id:crypto.randomUUID(),title:'',body:'',kind:'personal',created:new Date().toISOString()};
      documentData.notes.unshift(note); void save(key,documentData); render(note.id);
    });
    add.classList.add('primary');
    const summarize = button('Summarize conversation', async () => {
      const snapshot = source();
      if (!snapshot?.messages?.length || pending.has(baseKey)) return;
      const targetKey = baseKey;
      pending.add(targetKey); summarize.disabled = true; notify('Distilling the conversation…');
      try {
        const result = await window.__TAURI__.core.invoke('notebook_distill', {messages:snapshot.messages,provider:snapshot.provider || null,model:snapshot.model || null});
        if (!result || typeof result.summary !== 'string' || !Array.isArray(result.tasks) || !Array.isArray(result.remember)) throw new Error('No usable summary returned');
        const data = read(targetKey); // Includes notes/edits made during the request.
        const body = [result.summary, result.tasks.length ? '\n## Next steps\n' + result.tasks.map(task => '- [ ] ' + task).join('\n') : '', result.remember.length ? '\n## Remember\n' + result.remember.map(item => '• ' + item).join('\n') : ''].filter(Boolean).join('\n');
        data.notes.unshift({id:crypto.randomUUID(),title:result.title,body,kind:'summary',source:snapshot.label,created:new Date().toISOString()});
        await save(targetKey,data);
        if (key === targetKey) { documentData = data; render(); }
      } catch (error) { if (baseKey === targetKey) notify('Could not summarize: ' + String(error),true); }
      finally { pending.delete(targetKey); view.sync(); }
    });
    summarize.title = 'Create a new editable draft using this conversation’s model. Existing notes are kept.';
    bar.append(add,summarize); add.disabled = summarize.disabled = true;
    function loadKey(next) {
      key = next;
      try { documentData = read(key); add.disabled = false; render(); }
      catch (error) { list.replaceChildren(); add.disabled = summarize.disabled = true; notify(String(error),true); }
    }
    recover.onchange = () => { notify(''); loadKey(recover.value); };
    const view = {host, sync() {
      if (!loaded) return;
      context = source();
      const nextBase = PREFIX + (context?.id || 'workspace:' + (root || 'home'));
      summarize.disabled = !context?.messages?.length || pending.has(nextBase);
      if (nextBase === baseKey) return;
      baseKey = nextBase; notify('');
      scopeLabel.textContent = context ? context.label : 'Workspace notes · ' + (root?.split('/').pop() || 'Home');
      recover.replaceChildren(new Option('Current notebook',baseKey));
      storage().keys().filter(k => k.startsWith(baseKey + ':recovered:')).forEach(k => recover.append(new Option('Recovered edits · ' + k.split(':recovered:')[1],k)));
      recover.hidden = recover.options.length < 2;
      loadKey(baseKey);
    }};
    mounts.add(view);
    try { await storage().ready(); loaded = true; view.sync(); }
    catch (error) { notify('Could not load notes: ' + String(error),true); }
    return view;
  }
  window.xnautNotebook = {mount};
})();
