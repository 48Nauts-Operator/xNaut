// Approved Paper R1–R12. Presentation over the existing Journal/continuity stores.
(function () {
  'use strict';
  const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const stamp=s=>{const n=typeof s==='number'?s:Date.parse(s);return Number.isFinite(n)&&n>0?n:null;};
  const when=s=>stamp(s)?new Date(stamp(s)).toLocaleString([], {dateStyle:'medium',timeStyle:'short'}):'Time not recorded';
  const tone=s=>s==='verified'?'success':['failed','blocked','stalled','refused','interrupted'].includes(s)?'failure':'neutral';
  const label=s=>({active:'Working · recorded',review:'In review',verified:'Verified',unknown:'Needs inspection',done:'Done · reported',complete:'Complete · reported'}[s]||String(s||'Unknown').replace(/_/g,' '));
  const button=(text,attr='',cls='')=>`<button class="${cls}" ${attr}>${esc(text)}</button>`;
  const rows=values=>`<dl class="jd-fields">${values.filter(([,v])=>v!==undefined&&v!==null&&v!=='').map(([k,v])=>`<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join('')}</dl>`;
  function toolEvents(text) {
    const found=[];
    const visit=(v,depth=0)=>{
      if(!v||typeof v!=='object'||depth>5||found.length>=200)return;
      const kind=v.type||v.kind;
      if(['tool_use','tool_result','function_call','function_call_output'].includes(kind)){
        found.push({kind,name:v.name||v.tool_use_id||v.call_id||'Tool result',body:JSON.stringify(v.input??v.arguments??v.output??v.content??'',null,2)});return;
      }
      if(Array.isArray(v)){v.slice(0,200).forEach(x=>visit(x,depth+1));return;}
      for(const k of ['message','content','payload','item'])if(v[k])visit(v[k],depth+1);
    };
    for(const line of String(text||'').split('\n').slice(-2000)){try{visit(JSON.parse(line));}catch(_){/* Plain terminal output stays available below. */}}
    return found;
  }
  function mount(host,o) {
    const $=q=>host.querySelector(q), surface=$('.pj');surface.classList.add('jd');
    let data=null,m=null,tab='tickets',mode='list',selected='',returnScope='project',ticketLimit=40,runLimit=20,disposed=false,generation=0,author='',viewKey='';
    let runId='',compareId='',returnTab='tickets',lastScope='',inlineId='';
    const records=new Map(),pending=new Map(),errors=new Map();
    const doc=$('.pj-document'), oldActions=$('[data-panel="actions"]');
    const addPanel=(key,title)=>{const p=document.createElement('section');p.dataset.panel=key;p.id='jd-'+key;p.setAttribute('role','tabpanel');p.setAttribute('aria-label',title);p.hidden=true;doc.append(p);return p;};
    const timeline=addPanel('timeline','Timeline'),agents=addPanel('agents','Agents'),insights=addPanel('insights','Insights');
    timeline.append($('[data-lifecycle]'));
    oldActions.removeAttribute('data-panel');oldActions.hidden=false;oldActions.classList.add('jd-system-actions');
    const actionDetails=document.createElement('details');actionDetails.className='jd-dispatch-history';actionDetails.innerHTML='<summary>Dispatch actions and approved groups</summary>';actionDetails.append(oldActions);timeline.append(actionDetails);
    agents.append($('[aria-label="Deployed agents"]'));
    insights.innerHTML='<section data-insights-summary></section>';insights.append($('[data-calendar-host]'));
    const header=$('.pj-header');header.querySelector('.pj-eyebrow').remove();
    const scope=$('[data-scope-host]');header.prepend(scope);
    const utility=document.createElement('details');utility.className='jd-journal-tools';utility.innerHTML='<summary>Journal history & tools</summary>';utility.append($('[data-purpose]'),$('.pj-toolbar'),$('[data-sync]'));header.append(utility);
    const heading=$('[data-title]');heading.classList.add('jd-title');
    const progress=document.createElement('div');progress.className='jd-progress';heading.after(progress);
    const summary=$('.pj-summary');summary.querySelector('h2').hidden=true;
    const needs=document.createElement('details');needs.className='jd-needs';needs.open=true;needs.innerHTML='<summary data-needs-heading>Needs you</summary><div data-needs-list></div>';summary.after(needs);
    const tickets=$('[data-panel="tickets"]');tickets.prepend(Object.assign(document.createElement('div'),{className:'jd-tickets'}));
    const ticketHost=tickets.querySelector('.jd-tickets');
    const nav=$('.pj-tabs');nav.innerHTML=['tickets','timeline','agents','insights','notes','handoffs'].map(k=>`<button role="tab" data-journal-tab="${k}" aria-selected="${k===tab}" aria-controls="${$('[data-panel="'+k+'"]')?.id||''}" tabindex="${k===tab?0:-1}">${k[0].toUpperCase()+k.slice(1)}</button>`).join('');
    function activate(key){tab=key;o.switchTab(key);needs.open=key==='tickets'&&mode==='list';syncView();}
    nav.querySelectorAll('button').forEach((b,i,bs)=>{b.onclick=()=>activate(b.dataset.journalTab);b.onkeydown=e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();const n=e.key==='Home'?0:e.key==='End'?bs.length-1:(i+(e.key==='ArrowRight'?1:bs.length-1))%bs.length;bs[n].click();bs[n].focus();};});
    const noteTools=document.createElement('div');noteTools.className='jd-note-tools';noteTools.innerHTML='<label>Author <select aria-label="Note author"><option value="">All authors</option></select></label>'; $('[data-entries]').before(noteTools);
    noteTools.querySelector('select').onchange=e=>{author=e.target.value;decorateNotes();};
    function scopeTickets(){return m?window.xnautLifecycle.scoped(m,o.lifecycle?.scope()||'project'):[];}
    function assignments(id){return (data?.continuity?.assignments||[]).filter(a=>a.ticket===id).sort((a,b)=>(runTime(a.run_id)||0)-(runTime(b.run_id)||0)||a.run_id.localeCompare(b.run_id));}
    function runTime(id){const times=m?.events.filter(e=>e.run_id===id).map(e=>stamp(e.at)).filter(Boolean)||[];return times.length?Math.min(...times):null;}
    function runKnown(id){return (data?.continuity?.assignments||[]).some(a=>a.run_id===id)||(data?.console?.deployed||[]).some(a=>a.run_id===id)||m?.events.some(e=>e.run_id===id);}
    function cacheKey(kind,id){return `${data.project.key}:${kind}:${id}`;}
    function record(kind,id){return records.get(cacheKey(kind,id));}
    async function load(kind,id){
      const key=cacheKey(kind,id),project=data.project.key;
      if(records.has(key)||pending.has(key))return;
      if(kind!=='ticket'&&!runKnown(id)){errors.set(key,'No run in this project matches this reference.');return;}
      const epoch=generation;pending.set(key,true);errors.delete(key);renderTickets();renderInline();
      try{
        const r=await window.__TAURI__.core.invoke('project_wiki_source',{project,kind,id});
        if(disposed||epoch!==generation||data.project.key!==project)return;
        if(!r||typeof r.text!=='string')throw Error('Source returned no readable content.');
        let value=null;try{value=JSON.parse(r.text);}catch(_){}
        if(value?.project&&value.project!==project)throw Error('Source does not belong to this project.');
        if(kind==='ticket'&&value?.id&&value.id!==id)throw Error('Ticket source identity did not match.');
        if(kind==='run'&&value?.run_id&&value.run_id!==id)throw Error('Run source identity did not match.');
        records.set(key,{...r,value});if(records.size>80)records.delete(records.keys().next().value);
      }catch(e){if(!disposed&&epoch===generation)errors.set(key,String(e));}
      finally{pending.delete(key);if(!disposed&&epoch===generation){renderTickets();renderInline();}}
    }
    function sourceStatus(kind,id){const key=cacheKey(kind,id);return errors.has(key)?`<p role="status" class="jl-failure">${esc(errors.get(key))}</p>${button('Retry source',`data-reload="${esc(kind)}" data-id="${esc(id)}"`)}`:pending.has(key)?'<p role="status">Loading saved evidence…</p>':'';}
    function openTicket(id,previousScope){
      if(!m?.tickets.some(t=>t.id===id))return;
      if(previousScope)returnScope=previousScope;else if(!o.lifecycle.scope().startsWith('ticket:'))returnScope=o.lifecycle.scope();
      selected=id;mode='ticket';runId='';compareId='';viewKey='';
      if(o.lifecycle.scope()!=='ticket:'+id)o.lifecycle.selectScope('ticket:'+id);
      activate('tickets');renderTickets();void load('ticket',id);
    }
    function inspectRun(id){
      if(!runKnown(id))return;
      if(tab==='timeline'){inlineId=id;renderInline();void load('run',id);return;}
      const a=(data.continuity?.assignments||[]).find(a=>a.run_id===id),e=m.events.find(e=>e.run_id===id),r=(data.console?.deployed||[]).find(r=>r.run_id===id);
      const ticket=a?.ticket||e?.ticket||r?.ticket;if(!ticket)return;
      returnTab=tab;selected=ticket;runId=id;mode='trace';viewKey='';activate('tickets');renderTickets();void load('run',id);
    }
    function renderInline(){
      let panel=timeline.querySelector('.jd-map-inspector');
      if(!inlineId){panel?.remove();return;}
      if(!panel){panel=document.createElement('section');panel.className='jd-map-inspector jl-detail';timeline.prepend(panel);}
      const r=record('run',inlineId)?.value;
      panel.innerHTML=`<div class="jd-detail-heading"><strong>Selected run</strong>${button('Close ×','data-close-inline')}</div>${sourceStatus('run',inlineId)}${rows([['Run',inlineId],['Ticket',r?.ticket],['State',r?.state],['Harness',r?.runtime_id||r?.agent_handle],['Model',r?.model],['Destination',r?.remote_env],['Last signal',r?.last_signal]])}${button('Open trace ›','data-inline-trace','jd-primary')}`;
      panel.querySelector('[data-close-inline]').onclick=()=>{inlineId='';renderInline();};
      panel.querySelector('[data-inline-trace]').onclick=()=>{const id=inlineId;activate('tickets');inspectRun(id);returnTab='timeline';};
      panel.querySelectorAll('[data-reload]').forEach(b=>b.onclick=()=>void load('run',inlineId));
    }
    function summaryMarkup(){
      const ts=scopeTickets(),scope=o.lifecycle?.scope()||'project',verified=ts.filter(t=>t.state==='verified').length;
      const active=ts.filter(t=>t.state==='active').length,blocked=ts.filter(t=>['blocked','stalled'].includes(t.state)).length;
      heading.textContent=scope.startsWith('release:')?'Release '+scope.slice(8):scope.startsWith('ticket:')?scope.slice(7):scope.startsWith('group:')?'Approved task group':data.project.name+' · Live Journal';
      const segments=ts.slice(0,80).map(t=>`<span class="jd-segment jd-${tone(t.state)}" title="${esc(t.id+' · '+label(t.state))}"></span>`).join('');
      progress.innerHTML=`<div class="jd-segments" aria-label="${verified} of ${ts.length} verified">${segments}</div><div class="jd-progress-label"><span>${verified} of ${ts.length} verified</span><span>${blocked?blocked+' blocked':active?active+' active · recorded':'Recorded project state'}</span></div>`;
      const decisions=ts.filter(t=>t.state==='review'||/\b(approval|owner decision|human decision|retry budget|retries exhausted)\b/i.test(t.next_action||''));
      $('[data-needs-heading]').textContent='Needs you · '+decisions.length+(decisions.length===1?' decision':' decisions');
      $('[data-needs-list]').innerHTML=decisions.slice(0,3).map(t=>`<div class="jd-decision"><div><strong>${esc(t.id)} · ${esc(t.state==='review'?'Review evidence':label(t.state))}</strong><small>${esc(t.title)}</small></div>${button(t.state==='review'?'Review ›':'Inspect ›',`data-open-ticket="${esc(t.id)}"`)}</div>`).join('')+(decisions.length>3?button('Show all '+decisions.length+' decisions','data-all-decisions'):'')+(!decisions.length?'<p class="pj-muted">No recorded human decision is pending in this scope.</p>':'');
      needs.querySelectorAll('[data-open-ticket]').forEach(b=>b.onclick=()=>openTicket(b.dataset.openTicket));
      needs.querySelector('[data-all-decisions]')?.addEventListener('click',()=>{mode='list';ticketLimit=Math.max(40,ts.length);activate('tickets');renderTickets();});
    }
    function ticketList(){
      const ts=scopeTickets(),groups=[['Blocked / stalled',['blocked','stalled']],['In review',['review']],['Working',['active']],['Needs inspection',['unknown']],['Verified',['verified']]],known=new Set(groups.flatMap(([,ss])=>ss));
      if(ts.some(t=>!known.has(t.state)))groups.push(['Other recorded work',ts.filter(t=>!known.has(t.state)).map(t=>t.state)]);
      return `<div class="jd-list-heading"><h2>Tickets <small>· ${ts.length}</small></h2>${button('Timeline ›','data-go-timeline')}</div>`+groups.map(([name,states])=>{const group=ts.filter(t=>states.includes(t.state));if(!group.length)return '';return `<section class="jd-ticket-group"><h3 class="jl-${tone(states[0])}">${esc(name)} <small>${group.length}</small></h3>${group.slice(0,ticketLimit).map(t=>`<button class="jd-ticket-row" data-open-ticket="${esc(t.id)}"><span>${esc(t.id)}</span><div><strong>${esc(t.title)}</strong><small>${esc(t.owner||'Unassigned')} · ${assignments(t.id).length} recorded runs</small></div><span aria-hidden="true">›</span></button>`).join('')}${group.length>ticketLimit?button('Show more tickets','data-more-tickets'):''}</section>`;}).join('')+(!ts.length?'<p>No tickets recorded in this scope.</p>':'');
    }
    function ticketDetail(){
      const t=m.tickets.find(t=>t.id===selected);if(!t)return '<p>This ticket is no longer available in the project.</p>';
      const raw=record('ticket',selected)?.value,as=assignments(selected),latest=as.filter(a=>!a.next_run_id).at(-1)||as.at(-1);
      const checks=[...(raw?.body||'').matchAll(/^\s*[-*] \[([ xX])\]\s+(.+)$/gm)];
      return `${button('‹ Back to '+(returnScope.startsWith('release:')?'release':'tickets'),'data-back-list')}<div class="jd-detail-heading"><small>${esc(t.id)}</small><span class="jl-${tone(t.state)}">${esc(label(t.state))}</span></div><h2>${esc(t.title)}</h2>${rows([['Owner',t.owner||'Unassigned'],['Release',raw?.release||t.release||'Unassigned'],['Branch',latest?.branch],['Worktree',latest?.worktree],['Pull request',latest?.pr_url||'None recorded'],['Recorded runs',as.length]])}${sourceStatus('ticket',selected)}<h3>Description ${button('Refresh source',`data-refresh-source="${esc(selected)}"`)}</h3><div data-ticket-body></div>${checks.length?`<h3>Acceptance · ${checks.filter(c=>c[1]!==' ').length} of ${checks.length} checked</h3><small>Ticket checklist · author-reported, not independent verification</small><ul class="jd-checks">${checks.map(c=>`<li>${c[1]!==' '?'✓':'○'} ${esc(c[2])}</li>`).join('')}</ul>`:''}<h3>Next action</h3><p>${esc(t.next_action||'Inspect recorded work and evidence.')}</p><div class="jd-detail-actions">${button('Open trace · '+as.length+' runs','data-open-trace','jd-primary')}${button('Open ticket workspace ↗','data-delivery')}</div><details class="jd-ticket-evidence"><summary>Assignments and source evidence</summary><div data-detail-evidence></div></details>${footer()}`;
    }
    function footer(){return `<nav class="jd-detail-links">${button('Notes ›','data-related="notes"')}${button('Handoffs ›','data-related="handoffs"')}${button('Activity Map ›','data-related="map"')}</nav>`;}
    function trace(){
      const as=assignments(selected),current=runId?record('run',runId)?.value:null,output=runId?record('log',runId):null;
      const es=m.events.filter(e=>e.ticket===selected);
      const runIds=[...new Set([...as.map(a=>a.run_id),...es.map(e=>e.run_id).filter(Boolean)])];
      if(runId&&!runIds.includes(runId))runIds.push(runId);
      const tools=toolEvents(output?.text);
      const a=as.find(a=>a.run_id===runId),prev=a?.previous_run_id||runIds[runIds.indexOf(runId)-1];
      return `${button('‹ '+selected,'data-back-ticket')}<h2>Trace</h2><p class="pj-muted">${runIds.length} recorded runs · failures before admission remain labelled</p>${runIds.slice(0,runLimit).map((id,i)=>{const r=as.find(a=>a.run_id===id),e=es.find(e=>e.run_id===id);return `<details class="jd-attempt" ${id===runId?'open':''} data-attempt="${esc(id)}"><summary><span>Run ${i+1} <small>${esc(r?.owner||e?.actor||'Agent not recorded')} · ${esc(when(runTime(id)))}</small></span><span class="jl-${tone(r?.run_state||e?.state)}">${esc(label(r?.run_state||e?.state))}</span></summary>${id===runId?`${sourceStatus('run',id)}${rows([['Run',id],['Harness',current?.runtime_id||current?.agent_handle],['Model',current?.model||'Not recorded'],['Destination',current?.remote_env||'Not recorded'],['Admission',current?.admission_refused?'Refused before execution':current?.prelaunch_failure?.phase],['Branch',current?.branch||r?.branch],['Commit',current?.last_commit||r?.last_commit],['Last signal',current?.last_signal]])}<div class="jd-run-events">${es.filter(e=>e.run_id===id).slice(0,12).map(e=>`<p><time>${esc(when(e.at))}</time> <strong class="jl-${tone(e.state)}">${esc(label(e.state||e.kind))}</strong><small>${esc(e.detail||'')}</small></p>`).join('')}</div><div class="jd-detail-actions">${button('Saved output',`data-load-log="${esc(id)}"`)}${prev?button('Compare with previous run',`data-compare="${esc(prev)}"`):''}</div>${sourceStatus('log',id)}${tools.length?`<h3>Tool events · ${tools.length}${tools.length===200?' · display limit':''}</h3>${tools.map(t=>`<details class="jd-tool-call"><summary>${esc(t.name)} <small>${esc(t.kind.replace(/_/g,' '))}</small></summary><pre>${esc(t.body)}</pre></details>`).join('')}`:''}${output?`<h3>Saved output ${output.truncated?'· last 128 KiB':''}</h3><pre class="jd-output">${esc(output.text)}</pre>`:''}<details><summary>Raw execution record</summary><pre>${esc(record('run',id)?.text||'Record not loaded.')}</pre></details>`:''}</details>`;}).join('')}${runIds.length>runLimit?button('Show more runs','data-more-runs'):''}${!runIds.length?'<p>No execution records are linked to this ticket.</p>':''}${footer()}`;
    }
    function comparison(){
      const before=record('run',compareId)?.value,after=record('run',runId)?.value;
      const keys=['runtime_id','model','remote_env','state','admission_refused','branch','last_commit','last_signal'];
      return `${button('‹ Trace · '+selected,'data-back-trace')}<h2>Attempt comparison</h2><p class="pj-muted">Recorded fields and saved output. A changed commit is not proof of a successful implementation.</p>${sourceStatus('run',compareId)}${sourceStatus('run',runId)}<div class="jd-compare-header"><span>Before · ${esc(compareId)}</span><span>After · ${esc(runId)}</span></div>${keys.map(key=>{const a=before?.[key],b=after?.[key];if(a==null&&b==null)return '';return `<section class="jd-compare-field"><strong>${esc(key.replace(/_/g,' '))}</strong><div><pre>${esc(a??'Not recorded')}</pre><pre class="${a!==b?'jd-changed':''}">${esc(b??'Not recorded')}</pre></div></section>`;}).join('')}${button('Compare saved output','data-compare-output')}<div class="jd-compare-logs">${[compareId,runId].map(id=>`${sourceStatus('log',id)}${record('log',id)?`<section><h3>${id===compareId?'Before':'After'} output</h3><pre>${esc(record('log',id).text)}</pre></section>`:''}`).join('')}</div>${footer()}`;
    }
    function renderTickets(){
      if(disposed||!m)return;
      const html=mode==='list'?ticketList():mode==='ticket'?ticketDetail():mode==='trace'?trace():comparison();
      if(viewKey===html)return;viewKey=html;
      const scroll=$('.pj-scroll').scrollTop;
      ticketHost.innerHTML=html;
      if(mode==='ticket'){
        const raw=record('ticket',selected)?.value;
        o.markdown(ticketHost.querySelector('[data-ticket-body]'),raw?.body||'No saved description is available.',o.openWiki);
        o.evidence?.(ticketHost.querySelector('[data-detail-evidence]'),selected);
      }
      ticketHost.querySelectorAll('[data-open-ticket]').forEach(b=>b.onclick=()=>openTicket(b.dataset.openTicket));
      ticketHost.querySelector('[data-go-timeline]')?.addEventListener('click',()=>activate('timeline'));
      ticketHost.querySelector('[data-more-tickets]')?.addEventListener('click',()=>{ticketLimit+=40;renderTickets();});
      ticketHost.querySelector('[data-back-list]')?.addEventListener('click',()=>{mode='list';selected='';o.lifecycle.selectScope(returnScope);mode='list';viewKey='';renderTickets();});
      ticketHost.querySelector('[data-back-ticket]')?.addEventListener('click',()=>{if(returnTab==='timeline'||returnTab==='agents'){activate(returnTab);returnTab='tickets';}else openTicket(selected);});
      ticketHost.querySelector('[data-open-trace]')?.addEventListener('click',()=>{mode='trace';runId=assignments(selected).at(-1)?.run_id||'';returnTab='tickets';renderTickets();if(runId)void load('run',runId);});
      ticketHost.querySelectorAll('[data-attempt]').forEach(d=>{d.querySelector('summary').onclick=e=>{e.preventDefault();runId=runId===d.dataset.attempt?'':d.dataset.attempt;renderTickets();if(runId)void load('run',runId);};});
      ticketHost.querySelector('[data-more-runs]')?.addEventListener('click',()=>{runLimit+=20;renderTickets();});
      ticketHost.querySelectorAll('[data-load-log]').forEach(b=>b.onclick=()=>{void load('log',b.dataset.loadLog);renderTickets();});
      ticketHost.querySelector('[data-compare]')?.addEventListener('click',e=>{compareId=e.currentTarget.dataset.compare;mode='compare';renderTickets();void load('run',compareId);});
      ticketHost.querySelector('[data-back-trace]')?.addEventListener('click',()=>{mode='trace';renderTickets();});
      ticketHost.querySelector('[data-compare-output]')?.addEventListener('click',()=>{void load('log',compareId);void load('log',runId);renderTickets();});
      ticketHost.querySelectorAll('[data-reload]').forEach(b=>b.onclick=()=>{records.delete(cacheKey(b.dataset.reload,b.dataset.id));void load(b.dataset.reload,b.dataset.id);});
      ticketHost.querySelectorAll('[data-related]').forEach(b=>b.onclick=()=>{if(selected&&o.lifecycle.scope()!=='ticket:'+selected)o.lifecycle.selectScope('ticket:'+selected);if(b.dataset.related==='map'){activate('timeline');o.lifecycle.setView('map');}else activate(b.dataset.related);});
      ticketHost.querySelector('[data-refresh-source]')?.addEventListener('click',()=>{records.delete(cacheKey('ticket',selected));void load('ticket',selected);});
      ticketHost.querySelector('[data-delivery]')?.addEventListener('click',()=>window.xnautOpenDelivery?.({project:data.project.key,ticket:selected,tab:'tests'}));
      $('.pj-scroll').scrollTop=scroll;
    }
    function decorateNotes(){
      const container=$('[data-entries]'),entries=[...container.querySelectorAll('.pj-entry')];
      const authors=[...new Set(entries.map(e=>e.dataset.actor))].sort();
      const select=noteTools.querySelector('select'),html='<option value="">All authors</option>'+authors.map(a=>`<option>${esc(a)}</option>`).join('');
      if(select.dataset.markup!==html){select.innerHTML=html;select.dataset.markup=html;}select.value=author;
      const groups=new Map([...container.querySelectorAll('[data-note-day]')].map(d=>[d.dataset.noteDay,d]));
      for(const entry of entries){
        const day=stamp(entry.dataset.at)?new Date(stamp(entry.dataset.at)).toISOString().slice(0,10):'Date unknown';
        let g=groups.get(day);if(!g){g=document.createElement('details');g.className='jd-note-day';g.dataset.noteDay=day;g.open=true;g.append(document.createElement('summary'));groups.set(day,g);container.append(g);}g.append(entry);
        entry.hidden=Boolean(author&&author!==entry.dataset.actor);
        if(!entry.querySelector('[data-copy-note]')){const b=document.createElement('button');b.dataset.copyNote='';b.textContent='Copy';b.onclick=async()=>{try{await navigator.clipboard.writeText(entry.dataset.content||'');b.textContent='Copied';}catch(_){b.textContent='Copy unavailable';}};entry.querySelector('.pj-links').append(b);}
      }
      for(const [day,g] of groups){const count=[...g.querySelectorAll('.pj-entry')].filter(n=>!n.hidden).length;g.hidden=!count;g.querySelector('summary').textContent=day+' · '+count+' notes';if(!g.querySelector('.pj-entry'))g.remove();}
      [...container.querySelectorAll('[data-note-day]')].sort((a,b)=>b.dataset.noteDay.localeCompare(a.dataset.noteDay)).forEach(g=>container.append(g));
    }
    function paintAgents(sessions,error){
      const panel=agents.querySelector('[data-agents]');panel.classList.add('jd-agents');
      panel.querySelectorAll('[data-agent-run]').forEach(el=>{
        const r=(data.console?.deployed||[]).find(r=>r.run_id===el.dataset.agentRun);if(!r)return;
        el.querySelectorAll('.jd-agent-evidence').forEach(n=>n.remove());
        const found=sessions.filter(s=>s.session_id===r.pty_session||(r.zellij_session&&s.zellij_session===r.zellij_session&&s.remote_env===(r.destination==='local'?null:r.destination)));
        const live=found.length===1?found[0]:null,observed=live?.last_output_at_ms||r.observed_at,stale=!stamp(observed)||Date.now()-stamp(observed)>60000;
        const note=document.createElement('div');note.className='jd-agent-evidence';note.innerHTML=`<small>${esc(when(observed))}${stale?' · observation is stale':''}${live?' · session matched':' · no live session match'}</small><p>${esc(r.signal||'No progress message recorded.')}</p>`;el.querySelector('.pj-agent-links').before(note);
        const state=el.querySelector('.pj-agent-state');if(stale)state.textContent=label(live?.status||r.state)+' · stale';
      });
      let fresh=agents.querySelector('.jd-agent-freshness');if(!fresh){fresh=document.createElement('p');fresh.className='jd-agent-freshness pj-muted';panel.before(fresh);}
      fresh.textContent=error||'Observed '+when(data.observed_at)+' · state is recorded evidence; stale observations do not establish liveness.';
    }
    function paintInsights(){
      const ts=scopeTickets(),ids=new Set(ts.map(t=>t.id)),es=m.events.filter(e=>ids.has(e.ticket)),verified=ts.filter(t=>t.state==='verified').length;
      const dates=new Map();for(const e of es){if(!stamp(e.at))continue;const d=new Date(stamp(e.at)).toISOString().slice(0,10);dates.set(d,(dates.get(d)||0)+1);}
      const latest=[...dates].sort(([a],[b])=>a.localeCompare(b)).slice(-14),max=Math.max(1,...latest.map(([,n])=>n));
      const chart=latest.length?`<div class="jd-spark" aria-label="Recorded events by date">${latest.map(([d,n])=>`<div><i style="height:${Math.max(3,n/max*64)}px" title="${d}: ${n} events"></i><small>${esc(d.slice(5))}</small></div>`).join('')}</div>`:'<p>No dated evidence available.</p>';
      $('[data-insights-summary]').innerHTML=`<h2>Insights</h2><section class="jd-forecast"><h3>Release forecast</h3><p class="pj-muted">No evidence-backed completion forecast is recorded. ${ts.filter(t=>['blocked','stalled'].includes(t.state)).length} blocked or stalled tickets in this scope.</p></section><h3>Recorded activity trend</h3>${chart}<p class="pj-muted">${m.native?.limited?'Loaded history only. ':''}Event volume is not a measure of productivity.</p><div class="jd-metrics"><div><span>Verified tickets</span><strong>${verified} of ${ts.length}</strong></div><div><span>Recorded runs</span><strong>${new Set(es.map(e=>e.run_id).filter(Boolean)).size}</strong></div><div><span>Recorded events</span><strong>${es.length}</strong></div><div><span>Spend / token allowance</span><span>Not recorded for this scope</span></div></div>`;
    }
    function syncView(){actionDetails.hidden=o.lifecycle?.view()!=='event log';}
    function update(next,sessions,error){
      if(disposed)return;
      if(data&&data.project.key!==next.project.key){generation++;records.clear();errors.clear();inlineId='';renderInline();selected='';mode='list';viewKey='';}
      data=next;m=window.xnautLifecycle.model(data);
      const scope=o.lifecycle?.scope()||'project';
      if(scope!==lastScope){if(lastScope&&!lastScope.startsWith('ticket:'))returnScope=lastScope;inlineId='';renderInline();lastScope=scope;viewKey='';if(scope.startsWith('ticket:')){selected=scope.slice(7);mode='ticket';void load('ticket',selected);}else{selected='';mode='list';}}
      summaryMarkup();renderTickets();paintAgents(sessions,error);paintInsights();syncView();decorateNotes();
    }
    activate('tickets');
    return {update,openTicket,inspectRun,decorateNotes,syncView,dispose(){disposed=true;generation++;records.clear();pending.clear();}};
  }
  window.xnautJournalDesign={mount,toolEvents};
})();
