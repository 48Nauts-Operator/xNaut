// XNAUT-455 — Live working document; capture belongs to native storage, not this view.
(function () {
  'use strict';
  const instances = new WeakMap();
  const drafts = new Map();
  const groupActions = new Map();
  let mountId = 0;
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
  // XNAUT-461: the current summary is a projection of native coordination records.
  // Evidence text is escaped and labelled; a worker handback is never verification.
  const continuityLabels = {active:'Active · recorded signal',stalled:'Stalled',blocked:'Blocked',review:'Review needed',verified:'Verification recorded',unknown:'Needs inspection'};
  const evidenceLabels = {ticket:'Ticket record',run:'Run record',handback:'Agent claim',transfer:'Assignment transfer',pull_request:'Pull request record',review:'Review record',verification:'Verification evidence',launch_receipt:'Launch receipt'};
  function evidenceMarkup(records) {
    if (!records?.length) return '<p class="pj-muted">No evidence recorded.</p>';
    return `<ul class="pj-provenance">${records.map(e=>`<li><strong>${esc(evidenceLabels[e.kind] || 'Recorded evidence')}</strong><p>${esc(e.detail)}</p><small>Source: ${esc(e.source)}</small></li>`).join('')}</ul>`;
  }
  function assignmentMarkup(a) {
    return `<div class="pj-assignment"><strong>${esc(a.owner || 'Unassigned')} · ${esc(continuityLabels[a.state] || a.state)}</strong><p>${esc(a.next_action)}</p><dl><dt>Run</dt><dd>${esc(a.run_id)}</dd>${[['Recorded state',a.run_state],['Branch',a.branch],['Worktree',a.worktree],['Commit',a.last_commit],['Pull request',a.pr_url],['Review',a.review_state],['Previous run',a.previous_run_id],['Next run',a.next_run_id]].filter(([,v])=>v).map(([k,v])=>`<dt>${k}</dt><dd>${esc(v)}</dd>`).join('')}</dl><button data-continuity-run="${esc(a.run_id)}">Execution record ↗</button>${evidenceMarkup(a.evidence)}</div>`;
  }
  function continuityMarkup(snapshot, filter) {
    const assignments = snapshot.assignments || [];
    const tickets = (snapshot.tickets || []).filter(t=>!filter || t.id===filter);
    // A mature project may have years of unverified handbacks. Show live work
    // first, then recently updated tickets; retain the complete older record.
    tickets.sort((a,b)=>(a.state==='active'?0:1)-(b.state==='active'?0:1) || (Date.parse(b.updated_at)||0)-(Date.parse(a.updated_at)||0));
    const current = t => ['active','stalled','blocked','review'].includes(t.state) || ['in_progress','in_review','review','blocked'].includes(t.status);
    const card = t => {
      const linked=assignments.filter(a=>(t.assignment_ids || []).includes(a.run_id));
      return `<section class="pj-work" data-continuity-ticket="${esc(t.id)}"><h3>${esc(t.id)} · ${esc(t.title)}</h3><p class="pj-work-state">${esc(continuityLabels[t.state] || t.state)} · Ticket: ${esc((t.status || '').replace(/_/g,' '))} · Owner: ${esc(t.owner || 'Unassigned')}</p><p><strong>Next:</strong> ${esc(t.next_action)}</p><details data-continuity-detail="ticket:${esc(t.id)}"><summary>Assignments and evidence${linked.length?' · '+linked.length+' run'+(linked.length===1?'':'s'):''}</summary><div class="pj-evidence">${evidenceMarkup(t.evidence)}${linked.map(assignmentMarkup).join('')}</div></details></section>`;
    };
    const active=tickets.filter(current), other=tickets.filter(t=>!current(t));
    // Orphaned ticket references still need to be inspectable; never silently lose a run.
    const orphaned=assignments.filter(a=>(!filter || a.ticket===filter) && !(snapshot.tickets || []).some(t=>(t.assignment_ids || []).includes(a.run_id)));
    const diagnostics=snapshot.diagnostics || [];
    return `${diagnostics.length?`<div class="pj-continuity-warning" role="status"><strong>Current state is incomplete.</strong><ul>${diagnostics.map(d=>`<li>${esc(d.message)} <small>Source: ${esc(d.source)}</small></li>`).join('')}</ul></div>`:''}
      <p class="pj-muted">${diagnostics.length?'Available records show ':''}${active.length} recorded work item${active.length===1?'':'s'} needing attention${filter?' in '+esc(filter):''}. Status comes from saved records; a recorded signal does not confirm a worker is still running.</p>
      ${active.map(card).join('')}
      ${!tickets.length&&!orphaned.length?'<p class="pj-muted">'+(diagnostics.length?'Work could not be established from the available records.':'No work is recorded for this selection.')+'</p>':''}
      ${other.length?`<details data-continuity-detail="other"><summary>Other recorded work · ${other.length} · outcomes and unstarted work</summary><div class="pj-evidence">${other.map(card).join('')}</div></details>`:''}
      ${orphaned.length?`<details data-continuity-detail="unlinked"><summary>Other run records · ${orphaned.length}</summary><div class="pj-evidence">${orphaned.map(assignmentMarkup).join('')}</div></details>`:''}`;
  }
  // Native coordination history is already durably captured in the Journal.
  // The older system ledger does not record native swarm/PR transitions.
  const isWorkerActivity = e => e.id?.startsWith('activity:') || (e.run_id && ['execution','summary'].includes(e.kind));
  function mount(host, root, openWiki) {
    instances.get(host)?.dispose();
    let stopped=false, busy=false, queued=false, data=null, selected=null, filter='', fingerprint='', openingFingerprint=null, groupsFingerprint='', timer, sessions=[], sessionsError='', tab='actions', actionKind='', actionBefore=null;
    const id=++mountId;
    host.innerHTML = `<section class="pj"><header class="pj-header"><div class="pj-eyebrow">PROJECT WORKING DOCUMENT <span class="pj-live">● Live</span></div><h1 data-title>Live Journal</h1><p data-purpose>Loading saved project context…</p><div class="pj-toolbar"><select aria-label="Journal date" data-date><option value="">Today</option></select><select aria-label="Journal workstream" data-filter><option value="">All workstreams</option></select><button data-refresh>Refresh</button><button data-wiki>Open in Wiki ↗</button></div><small data-sync></small><p role="status" data-status></p></header>
      <div class="pj-live-context"><section class="pj-summary"><h2>Where we stand</h2><p class="pj-muted" data-continuity-time></p><div data-summary></div></section><section aria-label="Deployed agents" class="pj-deployed"><div class="pj-section-heading"><h3>Deployed agents <small>· current project</small></h3><button data-approvals>Approvals ↗</button></div><div data-agents class="pj-agent-grid"></div></section></div>
      <nav class="pj-tabs" role="tablist" aria-label="Journal sections">${['actions','tickets','notes','handoffs'].map(k=>`<button role="tab" id="pj-${id}-${k}" aria-controls="pj-${id}-${k}-panel" aria-selected="${k===tab}" tabindex="${k===tab?0:-1}" data-journal-tab="${k}">${k[0].toUpperCase()+k.slice(1)}</button>`).join('')}</nav>
      <div class="pj-scroll"><article class="pj-document">
      <section role="tabpanel" id="pj-${id}-actions-panel" aria-labelledby="pj-${id}-actions" data-panel="actions"><div class="pj-section-heading"><h2>Actions</h2><button data-tools>Computers & verification</button></div><p class="pj-muted" data-actions-date></p><section data-groups></section><div data-actions></div></section>
      <section role="tabpanel" id="pj-${id}-tickets-panel" aria-labelledby="pj-${id}-tickets" data-panel="tickets" hidden><h2>Tickets <small>· current state</small></h2><div data-continuity></div><section data-current></section></section>
      <section role="tabpanel" id="pj-${id}-notes-panel" aria-labelledby="pj-${id}-notes" data-panel="notes" hidden><h2>Notes</h2><p class="pj-muted" data-history-date></p><details class="pj-notes"><summary>Add your note or question</summary><form data-form><input data-ticket aria-label="Existing project ticket" placeholder="Ticket, e.g. XNAUT-455" required><select data-kind aria-label="Note type"><option value="note">Note</option><option value="question">Question</option><option value="decision">Decision</option></select><textarea data-note aria-label="Journal note" placeholder="Add context for the next person, a decision, or a question…" required></textarea><button type="submit">Save to Journal</button><small>Your name and time are recorded. Questions are saved here; use chat to ask an agent to answer.</small></form></details><details class="pj-saved-context"><summary>Saved context & previous journals</summary><div class="pj-evidence" data-opening></div></details><p data-empty class="pj-muted"></p><div data-entries></div></section>
      <section role="tabpanel" id="pj-${id}-handoffs-panel" aria-labelledby="pj-${id}-handoffs" data-panel="handoffs" hidden><h2>Handoffs <small>· current recorded outcomes</small></h2><div data-handoffs></div></section>
      </article></div></section>`;
    const $ = q => host.querySelector(q);
    const status = s => { $('[data-status]').textContent=s; };
    const remember = () => drafts.set(root,{text:$('[data-note]').value,ticket:$('[data-ticket]').value,kind:$('[data-kind]').value});
    const draft=drafts.get(root); if(draft){$('[data-note]').value=draft.text;$('[data-ticket]').value=draft.ticket;$('[data-kind]').value=draft.kind;}
    $('[data-form]').oninput=remember;
    function switchTab(key, focus=false) {
      tab=key;
      host.querySelectorAll('[data-journal-tab]').forEach(b=>{const active=b.dataset.journalTab===key;b.setAttribute('aria-selected',String(active));b.tabIndex=active?0:-1;if(active&&focus)b.focus();});
      host.querySelectorAll('[data-panel]').forEach(p=>{p.hidden=p.dataset.panel!==key;});
      $('[data-panel="'+key+'"]').scrollTop=0;
    }
    host.querySelectorAll('[data-journal-tab]').forEach((b,i,list)=>{
      b.onclick=()=>switchTab(b.dataset.journalTab);
      b.onkeydown=e=>{let n=i;if(e.key==='ArrowRight')n=(i+1)%list.length;else if(e.key==='ArrowLeft')n=(i+list.length-1)%list.length;else if(e.key==='Home')n=0;else if(e.key==='End')n=list.length-1;else return;e.preventDefault();switchTab(list[n].dataset.journalTab,true);};
    });
    $('[data-approvals]').onclick=()=>window.xnautOpenMesh?.();
    $('[data-tools]').onclick=()=>window.xnautOpenAgentTools?.(data?.project);
    function paintConsole(snapshot) {
      const c=data.console?.project===data.project.key?data.console:null;
      const tickets=(snapshot?.tickets || []).filter(t=>!filter||t.id===filter);
      const counts=['active','blocked','stalled','review','unknown'].map(k=>[k,tickets.filter(t=>t.state===k).length]).filter(([,n])=>n);
      const diagnostics=snapshot?.diagnostics || [];
      const summaryMarkup=(snapshot?`<p class="pj-counts">${counts.length?counts.map(([k,n])=>`<span><strong>${n}</strong> ${esc({active:'active',blocked:'blocked',stalled:'stalled',review:'awaiting review',unknown:'need inspection'}[k])}</span>`).join(''):'No work needs attention in this selection.'}</p>`:`<p class="pj-continuity-warning">${esc(data.continuity_error || 'Current project state is unavailable.')}</p>`)+(diagnostics.length?`<details class="pj-diagnostics"><summary>Current state is incomplete · ${diagnostics.length} source warning${diagnostics.length===1?'':'s'}</summary>${diagnostics.map(d=>`<p>${esc(d.message)} <small>${esc(d.source)}</small></p>`).join('')}</details>`:'');
      const summaryHost=$('[data-summary]');
      if(summaryHost.dataset.fingerprint!==summaryMarkup){const open=summaryHost.querySelector('details')?.open;summaryHost.innerHTML=summaryMarkup;summaryHost.dataset.fingerprint=summaryMarkup;if(open&&summaryHost.querySelector('details'))summaryHost.querySelector('details').open=true;}
      const agents=c?.deployed || [];
      const markup=agents.map(r=>{
        // A harness name or owner is not a session identity. Adoption can change
        // PTY id, so permit only the exact durable zellij identity as fallback.
        const candidates=sessions.filter(s=>s.session_id===r.pty_session || (r.zellij_session&&s.zellij_session===r.zellij_session&&s.remote_env===(r.destination==='local'?null:r.destination)));
        const live=candidates.length===1?candidates[0]:null;
        return `<section class="pj-agent" title="${esc(r.signal || 'No progress message recorded')} · Observed ${esc(date(live?.last_output_at_ms || r.observed_at))} · ${esc(r.machine || '')}" data-agent-run="${esc(r.run_id)}"><div><strong>@${esc(r.agent)}</strong><span class="pj-agent-state">${esc(live?.status || r.state)}${live?'':' · recorded'}</span></div><small>${esc(r.ticket || 'No ticket')} · ${esc(r.destination)}</small><div class="pj-agent-links">${live?`<button data-session="${esc(live.session_id)}" aria-label="Open session for ${esc(r.agent)}">Session ↗</button>`:''}<button data-inspect="${esc(r.run_id)}">Inspect</button><button data-agent-settings="${esc(r.agent)}" aria-label="Settings for ${esc(r.agent)}" title="Agent settings">⚙</button></div></section>`;
      }).join('');
      const agentHost=$('[data-agents]');
      const content=(sessionsError?`<p class="pj-muted">${esc(sessionsError)}</p>`:'')+(markup || `<p class="pj-muted">${c?'No deployed workers recorded for this project.':'Deployed-agent records are unavailable.'}</p>`);
      if(agentHost.dataset.fingerprint!==content){agentHost.innerHTML=content;agentHost.dataset.fingerprint=content;}
      agentHost.querySelectorAll('[data-session]').forEach(b=>{b.onclick=()=>window.xnautOpenAgentSession?.(b.dataset.session,'Agent session');});
      agentHost.querySelectorAll('[data-inspect]').forEach(b=>{b.onclick=()=>showSource(b.dataset.inspect);});
      agentHost.querySelectorAll('[data-agent-settings]').forEach(b=>{b.onclick=()=>window.xnautOpenAgentSettings?.(b.dataset.agentSettings);});
      $('[data-actions-date]').textContent='Recorded actions · '+data.path.split('/').pop().replace('.md','')+' (UTC)'+(filter?' · '+filter:'');
      const workerActions=actionBefore==null ? data.entries.filter(isWorkerActivity).map(e=>({
        at:e.at,kind:e.kind,agent:e.agent,actor:e.actor,ticket:e.ticket,run_id:e.run_id,
        detail:e.title,entry:e,
      })) : [];
      const actions=[...workerActions,...(c?.activity?.entries || [])]
        .filter(e=>!filter||e.ticket===filter)
        .sort((a,b)=>(Date.parse(b.at)||0)-(Date.parse(a.at)||0));
      const actionHost=$('[data-actions]');
      const groups=[];
      for(const e of actions.filter(e=>!actionKind||e.kind===actionKind)){
        const key=JSON.stringify([e.run_id,e.ticket,e.agent,e.kind,e.detail,e.session,e.entry?.id]);
        const previous=groups.at(-1);
        if(previous?.key===key)previous.rows.push(e);else groups.push({key,rows:[e]});
      }
      const actionsKey=JSON.stringify([groups,actionKind,c?.error,c?.activity?.total,c?.activity?.next_before,actionBefore,sessions.map(s=>s.session_id)]);
      if(actionHost.dataset.fingerprint!==actionsKey){
        const open=new Set([...actionHost.querySelectorAll('details[open]')].map(d=>d.dataset.workerHistory?'worker:'+d.dataset.workerHistory:d.dataset.actionRepeat));
        actionHost.innerHTML=`${c?.error?`<p class="pj-continuity-warning">${esc(c.error)} Saved worker history remains available.</p>`:!c?'<p class="pj-muted">System actions are unavailable. Saved worker history remains available.</p>':''}${actionKind?`<p class="pj-action-filter">${esc(actionKind)} <button data-act-clear-kind>Clear action filter</button></p>`:''}${c?.activity?.total>c?.activity?.limit?`<p class="pj-muted">${actionBefore==null?'Latest':'Earlier'} system actions · ${c.activity.total} recorded for this date.${actionBefore!=null?' Choose Latest actions to include saved worker history.':''}</p>`:''}<div class="pj-toolbar">${c?.activity?.next_before!=null?'<button data-older-actions>Older actions</button>':''}${actionBefore!=null?'<button data-latest-actions>Latest actions</button>':''}</div>${groups.length?groups.map(({key,rows})=>{
          const e=rows[0];
          return `<article class="pj-action"><div><time>${esc(date(e.at))}</time><button data-act-kind="${esc(e.kind)}">${esc(e.kind.replace(/_/g,' '))}</button></div><small>${esc(e.actor || '@'+e.agent)}</small> ${e.ticket?`<button data-act-ticket="${esc(e.ticket)}">${esc(e.ticket)}</button>`:''}<p>${esc(e.detail)}</p>${e.entry?`<details data-worker-history="${esc(e.entry.id)}"><summary>Recorded details and evidence</summary><div data-worker-body></div></details>`:''}${e.run_id?`<button data-action-run="${esc(e.run_id)}">Execution record ↗</button>`:''}${e.session&&sessions.some(s=>s.session_id===e.session)?`<button data-act-session="${esc(e.session)}">Open session ↗</button>`:''}${rows.length>1?`<details data-action-repeat="${esc(key)}"><summary>${rows.length} occurrences</summary>${rows.map(row=>`<p>${esc(date(row.at))} · ${esc(row.detail)}</p>`).join('')}</details>`:''}</article>`;
        }).join(''):'<p class="pj-muted">No actions recorded for this selection.</p>'}`;
        actionHost.dataset.fingerprint=actionsKey;
        actionHost.querySelectorAll('[data-action-repeat]').forEach(d=>{d.open=open.has(d.dataset.actionRepeat);});
        actionHost.querySelectorAll('[data-worker-history]').forEach(d=>{
          const entry=workerActions.find(a=>a.entry.id===d.dataset.workerHistory)?.entry;
          let rendered=false;
          d.addEventListener('toggle',()=>{if(d.open&&!rendered&&entry){markdown(d.querySelector('[data-worker-body]'),entry.content || entry.preview,openWiki);rendered=true;}});
          d.open=open.has('worker:'+d.dataset.workerHistory);
        });
        actionHost.querySelectorAll('[data-action-run]').forEach(b=>{b.onclick=()=>showSource(b.dataset.actionRun);});
        actionHost.querySelectorAll('[data-act-ticket]').forEach(b=>{b.onclick=()=>window.xnautOpenDelivery?.({project:data.project.key,ticket:b.dataset.actTicket,tab:'tests'});});
        actionHost.querySelectorAll('[data-act-session]').forEach(b=>{b.onclick=()=>window.xnautOpenAgentSession?.(b.dataset.actSession,'Agent session');});
        actionHost.querySelectorAll('[data-act-kind]').forEach(b=>{b.onclick=()=>{const kind=b.dataset.actKind;actionKind=actionKind===kind?'':kind;paintConsole(snapshot);[...actionHost.querySelectorAll('[data-act-kind]')].find(n=>n.dataset.actKind===kind)?.focus();};});
        const older=actionHost.querySelector('[data-older-actions]');if(older)older.onclick=()=>{actionBefore=c.activity.next_before;void refresh();};
        const latest=actionHost.querySelector('[data-latest-actions]');if(latest)latest.onclick=()=>{actionBefore=null;void refresh();};
        const clear=actionHost.querySelector('[data-act-clear-kind]');if(clear)clear.onclick=()=>{actionKind='';paintConsole(snapshot);actionHost.querySelector('[data-act-kind]')?.focus();};
      }
      const handoffs=(snapshot?.assignments || []).filter(a=>(!filter||a.ticket===filter)&&(a.pr_url||(a.evidence || []).some(e=>['handback','transfer','review','verification'].includes(e.kind))));
      const target=$('[data-handoffs]');
      // Keep expanded evidence across polling and tab changes.
      const key=JSON.stringify([handoffs,c?.handoffs]);
      if(target.dataset.fingerprint!==key){
        const expanded=new Set([...target.querySelectorAll('details[open]')].map(d=>d.dataset.handoff));
        target.innerHTML=(handoffs.length?handoffs.map(a=>`<details data-handoff="${esc(a.run_id)}"><summary>${esc(a.ticket || 'Run')} · @${esc(a.owner)} · ${esc(continuityLabels[a.state] || a.state)}</summary><div class="pj-evidence">${assignmentMarkup(a)}</div></details>`).join(''):'<p class="pj-muted">No handoff evidence recorded for this selection.</p>')+((c?.handoffs || []).length?`<h3>Project handoff documents</h3><p class="pj-muted">Saved documents for the whole project, across dates.</p>${c.handoffs.map(d=>`<p><button data-handoff-wiki="${esc(d.path)}">${esc(d.title || d.path)} ↗</button></p>`).join('')}`:'');
        target.dataset.fingerprint=key;
        target.querySelectorAll('[data-handoff]').forEach(d=>{d.open=expanded.has(d.dataset.handoff);});
        target.querySelectorAll('[data-continuity-run]').forEach(b=>{b.onclick=()=>showSource(b.dataset.continuityRun);});
        target.querySelectorAll('[data-handoff-wiki]').forEach(b=>{b.onclick=()=>openWiki?.(b.dataset.handoffWiki);});
      }
    }
    function source(entry) {
      if (entry.run_id) showSource(entry.run_id);
      else if (entry.thread_id && entry.agent && window.xnautOpenAgentSpace) window.xnautOpenAgentSpace(entry.agent,entry.thread_id);

    }
    async function showSource(id) {
      try {
        const record=await invoke('project_wiki_source',{project:data.project.key,kind:'run',id});
        if(stopped)return;
        const dialog=document.createElement('dialog');dialog.className='pj-dialog';dialog.innerHTML=`<h2>${esc(record.title)}</h2><div><button data-record>Execution record</button> <button data-output>Saved output</button> <button data-close>Close</button></div><p role="status"></p><pre>${esc(record.text)}</pre>`;
        host.append(dialog);dialog.querySelector('[data-close]').onclick=()=>dialog.remove();dialog.addEventListener('close',()=>dialog.remove());dialog.showModal();
        dialog.querySelector('[data-record]').onclick=()=>{dialog.querySelector('pre').textContent=record.text;dialog.querySelector('[role="status"]').textContent='';};
        dialog.querySelector('[data-output]').onclick=async()=>{
          const button=dialog.querySelector('[data-output]');button.disabled=true;
          try{const log=await invoke('project_wiki_source',{project:data.project.key,kind:'log',id});if(dialog.isConnected){dialog.querySelector('pre').textContent=log.text;dialog.querySelector('[role="status"]').textContent=log.truncated?'Showing the last 128 KiB of saved output.':'';}}
          catch(error){if(dialog.isConnected)dialog.querySelector('[role="status"]').textContent=String(error);}
          finally{button.disabled=false;}
        };
      } catch(e){status(String(e));}
    }
    function paintGroups() {
      const groups=(data.groups || []).filter(g=>g.project===data.project.key && g.approved_at!=null && (!filter || g.members?.some(m=>m.ticket===filter)));
      const key=g=>data.project.key+':'+g.id;
      const next=JSON.stringify([filter,groups,data.groups_error,groups.map(g=>groupActions.get(key(g)))]);
      if(next===groupsFingerprint)return;groupsFingerprint=next;
      $('[data-groups]').innerHTML=(data.groups_error?`<p class="pj-continuity-warning">${esc(data.groups_error)}</p>`:'')+(groups.length?`<h3>Approved groups</h3><p class="pj-muted">Stopping further dispatch prevents this group from starting more work. Active workers retain their current work.</p>${groups.map(g=>{
        const action=groupActions.get(key(g)) || {}, ended=g.stopped_at ?? action.stoppedAt;
        const counts=g.counts || {};
        return `<section class="pj-work" data-group="${esc(g.id)}"><strong>${ended!=null?'Further dispatch stopped':'Approved group'}</strong><p>${esc(counts.queued || 0)} queued · ${esc(counts.running || 0)} active · ${esc(counts.blocked || 0)} blocked${counts.verified?' · '+esc(counts.verified)+' verified':''}</p><small>${ended!=null?'Stopped '+esc(date(ended)):'Approved '+esc(date(g.approved_at))}</small><details><summary>Group members · ${g.members?.length || 0}</summary><ul>${(g.members || []).map(m=>`<li><strong>${esc(m.ticket)}</strong> · ${esc(String(m.state || '').replace(/_/g,' '))}${m.reason?' — '+esc(m.reason):''}</li>`).join('')}</ul></details>${ended==null?`<button data-stop-group="${esc(g.id)}" ${action.pending?'disabled':''}>${action.pending?'Stopping…':'Stop further dispatch'}</button>`:''}${action.error?`<p role="status" class="pj-continuity-warning">${esc(action.error)}</p>`:''}</section>`;
      }).join('')}`:'');
      $('[data-groups]').querySelectorAll('[data-stop-group]').forEach(button=>{button.onclick=()=>stopGroup(button.dataset.stopGroup);});
    }
    async function stopGroup(id) {
      const group=(data?.groups || []).find(g=>g.id===id && g.project===data.project.key && g.approved_at!=null);
      if(!group || group.stopped_at!=null)return;
      const project=data.project.key, key=project+':'+id, previous=groupActions.get(key);
      if(previous?.pending || previous?.stoppedAt!=null)return;
      const action={pending:true};groupActions.set(key,action);paintGroups();
      try {
        const result=await invoke('swarm_plan_stop',{planId:id});
        if(result?.plan?.id!==id || result?.plan?.project!==project || result?.stopped_at==null)throw Error('Stop receipt did not match this project group. Refresh to inspect its state.');
        action.stoppedAt=result.stopped_at;
      } catch(error) {action.error='Could not stop further dispatch: '+String(error);}
      finally {action.pending=false;if(!stopped){paintGroups();if(action.stoppedAt!=null)void refresh();}}
    }
    function paint() {
      $('[data-title]').textContent = `${data.project.name} · Live Journal`;
      $('[data-purpose]').textContent = data.project.purpose || 'The working document, from first question to handoff.';
      const dates=$('[data-date]');const previous=dates.value;
      const availableDates=[...new Set([...data.documents.map(d=>d.path),...(data.console?.project===data.project.key?(data.console.activity?.dates || []).map(d=>'Development/journal/'+d+'.md'):[])])].sort().reverse().map(path=>({path}));
      dates.innerHTML='<option value="">Today</option>'+availableDates.map(d=>`<option value="${esc(d.path)}">${esc(d.path.split('/').pop().replace('.md',''))}</option>`).join('');dates.value=selected || previous;
      const snapshot=data.continuity?.project===data.project.key ? data.continuity : null;
      const tickets=[...new Set([...data.entries.map(e=>e.ticket),...data.runs.map(r=>r.ticket),...(data.console?.project===data.project.key?(data.console.activity?.entries || []).map(e=>e.ticket):[]),...(snapshot?.tickets || []).map(t=>t.id),...(snapshot?.assignments || []).map(a=>a.ticket),...(data.groups || []).filter(g=>g.project===data.project.key).flatMap(g=>(g.members || []).map(m=>m.ticket))].filter(Boolean))];
      $('[data-filter]').innerHTML='<option value="">All workstreams</option>'+tickets.map(t=>`<option>${esc(t)}</option>`).join('');$('[data-filter]').value=filter;
      $('[data-continuity-time]').textContent=snapshot ? 'Current project records · checked '+date(snapshot.observed_at)+(selected?' · independent of the selected Journal date':'') : 'Current project state is unavailable.';
      $('[data-history-date]').textContent='Journal date: '+data.path.split('/').pop().replace('.md','')+'. Earlier reports retain their original dates and are not fresh verification.';
      // Read timestamps change each poll; only changed records should replace the DOM.
      const next=JSON.stringify([data.path,data.opening,data.entries,data.runs,snapshot&&{...snapshot,observed_at:0},data.continuity_error,filter]);
      paintGroups();
      paintConsole(snapshot);
      if(next===fingerprint)return;fingerprint=next;
      const expanded=new Set([...$('[data-continuity]').querySelectorAll('details[open]')].map(d=>d.dataset.continuityDetail));
      $('[data-continuity]').innerHTML=snapshot ? continuityMarkup(snapshot,filter) : `<p class="pj-continuity-warning">${esc(data.continuity_error || 'Could not reconcile current project records. Refresh to retry.')} Saved context remains available below.</p>`;
      $('[data-continuity]').querySelectorAll('[data-continuity-detail]').forEach(d=>{d.open=expanded.has(d.dataset.continuityDetail);});
      $('[data-continuity]').querySelectorAll('[data-continuity-run]').forEach(b=>{b.onclick=()=>showSource(b.dataset.continuityRun);});
      if(openingFingerprint!==data.opening){markdown($('[data-opening]'),data.opening,openWiki);openingFingerprint=data.opening;}
      $('[data-current]').innerHTML = !snapshot && data.runs.length ? `<h2>Saved run records</h2>${data.runs.filter(r=>!filter||r.ticket===filter).map(r=>`<div class="pj-run"><strong>${esc(r.ticket || r.run_id)} · @${esc(r.agent_handle)}</strong><span>${esc(r.state)} · observed ${esc(date(r.last_seen_at))}</span><p>${esc(r.last_signal)}</p></div>`).join('')}` : '';
      const shown=data.entries.filter(e=>!isWorkerActivity(e)&&(!filter||e.ticket===filter));
      $('[data-empty]').textContent=shown.length ? '' : 'No notes recorded for this selection yet. Project-bound chat turns and agent-authored notes appear here as they are saved. Worker transitions are in Actions.';
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
            const evidence=document.createElement('details');const summary=document.createElement('summary');summary.textContent='Execution details and evidence';const full=document.createElement('div');full.className='pj-evidence';
            let rendered=false;
            evidence.addEventListener('toggle',()=>{if(evidence.open&&!rendered){markdown(full,entry.content,openWiki);rendered=true;}});
            evidence.append(summary,full);contentHost.append(evidence);
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
      if(stopped)return;if(busy){queued=true;return;}busy=true;
      const requested=selected, requestedBefore=actionBefore;
      try {
        const [next,live]=await Promise.all([invoke('project_journal_read',{project:root,path:requested,actionBefore:requestedBefore}),invoke('agent_sessions_list').then(value=>({value:value || []}),()=>({value:[],error:'Session status is unavailable; showing recorded worker state.'}))]);
        sessions=live.value;sessionsError=live.error || '';
        if(stopped || requested!==selected || requestedBefore!==actionBefore)return;data=next;paint();$('[data-sync]').textContent='Journal saved in the Vault · checked '+date(data.observed_at);
        status(data.warning ? 'Capture needs attention: '+data.warning : '');
      }catch(e){if(!stopped){status('Refresh failed: '+String(e));$('[data-continuity-time]').textContent='Current state could not be refreshed. Displayed records are from the last successful read.';if(!data)$('[data-purpose]').textContent='Select a registered project to read its Journal.';}}
      finally{busy=false;if(queued&&!stopped){queued=false;void refresh();}}
    }
    $('[data-date]').onchange=e=>{selected=e.target.value||null;actionBefore=null;fingerprint='';void refresh();};
    $('[data-filter]').onchange=e=>{filter=e.target.value;actionKind='';if(actionBefore!=null){actionBefore=null;void refresh();}paint();};
    $('[data-refresh]').onclick=refresh;
    $('[data-wiki]').onclick=()=>{if(data)openWiki?.(data.documents.some(d=>d.path===data.path)?data.path:null);};
    $('[data-form]').onsubmit=async e=>{
      e.preventDefault();if(!data)return;const button=$('[data-form] button');button.disabled=true;remember();
      const text=$('[data-note]').value;const ticket=$('[data-ticket]').value.trim();const kind=$('[data-kind]').value;
      try{await invoke('project_journal_add',{request:{project:data.project.key,ticket,kind,title:kind==='question'?'Your question':kind==='decision'?'Your decision':'Your note',content:text}});if(stopped)return;$('[data-note]').value='';remember();selected=null;actionBefore=null;fingerprint='';await refresh();}
      catch(err){if(!stopped)status('Note not saved: '+String(err));}finally{if(button.isConnected)button.disabled=false;}
    };
    void refresh();timer=setInterval(()=>{if(!host.isConnected){clearInterval(timer);stopped=true;return;}if(!document.hidden&&host.getClientRects().length)void refresh();},5000);
    const instance={switchTab,dispose(){remember();stopped=true;clearInterval(timer);},refresh};instances.set(host,instance);return instance;
  }
  window.xnautJournal={mount};
})();
