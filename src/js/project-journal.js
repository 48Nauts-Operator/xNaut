// XNAUT-455 — Live working document; capture belongs to native storage, not this view.
(function () {
  'use strict';
  const instances = new WeakMap();
  const drafts = new Map();
  const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const invoke = (name, args) => window.__TAURI__.core.invoke(name, args);
  const date = s => { const d = new Date(s); return Number.isNaN(d.getTime()) ? 'Time not recorded' : d.toLocaleString([], {dateStyle:'medium',timeStyle:'short'}); };
  const labels = {note:'Your notes',question:'Open question',proposal:'Proposed change',decision:'Decision',finding:'Finding',fix:'Fix · reported',verification:'Verification · reported',summary:'Work summary',execution:'Execution receipt',progress:'Agent progress'};
  function markdown(el, text, openWiki) {
    // The offline Markdown renderer only recognizes absolute links. Use a local
    // sentinel while parsing, then route it directly to the existing Wiki.
    const prepared=String(text).replace(/\]\(\.\.\/\.\.\/(Development\/[a-zA-Z0-9_./-]+\.md)\)/g,'](https://xnaut-wiki.invalid/$1)');
    const parsed = new DOMParser().parseFromString(window.xnautMarkdown ? window.xnautMarkdown.render(prepared) : `<pre>${esc(text)}</pre>`, 'text/html');
    const allowed = new Set('P H1 H2 H3 H4 H5 H6 UL OL LI STRONG EM B I S DEL BLOCKQUOTE CODE PRE TABLE THEAD TBODY TR TH TD HR BR A DETAILS SUMMARY SPAN'.split(' '));
    parsed.body.querySelectorAll('*').forEach(node => {
      if(node.tagName==='DIV' && node.classList.contains('mermaid')) {const pre=parsed.createElement('pre');pre.textContent=node.textContent;node.replaceWith(pre);return;}
      if (!allowed.has(node.tagName)) { node.remove(); return; }
      for (const attr of [...node.attributes]) if (!(node.tagName === 'A' && attr.name === 'href') && !(node.tagName === 'CODE' && attr.name === 'class')) node.removeAttribute(attr.name);
      if (node.tagName === 'A') {
        const href = node.getAttribute('href') || '';
        if (/^https:\/\/xnaut-wiki\.invalid\/Development\/[a-zA-Z0-9_./-]+\.md$/.test(href)) { node.dataset.wikiPath=href.slice('https://xnaut-wiki.invalid/'.length); node.removeAttribute('href'); }
        else if (!/^https?:\/\//i.test(href)) node.removeAttribute('href');
        else { node.setAttribute('target','_blank'); node.setAttribute('rel','noopener noreferrer'); }
      }
    });
    el.replaceChildren(...parsed.body.childNodes);
    el.querySelectorAll('[data-wiki-path]').forEach(a => { a.setAttribute('role','button'); a.tabIndex=0; a.onclick=()=>openWiki?.(a.dataset.wikiPath); a.onkeydown=e=>{if(e.key==='Enter')a.click();}; });
    el.querySelectorAll('a[href]').forEach(a => { a.onclick = e => {e.preventDefault();invoke('plugin:shell|open',{path:a.getAttribute('href')}).catch(err => {a.title=String(err);});}; });
    el.querySelectorAll('pre').forEach(pre => {
      const content = pre.textContent;
      if (content.split('\n').length > 12 || content.length > 900) {
        const details = document.createElement('details');
        const summary = document.createElement('summary'); summary.textContent = `Code · ${content.split('\n').length} lines · expand`;
        pre.replaceWith(details); details.append(summary, pre);
      }
      const button = document.createElement('button'); button.className = 'pj-copy'; button.textContent = 'Copy';
      button.onclick = async () => {try {await navigator.clipboard.writeText(content);button.textContent='Copied';}catch(_){button.textContent='Copy unavailable';}};
      pre.append(button);
    });
  }
  function mount(host, root, openWiki) {
    instances.get(host)?.dispose();
    let stopped=false, busy=false, data=null, selected=null, filter='', fingerprint='', timer;
    host.innerHTML = `<section class="pj"><header class="pj-header"><div class="pj-eyebrow">PROJECT WORKING DOCUMENT <span class="pj-live">● Live</span></div><h1 data-title>Live Journal</h1><p data-purpose>Loading saved project context…</p><div class="pj-toolbar"><select aria-label="Journal date" data-date><option value="">Today</option></select><select aria-label="Journal workstream" data-filter><option value="">All workstreams</option></select><button data-refresh>Refresh</button><button data-wiki>Open in Wiki ↗</button></div><small data-sync></small><p role="status" data-status></p></header><div class="pj-scroll"><article class="pj-document"><section class="pj-opening"><div class="pj-eyebrow">START HERE</div><h2>Where we stand</h2><p class="pj-muted">Saved context from earlier work. Evidence retains its original date.</p><div data-opening></div></section><section data-current></section><section class="pj-notes"><h2>Add your note or question</h2><form data-form><input data-ticket aria-label="Existing project ticket" placeholder="Ticket, e.g. XNAUT-455" required><select data-kind aria-label="Note type"><option value="note">Note</option><option value="question">Question</option><option value="decision">Decision</option></select><textarea data-note aria-label="Journal note" placeholder="Add context for the next person, a decision, or a question…" required></textarea><button type="submit">Save to Journal</button><small>Your name and time are recorded. Questions are saved here; use chat to ask an agent to answer.</small></form></section><section><div class="pj-eyebrow">AS THE WORK DEVELOPS</div><h2>Working notes</h2><p data-empty class="pj-muted"></p><div data-entries></div></section></article></div></section>`;
    const $ = q => host.querySelector(q);
    const status = s => { $('[data-status]').textContent=s; };
    const remember = () => drafts.set(root,{text:$('[data-note]').value,ticket:$('[data-ticket]').value,kind:$('[data-kind]').value});
    const draft=drafts.get(root); if(draft){$('[data-note]').value=draft.text;$('[data-ticket]').value=draft.ticket;$('[data-kind]').value=draft.kind;}
    $('[data-form]').oninput=remember;
    function source(entry) {
      if (entry.run_id) showSource(entry.run_id);
      else if (entry.thread_id && entry.agent && window.xnautOpenAgentSpace) window.xnautOpenAgentSpace(entry.agent,entry.thread_id);

    }
    async function showSource(id) {
      try {
        const record=await invoke('project_wiki_source',{project:data.project.key,kind:'run',id});
        if(stopped)return;
        const dialog=document.createElement('dialog');dialog.className='pj-dialog';dialog.innerHTML=`<h2>${esc(record.title)}</h2><pre>${esc(record.text)}</pre><button>Close</button>`;
        host.append(dialog);dialog.querySelector('button').onclick=()=>dialog.remove();dialog.showModal();
      } catch(e){status(String(e));}
    }
    function paint() {
      $('[data-title]').textContent = `${data.project.name} · Live Journal`;
      $('[data-purpose]').textContent = data.project.purpose || 'The working document, from first question to handoff.';
      const dates=$('[data-date]');const previous=dates.value;
      dates.innerHTML='<option value="">Today</option>'+data.documents.map(d=>`<option value="${esc(d.path)}">${esc(d.path.split('/').pop().replace('.md',''))}</option>`).join('');dates.value=selected || previous;
      const tickets=[...new Set([...data.entries.map(e=>e.ticket),...data.runs.map(r=>r.ticket)].filter(Boolean))];
      $('[data-filter]').innerHTML='<option value="">All workstreams</option>'+tickets.map(t=>`<option>${esc(t)}</option>`).join('');$('[data-filter]').value=filter;
      const next=JSON.stringify([data.path,data.opening,data.entries,data.runs,filter]);
      if(next===fingerprint)return;fingerprint=next;
      markdown($('[data-opening]'),data.opening,openWiki);
      $('[data-current]').innerHTML = data.runs.length ? `<h2>Currently in progress</h2>${data.runs.filter(r=>!filter||r.ticket===filter).map(r=>`<div class="pj-run"><strong>${esc(r.ticket || r.run_id)} · @${esc(r.agent_handle)}</strong><span>${esc(r.state)} · observed ${esc(date(new Date(r.last_seen_at).toISOString()))}</span><p>${esc(r.last_signal)}</p></div>`).join('')}` : '';
      const shown=data.entries.filter(e=>!filter||e.ticket===filter);
      $('[data-empty]').textContent=shown.length ? '' : 'No entries recorded for this selection yet. Project-bound chat turns, worker receipts and agent-authored findings appear here as they are saved.';
      const container=$('[data-entries]');const existing=new Map([...container.children].map(n=>[n.dataset.id,n]));
      // Reuse unchanged blocks: incoming work must not collapse code or move the reader.
      for(const entry of shown){
        let node=existing.get(entry.id);existing.delete(entry.id);
        if(node && node.dataset.content!==entry.content){node.remove();node=null;}
        if(!node){
          node=document.createElement('section');node.className='pj-entry pj-'+entry.kind;node.dataset.id=entry.id;node.dataset.content=entry.content;
          node.innerHTML=`<div class="pj-entry-meta"><span>${esc(labels[entry.kind] || entry.kind)}</span><time>${esc(date(entry.at))}</time></div><div class="pj-author">${esc(entry.actor)}${entry.ticket?' · '+esc(entry.ticket):''}</div><div class="pj-body"></div><div class="pj-links"></div>`;
          const contentHost=node.querySelector('.pj-body');
          if(entry.run_id && entry.preview){
            const brief=document.createElement('div');markdown(brief,entry.preview,openWiki);contentHost.append(brief);
            const evidence=document.createElement('details');const summary=document.createElement('summary');summary.textContent='Execution details and evidence';const full=document.createElement('div');full.className='pj-evidence';markdown(full,entry.content,openWiki);evidence.append(summary,full);contentHost.append(evidence);
          } else markdown(contentHost,entry.content,openWiki);
          const body=node.querySelector('.pj-body');
          const attribution=body.querySelector('h3 + p');
          if(attribution?.textContent === `${entry.at} · ${entry.actor} · ${entry.kind}`) attribution.remove();
          if(entry.thread_id || entry.run_id){const b=document.createElement('button');b.textContent=entry.run_id?'Execution record ↗':'Source conversation ↗';b.onclick=()=>source(entry);node.querySelector('.pj-links').append(b);}
          container.append(node);
        }
      }
      for(const node of existing.values())node.remove();
    }
    async function refresh() {
      if(busy||stopped)return;busy=true;
      try {
        const next=await invoke('project_journal_read',{project:root,path:selected});
        if(stopped)return;data=next;paint();$('[data-sync]').textContent='Saved in the Vault · checked '+date(data.observed_at);
        status(data.warning ? 'Capture needs attention: '+data.warning : '');
      }catch(e){if(!stopped){status(String(e));$('[data-purpose]').textContent='Select a registered project to read its Journal.';}}
      finally{busy=false;}
    }
    $('[data-date]').onchange=e=>{selected=e.target.value||null;fingerprint='';void refresh();};
    $('[data-filter]').onchange=e=>{filter=e.target.value;paint();};
    $('[data-refresh]').onclick=refresh;
    $('[data-wiki]').onclick=()=>{if(data)openWiki?.(data.documents.some(d=>d.path===data.path)?data.path:null);};
    $('[data-form]').onsubmit=async e=>{
      e.preventDefault();if(!data)return;const button=$('[data-form] button');button.disabled=true;remember();
      const text=$('[data-note]').value;const ticket=$('[data-ticket]').value.trim();const kind=$('[data-kind]').value;
      try{await invoke('project_journal_add',{request:{project:data.project.key,ticket,kind,title:kind==='question'?'Your question':kind==='decision'?'Your decision':'Your note',content:text}});if(stopped)return;$('[data-note]').value='';remember();selected=null;fingerprint='';await refresh();}
      catch(err){if(!stopped)status('Note not saved: '+String(err));}finally{if(button.isConnected)button.disabled=false;}
    };
    void refresh();timer=setInterval(()=>{if(!host.isConnected){clearInterval(timer);stopped=true;return;}void refresh();},5000);
    const instance={dispose(){remember();stopped=true;clearInterval(timer);},refresh};instances.set(host,instance);return instance;
  }
  window.xnautJournal={mount};
})();
