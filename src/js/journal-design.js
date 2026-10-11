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
    let notesMonth=new Date().toISOString().slice(0,7),monthRows=null,monthLoading=false,monthError='',monthLimited=false,monthRequest=0;
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
    const projectName=document.createElement('span');projectName.className='jd-project-name';scope.prepend(projectName);
    const utility=document.createElement('details');utility.className='jd-journal-tools';utility.innerHTML='<summary aria-label="Journal history and tools" title="Journal history and tools">⋯</summary><div class="jd-tools-popover"></div>';utility.querySelector('div').append($('[data-purpose]'),$('.pj-toolbar'),$('[data-sync]'),$('[data-continuity-time]'));scope.append(utility);
    const heading=$('[data-title]');heading.classList.add('jd-title');
    const hero=document.createElement('section');hero.className='jd-hero';hero.innerHTML='<small class="jd-scope-label">Release</small><div class="jd-hero-heading"><span data-scope-state></span></div>';header.append(hero);hero.querySelector('div').prepend(heading);
    const progress=document.createElement('div');progress.className='jd-progress';hero.append(progress);
    const summary=$('.pj-summary');summary.querySelector('h2').hidden=true;
    const needs=document.createElement('details');needs.className='jd-needs';needs.open=true;needs.innerHTML='<summary data-needs-heading>Needs you</summary><div data-needs-list></div>';summary.after(needs);
    const tickets=$('[data-panel="tickets"]');tickets.prepend(Object.assign(document.createElement('div'),{className:'jd-tickets'}));
    const ticketHost=tickets.querySelector('.jd-tickets');
    const nav=$('.pj-tabs');nav.innerHTML=['tickets','timeline','agents','insights','notes','handoffs'].map(k=>`<button role="tab" data-journal-tab="${k}" aria-selected="${k===tab}" aria-controls="${$('[data-panel="'+k+'"]')?.id||''}" tabindex="${k===tab?0:-1}">${k[0].toUpperCase()+k.slice(1)}</button>`).join('');
    function activate(key){tab=key;o.switchTab(key);needs.open=key==='tickets'&&mode==='list';syncView();syncStructure();if(key==='notes'&&monthRows===null&&!monthLoading)void loadMonth();}
    nav.querySelectorAll('button').forEach((b,i,bs)=>{b.onclick=()=>activate(b.dataset.journalTab);b.onkeydown=e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();const n=e.key==='Home'?0:e.key==='End'?bs.length-1:(i+(e.key==='ArrowRight'?1:bs.length-1))%bs.length;bs[n].click();bs[n].focus();};});
    const notes=$('[data-panel="notes"]');notes.querySelector('h2').textContent='Working notes';
    const notesIntro=document.createElement('p');notesIntro.className='jd-section-subtitle';notesIntro.textContent='Decisions · context · your observations';notes.querySelector('h2').after(notesIntro);
    const noteTools=document.createElement('div');noteTools.className='jd-note-tools';noteTools.innerHTML='<select aria-label="Note author"><option value="">All authors</option></select><select aria-label="Notes month"></select><button data-add-note>+ Add note</button>';notesIntro.after(noteTools);
    noteTools.querySelector('[data-add-note]').onclick=()=>{const form=$('.pj-notes');form.open=true;if(form.open){form.scrollIntoView({block:'nearest'});$('[data-note]').focus();}};
    noteTools.querySelector('[aria-label="Notes month"]').onchange=e=>{notesMonth=e.target.value;monthRows=null;void loadMonth();};
    $('.pj-notes').open=true;$('.pj-notes>summary').textContent='New note';$('[data-entries]').after($('.pj-notes'));utility.querySelector('div').append($('.pj-saved-context'));
    const handoffs=$('[data-panel="handoffs"]');handoffs.querySelector('h2').textContent='Handoffs';const handoffIntro=document.createElement('p');handoffIntro.className='jd-section-subtitle';handoffIntro.textContent='Who handed over what · evidence and next action';handoffs.querySelector('h2').after(handoffIntro);
    function syncStructure(){surface.dataset.detail=String(tab==='tickets'&&mode!=='list');surface.dataset.section=tab;surface.dataset.mode=mode;}

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
        const assignedTicket=(data.continuity?.assignments||[]).find(a=>a.run_id===id)?.ticket;
        if(kind==='run'&&assignedTicket&&value?.ticket&&value.ticket!==assignedTicket)throw Error('Run source ticket did not match its recorded assignment.');
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
      if(!panel){panel=document.createElement('section');panel.className='jd-map-inspector jl-detail';timeline.append(panel);}
      const r=record('run',inlineId)?.value;
      const a=(data.continuity?.assignments||[]).find(a=>a.run_id===inlineId);
      panel.innerHTML=`<div class="jd-detail-heading"><small>Selected · ${esc(a?.ticket||r?.ticket||'run')}</small>${button('Close ×','data-close-inline')}</div>${sourceStatus('run',inlineId)}<h3 class="jl-${tone(r?.state)}">${esc(label(r?.state||a?.run_state||a?.state))}</h3><p class="pj-muted">${esc(r?.runtime_id||r?.agent_handle||a?.owner||'Agent unknown')} · ${esc(r?.model||'Model not recorded')} · ${esc(r?.remote_env||'Destination not recorded')}</p><p>${esc(r?.last_signal||'No progress message recorded.')}</p>${button('Open trace ›','data-inline-trace','jd-primary')}<details class="jd-inline-record"><summary>Execution details</summary>${rows([['Run',inlineId],['Ticket',r?.ticket],['State',r?.state],['Harness',r?.runtime_id||r?.agent_handle],['Model',r?.model],['Destination',r?.remote_env]])}</details>`;
      panel.querySelector('[data-close-inline]').onclick=()=>{inlineId='';renderInline();};
      panel.querySelector('[data-inline-trace]').onclick=()=>{const id=inlineId;activate('tickets');inspectRun(id);returnTab='timeline';};
      panel.querySelectorAll('[data-reload]').forEach(b=>b.onclick=()=>void load('run',inlineId));
    }
    function summaryMarkup(){
      const ts=scopeTickets(),scope=o.lifecycle?.scope()||'project',verified=ts.filter(t=>t.state==='verified').length;
      const active=ts.filter(t=>t.state==='active').length,blocked=ts.filter(t=>['blocked','stalled'].includes(t.state)).length;
      projectName.textContent=data.project.name;
      hero.hidden=!scope.startsWith('release:');
      heading.textContent=scope.startsWith('release:')?scope.slice(8):scope.startsWith('ticket:')?scope.slice(7):data.project.name;
      hero.querySelector('[data-scope-state]').textContent=blocked?'● At risk':ts.some(t=>t.state==='review')?'● Review pending':verified===ts.length&&ts.length?'● Verified':'● In progress';
      hero.querySelector('[data-scope-state]').className=verified===ts.length&&ts.length?'jl-success':'';
      const allOption=scopeSelectOption();if(allOption)allOption.textContent='All tickets';
      const pin=scopeEl().querySelector('[data-pin]');if(pin)pin.textContent=pin.getAttribute('aria-pressed')==='true'?'Pinned · Unpin':'Following · Pin';
      const monthSelect=noteTools.querySelector('[aria-label="Notes month"]');
      const months=[...new Set([notesMonth,new Date().toISOString().slice(0,7),...(data.documents||[]).map(d=>(d.path.match(/(\d{4}-\d{2})-\d{2}\.md$/)||[])[1]).filter(Boolean)])].sort().reverse();
      monthSelect.innerHTML=months.map(v=>`<option value="${v}">${new Date(v+'-01T12:00:00Z').toLocaleDateString([], {month:'long',year:'numeric',timeZone:'UTC'})}</option>`).join('');monthSelect.value=notesMonth;
      function scopeSelectOption(){return scopeEl().querySelector('option[value="project"]');}
      function scopeEl(){return host.querySelector('[data-scope-host]');}
      const segments=[...ts].sort((a,b)=>(['verified','review','active','blocked','stalled','unknown'].indexOf(a.state))-(['verified','review','active','blocked','stalled','unknown'].indexOf(b.state))).slice(0,80).map(t=>`<span class="jd-segment jd-${tone(t.state)} jd-segment-${esc(t.state)}" title="${esc(t.id+' · '+label(t.state))}"></span>`).join('');
      progress.innerHTML=`<div class="jd-segments" aria-label="${verified} of ${ts.length} verified">${segments}</div><div class="jd-progress-label"><span>${verified} of ${ts.length} verified</span><span>${active?active+' working':blocked?blocked+' blocked':'Current evidence'}</span></div>`;
      const decisions=ts.filter(t=>t.state==='review'||/\b(approval|owner decision|human decision|retry budget|retries exhausted)\b/i.test(t.next_action||''));
      $('[data-needs-heading]').textContent='Needs you · '+decisions.length+(decisions.length===1?' decision':' decisions');
      $('[data-needs-list]').innerHTML=decisions.slice(0,3).map(t=>`<div class="jd-decision"><div><strong>${esc(t.id)} · ${esc(t.state==='review'?'Review evidence':label(t.state))}</strong><small>${esc(t.title)}</small></div>${button(t.state==='review'?'Review ›':'Inspect ›',`data-open-ticket="${esc(t.id)}"`)}</div>`).join('')+(decisions.length>3?button('Show all '+decisions.length+' decisions','data-all-decisions'):'')+(!decisions.length?'<p class="pj-muted">No recorded human decision is pending in this scope.</p>':'');
      needs.querySelectorAll('[data-open-ticket]').forEach(b=>b.onclick=()=>openTicket(b.dataset.openTicket));
      needs.querySelector('[data-all-decisions]')?.addEventListener('click',()=>{mode='list';ticketLimit=Math.max(40,ts.length);activate('tickets');renderTickets();});
    }
    function ticketList(){
      const ts=scopeTickets(),groups=[['Blocked / stalled',['blocked','stalled']],['In review',['review']],['Working',['active']],['Needs inspection',['unknown']],['Verified',['verified']]],known=new Set(groups.flatMap(([,ss])=>ss));
      if(ts.some(t=>!known.has(t.state)))groups.push(['Other recorded work',ts.filter(t=>!known.has(t.state)).map(t=>t.state)]);
      return `<div class="jd-list-heading"><small>Ticket / current work</small><small>Recorded runs</small></div>`+groups.map(([name,states])=>{const group=ts.filter(t=>states.includes(t.state));if(!group.length)return '';return `<${states[0]==='verified'?'details':'section'} class="jd-ticket-group ${states[0]==='verified'?'jd-verified-group':''}">${states[0]==='verified'?'<summary>':''}<h3 class="jl-${tone(states[0])} jd-group-${states[0]}"><i class="jd-state-dot"></i>${esc(name)} <small>${group.length}</small></h3>${states[0]==='verified'?'<span>Show ›</span></summary>':''}${group.slice(0,ticketLimit).map(t=>`<button class="jd-ticket-row" data-open-ticket="${esc(t.id)}"><span>${esc(t.id)}</span><div><strong>${esc(t.title)}</strong><small>${esc(t.owner||'Unassigned')}</small></div><span class="jd-run-count" aria-label="${assignments(t.id).length} recorded runs">${assignments(t.id).length}</span></button>`).join('')}${group.length>ticketLimit?button('Show more tickets','data-more-tickets'):''}</section>`;}).join('')+(!ts.length?'<p>No tickets recorded in this scope.</p>':'');
    }
    function ticketDescription(body){
      const source=String(body||''),match=source.match(/^##? (?:Outcome|Description)\s*\n([\s\S]*?)(?=^##? |$(?![\s\S]))/im);
      return (match?.[1]||source.split(/\n(?=#{1,3} )/)[0]).replace(/^\s*[-*] \[[ xX]\].*$/gm,'').trim();
    }
    function ticketDetail(){
      const t=m.tickets.find(t=>t.id===selected);if(!t)return '<p>This ticket is no longer available in the project.</p>';
      const raw=record('ticket',selected)?.value,as=assignments(selected),latest=as.filter(a=>!a.next_run_id).at(-1)||as.at(-1);
      const checks=[...(raw?.body||'').matchAll(/^\s*[-*] \[([ xX])\]\s+(.+)$/gm)];
      const evidence=(latest?.evidence||[]).filter(e=>['verification','review','handback','transfer'].includes(e.kind)).slice(-3);
      const events=m.events.filter(e=>e.ticket===selected).sort((a,b)=>(stamp(a.at)||0)-(stamp(b.at)||0)).slice(-5);
      return `${button('‹ Back to '+(returnScope.startsWith('release:')?'release':'tickets'),'data-back-list')}<div class="jd-detail-heading"><small>${esc(t.id)}</small><span class="jl-${tone(t.state)}">● ${esc(label(t.state))}</span></div><h2>${esc(t.title)}</h2>${rows([['Agent',t.owner||'Unassigned'],['Branch',latest?.branch],['Pull request',latest?.pr_url||'None recorded'],['Recorded runs',as.length]])}${sourceStatus('ticket',selected)}<h3>Description ${button('Refresh source',`data-refresh-source="${esc(selected)}"`)}</h3><div data-ticket-body></div>${checks.length?`<h3>Acceptance · ${checks.filter(c=>c[1]!==' ').length} of ${checks.length} checked</h3><small class="jd-check-authority">Author-reported · not independent verification</small><ul class="jd-checks">${checks.map(c=>`<li><span class="${c[1]!==' '?'jd-check-done':'jd-check-pending'}" role="img" aria-label="${c[1]!==' '?'Checked by author':'Not checked'}">${c[1]!==' '?'✓':'–'}</span>${esc(c[2])}</li>`).join('')}</ul>`:''}<h3>Latest evidence</h3><div class="jd-latest-evidence">${evidence.length?evidence.map(e=>`<p><span>${esc(label(e.kind))}</span> ${esc(e.detail||e.source||'Saved record')}</p>`).join(''):'<p class="pj-muted">No independent verification result is linked to the latest run.</p>'}</div><h3>What happened</h3><div class="jd-history">${events.length?events.map(e=>`<div><i class="jd-state-dot jl-${tone(e.state)}"></i><time title="${esc(when(e.at))}">${esc(stamp(e.at)?new Date(stamp(e.at)).toLocaleString([], {weekday:'short',hour:'2-digit',minute:'2-digit'}):'Time unknown')}</time><span>${esc(e.detail||label(e.state||e.kind))}</span></div>`).join(''):'<p class="pj-muted">No dated history recorded.</p>'}</div><div class="jd-trace-link">${button('Open trace','data-open-trace')}<small>${as.length} recorded runs</small><span>›</span></div><div class="jd-next-action"><p>${esc(t.next_action||'Inspect recorded work and evidence.')}</p>${button('Open ticket workspace ↗','data-delivery','jd-primary')}</div><details class="jd-full-description"><summary>Complete ticket description</summary><div data-full-ticket-body></div></details><details class="jd-ticket-evidence"><summary>Assignments and source evidence</summary><div data-detail-evidence></div></details>${footer()}`;
    }
    function footer(){return `<nav class="jd-detail-links">${button('Notes ›','data-related="notes"')}${button('Handoffs ›','data-related="handoffs"')}${button('Activity Map ›','data-related="map"')}</nav>`;}
    function trace(){
      const as=assignments(selected),current=runId?record('run',runId)?.value:null,output=runId?record('log',runId):null;
      const es=m.events.filter(e=>e.ticket===selected);
      const runIds=[...new Set([...as.map(a=>a.run_id),...es.map(e=>e.run_id).filter(Boolean)])];
      if(runId&&!runIds.includes(runId))runIds.push(runId);
      const tools=toolEvents(output?.text),a=as.find(a=>a.run_id===runId),prev=a?.previous_run_id||runIds[runIds.indexOf(runId)-1];
      const findings=(a?.evidence||[]).filter(e=>['verification','review','handback'].includes(e.kind));
      return `${button('‹ '+selected,'data-back-ticket')}<h2>Trace</h2><p class="jd-section-subtitle">${runIds.length} attempts · saved execution evidence</p><div class="jd-trace-path">${esc(a?.owner||'Agent')} <span>›</span> ${esc(label(a?.run_state||a?.state))} <span>›</span> Review</div>${runIds.slice(0,runLimit).map((id,i)=>{const r=as.find(a=>a.run_id===id),e=es.find(e=>e.run_id===id);return `<details class="jd-attempt" ${id===runId?'open':''} data-attempt="${esc(id)}"><summary><span class="jd-chevron">${id===runId?'⌄':'›'}</span><span>Attempt ${i+1}<small>${esc(r?.owner||e?.actor||'Agent not recorded')} · ${esc(when(runTime(id)))}</small></span><span class="jl-${tone(r?.run_state||e?.state)}">${esc(label(r?.run_state||e?.state))}</span></summary>${id===runId?`${sourceStatus('run',id)}<div class="jd-attempt-body"><p class="jd-run-model">${esc(current?.runtime_id||current?.agent_handle||r?.owner||'Harness unknown')} · ${esc(current?.model||'Model not recorded')} · ${esc(current?.remote_env||'Destination not recorded')}</p>${current?.admission_refused?'<p class="jl-failure">Refused before execution</p>':''}<div class="jd-run-events">${es.filter(e=>e.run_id===id).slice(0,8).map(e=>`<div><time>${esc(stamp(e.at)?new Date(stamp(e.at)).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'}):'Unknown')}</time><span class="jl-${tone(e.state)}">${esc(e.detail||label(e.state||e.kind))}</span></div>`).join('')}</div><h3>Worth a look · ${findings.length||1}</h3>${findings.length?findings.map(e=>`<div class="jd-finding"><i class="jd-state-dot"></i><div>${esc(e.detail||e.kind)}<small>${esc(e.source)}</small></div></div>`).join(''):`<div class="jd-finding"><i class="jd-state-dot"></i><div>${esc(current?.last_signal||'No verification finding is recorded.')}<small>Last recorded signal</small></div></div>`}<h3>Tool output${tools.length?' · '+tools.length+' records':''}</h3>${output?tools.length?tools.map(t=>`<details class="jd-tool-call"><summary>${esc(t.name)} <small>${esc(t.kind.replace(/_/g,' '))}</small></summary><pre>${esc(t.body)}</pre></details>`).join(''):`<pre class="jd-output">${esc(output.text)}</pre>`:'<p class="pj-muted">Open saved output to inspect the commands and results.</p>'}${sourceStatus('log',id)}${button(output?'Refresh saved output':'Saved output',`data-load-log="${esc(id)}"`)}<details class="jd-run-metadata"><summary>Execution details</summary>${rows([['Run',id],['Admission',current?.admission_refused?'Refused before execution':current?.prelaunch_failure?.phase],['Branch',current?.branch||r?.branch],['Commit',current?.last_commit||r?.last_commit]])}<details><summary>Raw execution record</summary><pre>${esc(record('run',id)?.text||'Record not loaded.')}</pre></details></details>${prev?`<div class="jd-trace-compare">${button('Compare with attempt '+runIds.indexOf(runId),`data-compare="${esc(prev)}"`)}</div>`:''}</div>`:''}</details>`;}).join('')}${runIds.length>runLimit?button('Show more runs','data-more-runs'):''}${!runIds.length?'<p>No execution records are linked to this ticket.</p>':''}${footer()}`;
    }
    function comparison(){
      const before=record('run',compareId)?.value,after=record('run',runId)?.value,as=assignments(selected);
      const keys=['runtime_id','model','remote_env','state','admission_refused','branch','last_commit','last_signal'];
      const changed=keys.filter(k=>before?.[k]!==after?.[k]);
      const field=key=>{const a=before?.[key],b=after?.[key];if(a==null&&b==null)return '';return `<section class="jd-compare-field"><strong>${esc(key.replace(/_/g,' '))}</strong><div><pre>${esc(a??'Not recorded')}</pre><span>→</span><pre class="${a!==b?'jd-changed':''}">${esc(b??'Not recorded')}</pre></div></section>`;};
      return `${button('‹ Trace · '+selected,'data-back-trace')}<h2>Attempt ${Math.max(1,as.findIndex(a=>a.run_id===compareId)+1)} → ${Math.max(1,as.findIndex(a=>a.run_id===runId)+1)}</h2><p class="jd-section-subtitle">${changed.length} changed fields · ${esc(label(after?.state))}</p>${sourceStatus('run',compareId)}${sourceStatus('run',runId)}<div class="jd-comparison-changes">${changed.map(field).join('')}</div><details class="jd-unchanged"><summary>Unchanged fields · ${keys.length-changed.length}</summary>${keys.filter(k=>!changed.includes(k)).map(field).join('')}</details><h3>Changes</h3><p class="pj-muted">Saved output from each attempt. A changed commit alone does not verify the work.</p>${button('Compare saved output','data-compare-output')}<div class="jd-compare-logs">${[compareId,runId].map(id=>`${sourceStatus('log',id)}${record('log',id)?`<section><h3>${id===compareId?'Before':'After'} output</h3><pre>${esc(record('log',id).text)}</pre></section>`:''}`).join('')}</div><div class="jd-detail-actions">${button('Open ticket workspace ↗','data-delivery','jd-primary')}${button('Back to trace','data-return-trace')}</div>${footer()}`;
    }
    let handoffTicket='',handoffState='',handoffMarkup='';
    function paintHandoffs(){
      const target=$('[data-handoffs]'),ids=new Set(scopeTickets().map(t=>t.id));
      const all=(data.continuity?.assignments||[]).filter(a=>ids.has(a.ticket)&&(a.pr_url||(a.evidence||[]).some(e=>['handback','transfer','review','verification'].includes(e.kind))));
      const filtered=all.filter(a=>(!handoffTicket||a.ticket===handoffTicket)&&(!handoffState||(handoffState==='verified'?a.state==='verified':a.state!=='verified')));
      const docs=data.console?.project===data.project.key?data.console.handoffs||[]:[];
      const html=`<div class="jd-handoff-filters"><select aria-label="Handoff ticket"><option value="">All tickets</option>${[...new Set(all.map(a=>a.ticket))].map(id=>`<option ${id===handoffTicket?'selected':''}>${esc(id)}</option>`).join('')}</select><select aria-label="Handoff state"><option value="">All recorded states</option><option value="pending" ${handoffState==='pending'?'selected':''}>Awaiting review</option><option value="verified" ${handoffState==='verified'?'selected':''}>Verified evidence</option></select></div>${filtered.slice(0,40).map((a,i)=>`<details class="jd-handoff" data-handoff="${esc(a.run_id)}" ${i<2?'open':''}><summary><time>${esc(when(runTime(a.run_id)))}</time><span class="jl-${tone(a.state)}">${esc(label(a.state))}</span></summary><h3>${esc(a.ticket)} · ${esc(a.owner||'Agent not recorded')}</h3><p class="jl-${tone(a.run_state||a.state)}">${esc(label(a.run_state||a.state))} · recorded outcome</p><p>${esc(a.next_action||(a.evidence||[]).at(-1)?.detail||'Inspect the linked work and evidence.')}</p><div class="jd-handoff-links">${button('Evidence ›',`data-handoff-evidence="${esc(a.run_id)}"`)}${button('Open trace ›',`data-handoff-trace="${esc(a.run_id)}"`)}${a.previous_run_id?button('Compare attempts ›',`data-handoff-compare="${esc(a.run_id)}"`):''}</div><div class="jd-handoff-actions">${button('Open ticket',`data-handoff-ticket="${esc(a.ticket)}"`)}</div><div data-handoff-evidence-body="${esc(a.run_id)}" hidden></div></details>`).join('')}${!filtered.length?'<p class="pj-muted">No handoff evidence recorded for this selection.</p>':''}${filtered.length>40?'<p class="pj-muted">Showing 40 records. Select a ticket to narrow this view.</p>':''}${docs.length?`<details class="jd-handoff-docs"><summary>Project handoff documents · ${docs.length}</summary>${docs.map(d=>button((d.title||d.path)+' ↗',`data-handoff-wiki="${esc(d.path)}"`)).join('')}</details>`:''}`;
      if(html===handoffMarkup&&target.querySelector('.jd-handoff-filters'))return;handoffMarkup=html;target.innerHTML=html;
      target.querySelector('[aria-label="Handoff ticket"]').onchange=e=>{handoffTicket=e.target.value;paintHandoffs();};target.querySelector('[aria-label="Handoff state"]').onchange=e=>{handoffState=e.target.value;paintHandoffs();};
      target.querySelectorAll('[data-handoff-ticket]').forEach(b=>b.onclick=()=>openTicket(b.dataset.handoffTicket));
      target.querySelectorAll('[data-handoff-trace]').forEach(b=>b.onclick=()=>{inspectRun(b.dataset.handoffTrace);returnTab='handoffs';});
      target.querySelectorAll('[data-handoff-compare]').forEach(b=>b.onclick=()=>{const a=all.find(a=>a.run_id===b.dataset.handoffCompare);inspectRun(a.run_id);compareId=a.previous_run_id;mode='compare';renderTickets();void load('run',compareId);});
      target.querySelectorAll('[data-handoff-evidence]').forEach(b=>b.onclick=()=>{const a=all.find(a=>a.run_id===b.dataset.handoffEvidence),body=[...target.querySelectorAll('[data-handoff-evidence-body]')].find(el=>el.dataset.handoffEvidenceBody===a.run_id);body.hidden=!body.hidden;if(!body.hidden)o.evidence?.(body,a.ticket);});
      target.querySelectorAll('[data-handoff-wiki]').forEach(b=>b.onclick=()=>o.openWiki?.(b.dataset.handoffWiki));
    }
    function renderTickets(){
      if(disposed||!m)return;
      syncStructure();
      const html=mode==='list'?ticketList():mode==='ticket'?ticketDetail():mode==='trace'?trace():comparison();
      if(viewKey===html)return;viewKey=html;
      const scroll=$('.pj-scroll').scrollTop;
      ticketHost.innerHTML=html;
      if(mode==='ticket'){
        const raw=record('ticket',selected)?.value;
        o.markdown(ticketHost.querySelector('[data-ticket-body]'),ticketDescription(raw?.body)||'No saved description is available.',o.openWiki);
        const full=ticketHost.querySelector('.jd-full-description');let rendered=false;full.ontoggle=()=>{if(full.open&&!rendered){o.markdown(full.querySelector('[data-full-ticket-body]'),raw?.body||'No saved description is available.',o.openWiki);rendered=true;}};
        o.evidence?.(ticketHost.querySelector('[data-detail-evidence]'),selected);
      }
      ticketHost.querySelectorAll('[data-open-ticket]').forEach(b=>b.onclick=()=>openTicket(b.dataset.openTicket));
      ticketHost.querySelector('[data-go-timeline]')?.addEventListener('click',()=>activate('timeline'));
      ticketHost.querySelector('[data-more-tickets]')?.addEventListener('click',()=>{ticketLimit+=40;renderTickets();});
      ticketHost.querySelector('[data-back-list]')?.addEventListener('click',()=>{mode='list';selected='';o.lifecycle.selectScope(returnScope);mode='list';viewKey='';renderTickets();});
      ticketHost.querySelector('[data-back-ticket]')?.addEventListener('click',()=>{if(returnTab==='timeline'||returnTab==='agents'||returnTab==='handoffs'){activate(returnTab);returnTab='tickets';}else openTicket(selected);});
      ticketHost.querySelector('[data-open-trace]')?.addEventListener('click',()=>{mode='trace';runId=assignments(selected).at(-1)?.run_id||'';returnTab='tickets';renderTickets();if(runId)void load('run',runId);});
      ticketHost.querySelectorAll('[data-attempt]').forEach(d=>{d.querySelector('summary').onclick=e=>{e.preventDefault();runId=runId===d.dataset.attempt?'':d.dataset.attempt;renderTickets();if(runId)void load('run',runId);};});
      ticketHost.querySelector('[data-more-runs]')?.addEventListener('click',()=>{runLimit+=20;renderTickets();});
      ticketHost.querySelectorAll('[data-load-log]').forEach(b=>b.onclick=()=>{void load('log',b.dataset.loadLog);renderTickets();});
      ticketHost.querySelector('[data-compare]')?.addEventListener('click',e=>{compareId=e.currentTarget.dataset.compare;mode='compare';renderTickets();void load('run',compareId);});
      ticketHost.querySelectorAll('[data-back-trace],[data-return-trace]').forEach(b=>b.onclick=()=>{mode='trace';renderTickets();});
      ticketHost.querySelector('[data-compare-output]')?.addEventListener('click',()=>{void load('log',compareId);void load('log',runId);renderTickets();});
      ticketHost.querySelectorAll('[data-reload]').forEach(b=>b.onclick=()=>{records.delete(cacheKey(b.dataset.reload,b.dataset.id));void load(b.dataset.reload,b.dataset.id);});
      ticketHost.querySelectorAll('[data-related]').forEach(b=>b.onclick=()=>{if(selected&&o.lifecycle.scope()!=='ticket:'+selected)o.lifecycle.selectScope('ticket:'+selected);if(b.dataset.related==='map'){activate('timeline');o.lifecycle.setView('map');}else activate(b.dataset.related);});
      ticketHost.querySelector('[data-refresh-source]')?.addEventListener('click',()=>{records.delete(cacheKey('ticket',selected));void load('ticket',selected);});
      ticketHost.querySelector('[data-delivery]')?.addEventListener('click',()=>window.xnautOpenDelivery?.({project:data.project.key,ticket:selected,tab:'tests'}));
      $('.pj-scroll').scrollTop=scroll;
    }
    async function loadMonth(){
      if(!data)return;const request=++monthRequest,project=data.project.key,month=notesMonth;
      monthLoading=true;monthError='';o.repaint();notesStatus();
      try{
        const result=await window.__TAURI__.core.invoke('project_journal_notes_month',{project,month});
        if(disposed||request!==monthRequest||data.project.key!==project)return;
        if(result?.project!==project||result?.month!==month||!Array.isArray(result.entries))throw Error('Monthly notes could not be loaded for this project.');
        monthRows=[...new Map(result.entries.map(e=>[e.id,e])).values()];monthLimited=result.limited;monthError=(result.warnings||[]).join(' ');
      }catch(e){if(!disposed&&request===monthRequest)monthError=String(e);}
      finally{if(!disposed&&request===monthRequest){monthLoading=false;o.repaint();notesStatus();}}
    }
    function notesStatus(){
      const el=$('[data-history-date]');
      el.textContent=monthLoading?'Loading saved notes…':monthError?monthError:monthRows&&monthLimited?'Showing the latest available notes; reading limit reached. Older days remain in Journal history.':'';
      el.hidden=!el.textContent;el.setAttribute('role','status');
    }
    function noteEntries(fallback){return monthLoading||monthError&&!monthRows?[]:monthRows||fallback;}
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
        const raw=noteEntries(data.entries||[]).find(e=>e.id===entry.dataset.id);
        const meta=entry.querySelector('.pj-entry-meta');meta.innerHTML=`<span>${esc(stamp(entry.dataset.at)?new Date(stamp(entry.dataset.at)).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'}):'Time unknown')} · ${esc(entry.dataset.actor)}</span>${raw?.ticket?button(raw.ticket+' ↗','data-note-ticket="'+esc(raw.ticket)+'"'):''}`;
        meta.querySelector('[data-note-ticket]')?.addEventListener('click',e=>openTicket(e.currentTarget.dataset.noteTicket));
        entry.querySelector('.pj-author').hidden=true;
        if(raw?.title&&!entry.querySelector('.pj-body h3')&&!entry.querySelector('.jd-note-title')){const title=document.createElement('h3');title.className='jd-note-title';title.textContent=raw.title;entry.querySelector('.pj-body').before(title);}

        if(!entry.querySelector('[data-copy-note]')){const b=document.createElement('button');b.dataset.copyNote='';b.textContent='Copy';b.onclick=async()=>{try{await navigator.clipboard.writeText(entry.dataset.content||'');b.textContent='Copied';}catch(_){b.textContent='Copy unavailable';}};entry.querySelector('.pj-links').append(b);}
      }
      for(const [day,g] of groups){const count=[...g.querySelectorAll('.pj-entry')].filter(n=>!n.hidden).length;g.hidden=!count;g.querySelector('summary').textContent=(day===new Date().toISOString().slice(0,10)?'Today · ':'')+(day==='Date unknown'?day:new Date(day+'T12:00:00Z').toLocaleDateString([], {day:'numeric',month:'long',timeZone:'UTC'}))+' · '+count+' notes';if(!g.querySelector('.pj-entry'))g.remove();}
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
      fresh.textContent=error||'Observed '+when(data.observed_at)+' · '+panel.querySelectorAll('[data-agent-run]').length+' assigned · '+panel.querySelectorAll('[data-session]').length+' sessions matched';
    }
    function paintInsights(){
      const ts=scopeTickets(),ids=new Set(ts.map(t=>t.id)),es=m.events.filter(e=>ids.has(e.ticket)),verified=ts.filter(t=>t.state==='verified').length;
      // PM completion transitions only: run completion and repeated observations do
      // not prove a ticket finished. Reopened tickets leave the observed total.
      const changes=es.filter(e=>e.kind==='ticket.updated'&&e.state&&stamp(e.at)).sort((a,b)=>stamp(a.at)-stamp(b.at));
      const states=new Map(),samples=[];
      for(const e of changes){
        states.set(e.ticket,e.state);
        const count=[...states.values()].filter(s=>s==='complete').length;
        if(samples.at(-1)?.at===stamp(e.at))samples[samples.length-1].count=count;
        else samples.push({at:stamp(e.at),count});
      }
      let chart='<div class="jd-no-forecast">No ticket completion history recorded.</div>';
      if(samples.length){
        const first=samples[0].at,last=samples.at(-1).at,span=Math.max(1,last-first),total=Math.max(1,ts.length);
        const x=at=>4+(at-first)/span*352,y=count=>112-count/total*96;
        let path=`M${x(first)} ${y(samples[0].count)}`;
        for(const sample of samples.slice(1))path+=`H${x(sample.at)}V${y(sample.count)}`;
        const ticks=Array.from({length:5},(_,i)=>first+span*i/4);
        const format=at=>new Date(at).toLocaleString([],span<2*86400000?{hour:'2-digit',minute:'2-digit',hourCycle:'h23',timeZone:'UTC'}:{day:'numeric',month:'short',timeZone:'UTC'});
        chart=`<div class="jd-progress-chart" role="img" aria-label="Recorded ticket completion history: ${samples.at(-1).count} marked complete in loaded history; ${ts.length} tickets in the current scope. No forecast recorded.">
          <div class="jd-chart-summary"><span>Recorded completion</span><span>Current scope · ${ts.length} tickets</span></div>
          <svg viewBox="0 0 360 128" preserveAspectRatio="none" aria-hidden="true">
            <path class="jd-chart-grid" d="M4 64H356 M4 112H356"/>
            <path class="jd-chart-scope" d="M4 16H356"/>
            <path class="jd-chart-completion" d="${path}"/>
          </svg><div class="jd-chart-dates">${ticks.map(at=>`<span>${esc(format(at))}</span>`).join('')}</div>
          <small class="jd-chart-label">${esc(new Date(first).toLocaleDateString([],{day:'numeric',month:'short',year:'numeric',timeZone:'UTC'}))} · UTC · Loaded completion records; earlier history may be missing.</small>
        </div>`;
      }
      const calendar=$('[data-calendar-host]');
      $('[data-insights-summary]').innerHTML=`<section class="jd-forecast"><h3>Release forecast</h3><p class="pj-muted">No evidence-backed completion forecast is recorded. ${ts.filter(t=>['blocked','stalled'].includes(t.state)).length} blocked or stalled tickets in this scope.</p>${chart}</section><div class="jd-metrics"><div><span>Verified tickets</span><strong>${verified} of ${ts.length}</strong></div><div><span>Recorded runs</span><strong>${new Set(es.map(e=>e.run_id).filter(Boolean)).size}</strong></div><div><span>Recorded events</span><strong>${es.length}</strong></div><div><span>Spend / token allowance</span><span>Not recorded for this scope</span></div></div>`;
      $('[data-insights-summary] .jd-metrics').before(calendar);
    }

    function syncView(){actionDetails.hidden=o.lifecycle?.view()!=='event log';syncStructure();}
    function update(next,sessions,error){
      if(disposed)return;
      if(data&&data.project.key!==next.project.key){generation++;monthRequest++;monthRows=null;monthLoading=false;records.clear();errors.clear();inlineId='';renderInline();selected='';mode='list';viewKey='';}
      data=next;m=window.xnautLifecycle.model(data);
      const scope=o.lifecycle?.scope()||'project';
      if(scope!==lastScope){if(lastScope&&!lastScope.startsWith('ticket:'))returnScope=lastScope;inlineId='';renderInline();lastScope=scope;viewKey='';if(scope.startsWith('ticket:')){selected=scope.slice(7);mode='ticket';void load('ticket',selected);}else{selected='';mode='list';}}
      if(monthRows&&data.path?.includes(notesMonth)){
        monthRows=[...new Map(monthRows.filter(e=>e.document_path!==data.path).concat((data.entries||[]).map(e=>({...e,document_path:data.path}))).map(e=>[e.id,e])).values()];
      }
      summaryMarkup();renderTickets();paintHandoffs();paintAgents(sessions,error);paintInsights();syncView();decorateNotes();notesStatus();
    }
    activate('tickets');
    return {update,openTicket,inspectRun,decorateNotes,syncView,noteEntries,reloadNotes(){if(tab==='notes')void loadMonth();},dispose(){disposed=true;generation++;records.clear();pending.clear();}};
  }
  window.xnautJournalDesign={mount,toolEvents};
})();
