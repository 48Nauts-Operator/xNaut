// XNAUT-494–496. Views of existing native evidence; no lifecycle writes or inferred links.
(function () {
  'use strict';
  const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const millis=s=>{const n=typeof s==='number'?s:Date.parse(s);return Number.isFinite(n)&&n>0?n:null;};
  const when=s=>millis(s)?new Date(millis(s)).toLocaleString([], {dateStyle:'short',timeStyle:'short'}):'Time unknown';
  const axisTime=s=>new Date(s).toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});
  const time=s=>millis(s)?new Date(millis(s)).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'}):'—';
  const tone=s=>s==='verified'?'success':['failed','blocked','stalled','refused','interrupted'].includes(s)?'failure':'neutral';
  const label=s=>({active:'Active · recorded',verified:'Verified',unknown:'Needs inspection',review:'Review',done:'Done · reported',complete:'Complete · reported'}[s]||String(s||'Unknown').replace(/_/g,' '));
  function model(data) {
    const key=data.project.key, snapshot=data.continuity?.project===key?data.continuity:null;
    const native=data.lifecycle?.project===key?data.lifecycle:null;
    const meta=new Map((native?.tickets||[]).map(t=>[t.id,t]));
    const tickets=(snapshot?.tickets||[]).map(t=>({...meta.get(t.id),...t}));
    for(const t of meta.values())if(!tickets.some(r=>r.id===t.id))tickets.push({...t,state:'unknown'});
    const ids=new Set(tickets.map(t=>t.id));
    const extra=(data.entries||[]).filter(e=>ids.has(e.ticket)).map(e=>({...e,id:'journal:'+e.id,detail:e.title,source:'Journal',authority:'Saved report'}));
    const events=[...(native?.events||[]),...extra].filter(e=>ids.has(e.ticket)&&(!e.project||e.project===key));
    const unique=[...new Map(events.map(e=>[e.id,e])).values()].sort((a,b)=>(millis(b.at)||0)-(millis(a.at)||0));
    const latest=new Map();for(const e of unique)latest.set(e.ticket,Math.max(latest.get(e.ticket)||0,millis(e.at)||0));
    tickets.sort((a,b)=>Math.max(millis(b.updated_at)||0,latest.get(b.id)||0)-Math.max(millis(a.updated_at)||0,latest.get(a.id)||0)||a.id.localeCompare(b.id,undefined,{numeric:true}));
    const groups=(data.groups||[]).filter(g=>g.project===key&&g.approved_at!=null);
    const links=[...(native?.links||[]),...groups.flatMap(g=>(g.members||[]).filter(m=>ids.has(m.ticket)).map(m=>({from:g.id,to:m.ticket,kind:'group member',ticket:m.ticket})))];
    return {key,tickets,events:unique,groups,links,native,error:data.lifecycle_error};
  }
  function scoped(m,scope) {
    const [kind,...parts]=scope.split(':');const value=parts.join(':');
    if(kind==='ticket')return m.tickets.filter(t=>t.id===value);
    if(kind==='release')return m.tickets.filter(t=>t.release===value);
    if(kind==='group'){const ids=new Set(m.groups.find(g=>g.id===value)?.members?.map(t=>t.ticket)||[]);return m.tickets.filter(t=>ids.has(t.id));}
    return m.tickets;
  }
  function intervals(events) {
    // Only two recorded PM status changes bound a status interval. Run snapshots
    // and unknown boundaries are points, never invented execution durations.
    const stages=events.filter(e=>e.kind==='ticket.updated'&&e.state&&millis(e.at)).sort((a,b)=>millis(a.at)-millis(b.at));
    const changes=stages.filter((e,i)=>!i||e.state!==stages[i-1].state);
    return changes.map((e,i)=>({event:e,start:millis(e.at),end:changes[i+1]?millis(changes[i+1].at):null}));
  }
  function mount(host,options) {
    let data=null,m=null,scope='project',view='waterfall',selected='',eventId='',limit=40,pinned=false,expanded=null,lastContext='',lastRender='';
    let remembered='project';const root=options.root;
    const storageKey='xnaut-journal-scope:'+root;
    try{const saved=JSON.parse(localStorage.getItem(storageKey)||'{}');if(saved.pinned){scope=saved.scope||'project';pinned=true;}view=saved.view||view;}catch(_){}
    const toolbar=document.createElement('div');toolbar.className='jl-scope-bar';options.toolbar.append(toolbar);
    const calendar=document.createElement('section');calendar.className='jl-calendar';calendar.setAttribute('aria-label','Activity calendar');calendar.innerHTML='<div class="jl-calendar-heading"><strong>Activity</strong><span data-calendar-range></span></div><div data-calendar></div>';options.calendar.append(calendar);
    function persist(){try{localStorage.setItem(storageKey,JSON.stringify({scope,pinned,view}));}catch(_){}window.xnautJournalPin=pinned?{root,scope}:null;}
    function selectScope(next,notify=true){scope=next;selected=next.startsWith('ticket:')?next.slice(7):'';eventId='';limit=40;persist();render();if(notify)options.onScope?.(scope);}
    function ticketClick(id){if(!scope.startsWith('ticket:'))remembered=scope;selectScope('ticket:'+id);}
    function context(){
      const c=window.xnautJournalContext;if(!m||!c||![data.project.key,data.project.root].includes(c.project))return;
      // Reuse the selected agent's existing accent, never invent a Journal palette.
      const surface=host.closest('.pj');
      if(c.accent&&CSS.supports('color',c.accent)){surface?.style.setProperty('--pj-accent',c.accent);expanded?.style.setProperty('--pj-accent',c.accent);}
      else {surface?.style.removeProperty('--pj-accent');expanded?.style.removeProperty('--pj-accent');}
      if(pinned)return;
      const key=JSON.stringify(c);if(key===lastContext)return;lastContext=key;
      const text=String(c.text||'');
      const tickets=m.tickets.filter(t=>new RegExp('(^|[^A-Za-z0-9-])'+t.id+'(?![A-Za-z0-9-])','i').test(text));
      const releases=[...new Set(m.tickets.map(t=>t.release).filter(Boolean))].filter(r=>text.toLowerCase().includes('release '+r.toLowerCase())||text.toLowerCase().includes('version '+r.toLowerCase()));
      const groups=m.groups.filter(g=>text.includes(g.id));
      const refs=tickets.length+releases.length+groups.length;
      if(refs===1){const next=tickets.length?'ticket:'+tickets[0].id:releases.length?'release:'+releases[0]:'group:'+groups[0].id;queueMicrotask(()=>{if(host.isConnected)selectScope(next);});}
      else if(c.thread!==window.__xnautJournalLastThread){scope='project';selected='';options.onScope?.(scope);}
      window.__xnautJournalLastThread=c.thread;
    }
    function renderCalendar() {
      // Scope changes apply to the heatmap too; the selected day does not erase
      // the other days, so users can navigate within this scope's history.
      const ids=new Set(scoped(m,scope).map(t=>t.id)), counts=new Map();
      for(const e of m.events){if(ids.has(e.ticket)&&millis(e.at)){const d=new Date(millis(e.at)).toISOString().slice(0,10);counts.set(d,(counts.get(d)||0)+1);}}
      const today=new Date();today.setUTCHours(0,0,0,0);
      const monday=new Date(today);monday.setUTCDate(today.getUTCDate()-(today.getUTCDay()+6)%7-77);
      const dayName=['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];
      const fmt=d=>d.toLocaleDateString([], {day:'numeric',month:'short',year:'numeric',timeZone:'UTC'});
      const week=d=>{const thursday=new Date(d);thursday.setUTCDate(d.getUTCDate()+3);const first=new Date(Date.UTC(thursday.getUTCFullYear(),0,1));return Math.ceil(((thursday-first)/86400000+1)/7);};
      const selectedDay=options.day?.()||today.toISOString().slice(0,10);
      calendar.querySelector('[data-calendar-range]').textContent=fmt(monday)+' – '+fmt(today)+' · UTC';
      calendar.querySelector('[data-calendar]').innerHTML=`<div class="jl-calendar-grid"><div class="jl-weekdays"><span>Week</span>${dayName.map(d=>`<span>${d}</span>`).join('')}</div>${Array.from({length:12},(_,col)=>{const start=new Date(monday.getTime()+col*7*86400000);return `<div class="jl-week"><span class="jl-week-number" title="Week ${week(start)} · ${fmt(start)}">W${week(start)}</span>${Array.from({length:7},(_,row)=>{const d=new Date(start.getTime()+row*86400000),key=d.toISOString().slice(0,10),count=counts.get(key)||0;return `<button data-day="${key}" ${count?'':'disabled'} aria-pressed="${key===selectedDay}" aria-label="${dayName[row]} ${fmt(d)}: ${count} recorded events" title="${dayName[row]} ${fmt(d)} · ${count} recorded events" style="--activity:${count?Math.min(.85,.2+Math.log2(count+1)*.12):.06}"></button>`;}).join('')}</div>`;}).join('')}</div><small>${esc(selectedDay)} · ${counts.get(selectedDay)||0} recorded events in this scope · Select a day to filter</small>`;
      calendar.querySelectorAll('[data-day]').forEach(b=>{b.onclick=()=>options.onDate?.(b.dataset.day);});
    }

    function renderToolbar() {
      const releases=[...new Set(m.tickets.map(t=>t.release).filter(Boolean))].sort().reverse();
      const html=`<select data-scope aria-label="Journal scope"><option value="project">Project · ${esc(data.project.name)}</option>${releases.map(r=>`<option value="release:${esc(r)}">Release · ${esc(r)}</option>`).join('')}${m.groups.map(g=>`<option value="group:${esc(g.id)}">Task group · ${esc(g.title||when(g.approved_at)+' · '+(g.members||[]).length+' tickets')}</option>`).join('')}${m.tickets.map(t=>`<option value="ticket:${esc(t.id)}">Ticket · ${esc(t.id)}</option>`).join('')}</select><button data-pin aria-pressed="${pinned}">${pinned?'Unpin':'Pin view'}</button><small>${pinned?'Pinned':'Following conversation'}</small>`;
      if(toolbar.dataset.markup!==html){toolbar.innerHTML=html;toolbar.dataset.markup=html;}
      const scopeSelect=toolbar.querySelector('[data-scope]');
      if(![...scopeSelect.options].some(o=>o.value===scope)){const missing=document.createElement('option');missing.value=scope;missing.textContent='Unavailable · '+scope;scopeSelect.append(missing);}
      if(scopeSelect.value!==scope)scopeSelect.value=scope;
      toolbar.querySelector('[data-scope]').onchange=e=>selectScope(e.target.value);
      toolbar.querySelector('[data-pin]').onclick=()=>{pinned=!pinned;persist();lastContext='';renderToolbar();if(!pinned)context();};
    }
    function visibleEvents(tickets){const ids=new Set(tickets.map(t=>t.id));return m.events.filter(e=>ids.has(e.ticket)&&(!options.day?.()||String(e.at||'').startsWith(options.day())));}
    function ticketButton(t){return `<button class="jl-ticket-link" data-ticket="${esc(t.id)}" title="${esc(t.title)}"><strong>${esc(t.id)}</strong><span>${esc(t.title)}</span></button>`;}
    function ticketRows(tickets,events) {
      const shown=tickets.slice(0,limit),times=events.map(e=>millis(e.at)).filter(Boolean);
      const min=Math.min(...times),max=Math.max(...times),span=Math.max(1,max-min);
      const pos=at=>Math.max(0,Math.min(98,(at-min)/span*98));
      const axis=view==='waterfall'&&times.length?`<div class="jl-row jl-axis"><span>Ticket</span><div class="jl-axis-labels">${[0,.5,1].map(f=>`<span>${esc(axisTime(min+span*f,span))}</span>`).join('')}</div><span>Now</span></div>`:'';
      return axis+shown.map(t=>{
        const es=events.filter(e=>e.ticket===t.id),chron=[...es].reverse();let body='';
        if(view==='timeline')body=`<div class="jl-movements">${chron.slice(-6).map(e=>`<button data-event="${esc(e.id)}" class="jl-${tone(e.state)}" title="${esc(when(e.at)+' · '+e.kind)}">${esc(time(e.at))} ${esc(label(e.state||e.kind))}</button>`).join('<span aria-hidden="true">→</span>')||'<small>No recorded movements</small>'}</div>`;
        else {
          const segments=intervals(es).filter(s=>s.end);
          body=`<div class="jl-track" aria-label="Recorded activity for ${esc(t.id)}">${segments.map(s=>`<button data-event="${esc(s.event.id)}" class="jl-bar jl-${tone(s.event.state)} ${['waiting','blocked','review','in_review'].includes(s.event.state)?'jl-wait':''}" style="left:${pos(s.start)}%;width:${Math.max(.5,pos(s.end)-pos(s.start))}%" title="${esc(label(s.event.state)+' · '+when(s.start)+' — '+when(s.end)+' · recorded elapsed time')}"></button>`).join('')}${es.filter(e=>millis(e.at)).slice(0,50).map(e=>`<button class="jl-point jl-${tone(e.state)}" data-event="${esc(e.id)}" style="left:${pos(millis(e.at))}%" title="${esc(when(e.at)+' · '+label(e.state||e.kind))}"><span class="jl-sr">${esc(e.kind)}</span></button>`).join('')}${!es.some(e=>millis(e.at))?'<small>No timed evidence</small>':''}</div>`;
        }
        return `<div class="jl-row" data-lifecycle-ticket="${esc(t.id)}">${ticketButton(t)}${body}<span class="jl-${tone(t.state)}">${esc(label(t.state))}</span></div>`;
      }).join('')+(tickets.length>limit?'<button data-more>Show more tickets</button>':'');
    }
    function eventLog(events){
      const groups=new Map();for(const e of events.slice(0,limit*3)){const day=millis(e.at)?new Date(millis(e.at)).toLocaleDateString():'Date unknown';if(!groups.has(day))groups.set(day,[]);groups.get(day).push(e);}
      return [...groups].map(([day,es])=>`<details class="jl-day" open><summary>${esc(day)} · ${es.length} events</summary>${es.map(e=>`<button class="jl-log-row" data-event="${esc(e.id)}"><time>${esc(time(e.at))}</time><strong>${esc(e.ticket)}</strong><span class="jl-${tone(e.state)}">${esc(label(e.state||e.kind))}</span><small>${esc(e.actor||'Actor unknown')}</small><span aria-hidden="true">›</span></button>`).join('')}</details>`).join('')+(events.length>limit*3?'<button data-more>Show more events</button>':'')||'<p class="jl-muted">No events recorded for this selection.</p>';
    }
    function mapView(tickets){
      const ids=new Set(tickets.map(t=>t.id));const links=m.links.filter(l=>ids.has(l.from)||ids.has(l.to)||ids.has(l.ticket));
      const node=id=>{
        const t=m.tickets.find(t=>t.id===id);if(t)return ticketButton(t);
        const run=m.events.find(e=>e.run_id===id);
        if(run)return `<button class="jl-map-node" data-inspect-event="${esc(id)}" title="${esc(id)}"><strong>@${esc(run.actor||'Unknown agent')} · ${esc(run.ticket)}</strong><small class="jl-${tone(run.state)}">${esc(label(run.state))} · recorded</small></button>`;
        const group=m.groups.find(g=>g.id===id);
        return `<span class="jl-map-node" title="${esc(id)}">${esc(group ? (group.title||'Approved group')+' · '+when(group.approved_at) : id)}</span>`;
      };
      return `<p class="jl-muted">Only recorded relationships. Membership and continuation links are not dependency claims.</p>`+(links.length?links.slice(0,limit).map(l=>`<div class="jl-map-edge">${node(l.from)}<span class="jl-connector">${esc(l.kind)} →</span>${node(l.to)}</div>`).join(''):'<p>No explicit relationships recorded for this selection.</p>')+(links.length>limit?'<button data-more>Show more relationships</button>':'');
    }
    function markup(){
      const tickets=scoped(m,scope),events=visibleEvents(tickets),e=events.find(e=>e.id===eventId);
      const selectedTicket=m.tickets.find(t=>t.id===selected);
      const coverage=!m.native?'Lifecycle history unavailable. Current ticket evidence remains accessible.':m.native.limited?'Recent history only · older events fall outside the loaded window.':m.native.coverage;
      return `<div class="jl-viewbar"><div role="tablist" aria-label="Lifecycle views">${['timeline','waterfall','map','event log'].map(v=>`<button role="tab" aria-selected="${view===v}" tabindex="${view===v?0:-1}" data-view="${v}">${v==='map'?'Activity Map':v[0].toUpperCase()+v.slice(1)}</button>`).join('')}</div><button data-expand>${expanded?'Close expanded view':'Expand ↗'}</button></div>${selected?`<button data-back>‹ Back to ${remembered==='project'?'project':remembered.split(':')[0]}</button>`:''}<p class="jl-coverage">${esc(coverage)}${m.native?.unreadable?' '+m.native.unreadable+' source files unreadable.':''}</p>${e?`<section class="jl-detail jl-detail-${tone(e.state)}"><strong class="jl-${tone(e.state)}">${esc(e.ticket)} · ${esc(label(e.state||e.kind))}</strong><small>${esc(when(e.at))} · ${esc(e.authority||'Recorded report')}</small><p>${esc(e.detail||e.title||'No further details recorded.')}</p><small>Source: ${esc(e.source||'Journal')}</small>${e.run_id?`<button data-inspect-event="${esc(e.run_id)}">Execution record ↗</button>`:''}<button data-ticket="${esc(e.ticket)}">Ticket details ↗</button><button data-close-detail>Close details</button></section>`:''}<div class="jl-content">${view==='map'?mapView(tickets):view==='event log'?eventLog(events):ticketRows(tickets,events)}</div>${view==='waterfall'?'<small class="jl-muted">Points: observations · bars: time between recorded status changes · hatched: waiting/review. Open ends have no invented duration.</small>':''}${selectedTicket?`<section class="jl-ticket-detail"><h3>${esc(selectedTicket.id)} · ${esc(selectedTicket.title)}</h3><p>${esc(selectedTicket.next_action||'Inspect the recorded evidence.')}</p><div data-ticket-evidence></div></section>`:''}`;
    }
    function wire(target){
      target.querySelectorAll('[data-view]').forEach((b,i,bs)=>{b.onclick=()=>{view=b.dataset.view;limit=40;persist();render();(expanded||host).querySelector('[data-view="'+view+'"]')?.focus();};b.onkeydown=e=>{if(!['ArrowLeft','ArrowRight'].includes(e.key))return;e.preventDefault();bs[(i+(e.key==='ArrowRight'?1:bs.length-1))%bs.length].click();};});
      target.querySelectorAll('[data-ticket]').forEach(b=>{b.onclick=()=>ticketClick(b.dataset.ticket);});
      target.querySelectorAll('[data-event]').forEach(b=>{b.onclick=()=>{eventId=b.dataset.event;render();};});
      target.querySelector('[data-more]')?.addEventListener('click',()=>{limit+=40;render();});
      target.querySelector('[data-close-detail]')?.addEventListener('click',()=>{eventId='';render();});
      target.querySelector('[data-back]')?.addEventListener('click',()=>selectScope(remembered));
      target.querySelectorAll('[data-inspect-event]').forEach(b=>{b.onclick=()=>options.onInspect?.(b.dataset.inspectEvent);});
      const evidence=target.querySelector('[data-ticket-evidence]');if(evidence)options.evidence?.(evidence,selected);
      target.querySelector('[data-expand]')?.addEventListener('click',()=>{
        if(expanded){expanded.close();return;}
        expanded=document.createElement('dialog');expanded.className='pj jl-expanded';expanded.style.setProperty('--pj-accent',getComputedStyle(host.closest('.pj')).getPropertyValue('--pj-accent'));document.body.append(expanded);expanded.addEventListener('close',()=>{expanded.remove();expanded=null;lastRender='';render();});render();expanded.showModal();
      });
    }
    function render(){if(!m)return;renderToolbar();renderCalendar();const html=markup();if(html===lastRender)return;lastRender=html;const target=expanded||host;target.innerHTML=html;wire(target);}
    const listener=()=>{context();render();};window.addEventListener('xnaut:journal-context',listener);persist();
    return {update(next){data=next;m=model(data);context();render();},selectScope,ticketIds(){return m?new Set(scoped(m,scope).map(t=>t.id)):null;},scope(){return scope;},dispose(){window.removeEventListener('xnaut:journal-context',listener);expanded?.close();toolbar.remove();calendar.remove();if(window.xnautJournalPin?.root===root)window.xnautJournalPin=null;}};
  }
  window.xnautLifecycle={mount,model,scoped,intervals};
})();
