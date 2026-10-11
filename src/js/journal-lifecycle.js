// XNAUT-494–496. Views of existing native evidence; no lifecycle writes or inferred links.
(function () {
  'use strict';
  const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const millis=s=>{const n=typeof s==='number'?s:Date.parse(s);return Number.isFinite(n)&&n>0?n:null;};
  const when=s=>millis(s)?new Date(millis(s)).toLocaleString([], {dateStyle:'short',timeStyle:'short'}):'Time unknown';
  const axisTime=s=>new Date(s).toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});
  const time=s=>millis(s)?new Date(millis(s)).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit',hourCycle:'h23'}):'—';
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
    let remembered='project',eventSearch='',localTicket='';const root=options.root;
    let calendarYear=new Date().getUTCFullYear(),calendarMonth=-1,calendarDay='',periodFilter=false;
    const storageKey='xnaut-journal-scope:'+root;
    try{const saved=JSON.parse(localStorage.getItem(storageKey)||'{}');if(saved.pinned){scope=saved.scope||'project';pinned=true;}view=saved.view||view;}catch(_){}
    const toolbar=document.createElement('div');toolbar.className='jl-scope-bar';options.toolbar.append(toolbar);
    const calendar=document.createElement('section');calendar.className='jl-calendar';calendar.setAttribute('aria-label','Activity graph');calendar.innerHTML='<div class="jl-calendar-heading"><strong>Activity</strong><span data-calendar-range></span></div><div class="jl-calendar-controls" data-calendar-controls></div><div data-calendar></div>';options.calendar.append(calendar);
    function persist(){try{localStorage.setItem(storageKey,JSON.stringify({scope,pinned,view}));}catch(_){}window.xnautJournalPin=pinned?{root,scope}:null;}
    function selectScope(next,notify=true){scope=next;localTicket='';eventSearch='';selected=next.startsWith('ticket:')?next.slice(7):'';eventId='';limit=40;persist();render();if(notify)options.onScope?.(scope);}
    function ticketClick(id){if(!scope.startsWith('ticket:'))remembered=scope;selectScope('ticket:'+id);options.onTicket?.(id,remembered);}
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
      const requestedDay=options.day?.()||'';
      if(requestedDay!==calendarDay){
        calendarDay=requestedDay;periodFilter=false;const date=requestedDay?new Date(requestedDay+'T00:00:00Z'):today;
        if(Number.isFinite(date.getTime())){calendarYear=date.getUTCFullYear();calendarMonth=date.getUTCMonth();}
      }
      const dayName=['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];
      const fmt=d=>d.toLocaleDateString([], {day:'numeric',month:'short',year:'numeric',timeZone:'UTC'});
      const months=Array.from({length:12},(_,i)=>new Date(Date.UTC(2000,i,1)).toLocaleDateString([], {month:'long',timeZone:'UTC'}));
      const years=new Set(Array.from({length:11},(_,i)=>today.getUTCFullYear()-i));years.add(calendarYear);
      for(const day of counts.keys())years.add(Number(day.slice(0,4)));
      for(const doc of data.documents||[]){const year=/([0-9]{4})-[0-9]{2}-[0-9]{2}/.exec(doc.path||'');if(year)years.add(Number(year[1]));}
      const controls=calendar.querySelector('[data-calendar-controls]');
      const controlsHtml=`<label>Year <select data-calendar-year aria-label="Activity year">${[...years].sort((a,b)=>b-a).map(y=>`<option value="${y}">${y}</option>`).join('')}</select></label><label>Month <select data-calendar-month aria-label="Activity month"><option value="-1">All months</option>${months.map((name,i)=>`<option value="${i}">${esc(name)}</option>`).join('')}</select></label><button data-calendar-today>This month</button>`;
      if(controls.dataset.markup!==controlsHtml){controls.innerHTML=controlsHtml;controls.dataset.markup=controlsHtml;}
      controls.querySelector('[data-calendar-year]').value=calendarYear;
      controls.querySelector('[data-calendar-month]').value=calendarMonth;
      controls.querySelector('[data-calendar-year]').onchange=e=>{calendarYear=Number(e.target.value);periodFilter=true;render();};
      controls.querySelector('[data-calendar-month]').onchange=e=>{calendarMonth=Number(e.target.value);periodFilter=true;render();};
      controls.querySelector('[data-calendar-today]').onclick=()=>{calendarYear=today.getUTCFullYear();calendarMonth=today.getUTCMonth();periodFilter=true;render();};
      // GitHub-style contribution graph: every day of the year remains visible.
      const first=new Date(Date.UTC(calendarYear,0,1)),end=new Date(Date.UTC(calendarYear+1,0,1));
      const days=Math.round((end-first)/86400000),offset=(first.getUTCDay()+6)%7,columns=Math.ceil((offset+days)/7);
      const prefix=String(calendarYear)+(calendarMonth<0?'':'-'+String(calendarMonth+1).padStart(2,'0'));
      const total=[...counts].filter(([day])=>day.startsWith(prefix)).reduce((n,[,count])=>n+count,0);
      const latest=[...counts.keys()].sort().at(-1), selectedDay=requestedDay||today.toISOString().slice(0,10);
      calendar.querySelector('[data-calendar-range]').textContent=calendarYear+' · UTC';
      const monthLabels=months.map((name,i)=>{const start=Math.floor((offset+(Date.UTC(calendarYear,i,1)-first)/86400000)/7)+2;return `<span class="jl-graph-month" style="grid-column:${start};grid-row:1" title="${esc(name)}">${esc(name.slice(0,3))}</span>`;}).join('');
      const peak=Math.max(1,...[...counts].filter(([day])=>day.startsWith(String(calendarYear)+'-')).map(([,count])=>count));
      const cells=Array.from({length:days},(_,i)=>{const d=new Date(first.getTime()+i*86400000),key=d.toISOString().slice(0,10),count=counts.get(key)||0,inMonth=calendarMonth<0||d.getUTCMonth()===calendarMonth;return `<button data-day="${key}" data-in-month="${inMonth}" data-level="${count?Math.ceil(count/peak*3):0}" ${count?'':'disabled'} aria-pressed="${key===selectedDay}" aria-label="${fmt(d)}: ${count} recorded events" title="${fmt(d)} · ${count} recorded events" style="grid-column:${Math.floor((offset+i)/7)+2};grid-row:${(offset+i)%7+2}"></button>`;}).join('');
      const grid=`<div class="jl-year-graph" style="--graph-columns:${columns}">${monthLabels}${dayName.map((d,i)=>`<span class="jl-graph-weekday" style="grid-column:1;grid-row:${i+2}">${i%2===0?d:''}</span>`).join('')}${cells}</div>`;
      const period=calendarMonth<0?'year':'month';
      const message=total?`${total} recorded events in this ${period} · Select a day to filter`:`No recorded activity in this ${period}.`;
      const html=grid+`<div class="jl-graph-caption"><small>${esc(message)}</small><span class="jl-graph-legend" aria-label="Activity intensity: low, medium, high relative to the busiest day in this year" title="Activity relative to the busiest recorded day in the selected year"><small>Less</small><i data-level="0"></i><i data-level="1"></i><i data-level="2"></i><i data-level="3"></i><small>More</small></span></div>${!total&&latest?`<p class="jl-calendar-empty">Latest available activity: ${esc(fmt(new Date(latest+'T00:00:00Z')))} <button data-calendar-latest>View ${esc(months[Number(latest.slice(5,7))-1])} ${esc(latest.slice(0,4))} ↗</button></p>`:''}${m.native?.limited?'<small class="jl-calendar-limit">Loaded history only; older activity may not be available.</small>':''}`;
      const body=calendar.querySelector('[data-calendar]');if(body.dataset.markup!==html){body.innerHTML=html;body.dataset.markup=html;}
      body.querySelectorAll('[data-day]').forEach(b=>{b.onclick=()=>{periodFilter=false;options.onDate?.(b.dataset.day);render();};});
      const jump=body.querySelector('[data-calendar-latest]');if(jump)jump.onclick=()=>{calendarYear=Number(latest.slice(0,4));calendarMonth=Number(latest.slice(5,7))-1;periodFilter=true;render();};
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
    function visibleEvents(tickets){const ids=new Set(tickets.map(t=>t.id));const prefix=periodFilter?String(calendarYear)+(calendarMonth<0?'':'-'+String(calendarMonth+1).padStart(2,'0')):options.day?.();return m.events.filter(e=>ids.has(e.ticket)&&(!prefix||millis(e.at)&&new Date(millis(e.at)).toISOString().startsWith(prefix)));}
    function ticketButton(t){return `<button class="jl-ticket-link" data-ticket="${esc(t.id)}" title="${esc(t.title)}"><strong>${esc(t.id)}</strong><span>${esc(t.title)}</span></button>`;}
    function ticketRows(tickets,events) {
      const shown=tickets.slice(0,limit),times=events.map(e=>millis(e.at)).filter(Boolean);
      const min=Math.min(...times),max=Math.max(...times),span=Math.max(1,max-min);
      const pos=at=>Math.max(0,Math.min(98,(at-min)/span*98));
      const axis=(view==='waterfall'||options.designMode?.())&&times.length?`<div class="jl-row jl-axis"><span>Ticket</span><div class="jl-axis-labels">${[0,.5,1].map(f=>`<span>${esc(axisTime(min+span*f,span))}</span>`).join('')}</div><span>Now</span></div>`:'';
      return axis+shown.map(t=>{
        const es=events.filter(e=>e.ticket===t.id),chron=[...es].reverse();let body='';
        if(view==='timeline'&&!options.designMode?.())body=`<div class="jl-movements">${chron.slice(-6).map(e=>`<button data-event="${esc(e.id)}" class="jl-${tone(e.state)}" title="${esc(when(e.at)+' · '+e.kind)}">${esc(time(e.at))} ${esc(label(e.state||e.kind))}</button>`).join('<span aria-hidden="true">→</span>')||'<small>No recorded movements</small>'}</div>`;
        else {
          const segments=intervals(es).filter(s=>s.end);
          body=`<div class="jl-track" aria-label="Recorded activity for ${esc(t.id)}">${segments.map(s=>`<button data-event="${esc(s.event.id)}" class="jl-bar jl-${tone(s.event.state)} ${['waiting','blocked','review','in_review'].includes(s.event.state)?'jl-wait':''}" style="left:${pos(s.start)}%;width:${Math.max(.5,pos(s.end)-pos(s.start))}%" title="${esc(label(s.event.state)+' · '+when(s.start)+' — '+when(s.end)+' · recorded elapsed time')}"></button>`).join('')}${es.filter(e=>millis(e.at)).slice(0,50).map(e=>`<button class="jl-point jl-${tone(e.state)}" data-event="${esc(e.id)}" style="left:${pos(millis(e.at))}%" title="${esc(when(e.at)+' · '+label(e.state||e.kind))}"><span class="jl-sr">${esc(e.kind)}</span></button>`).join('')}${!es.some(e=>millis(e.at))?'<small>No timed evidence</small>':''}</div>`;
        }
        return `<div class="jl-row" data-lifecycle-ticket="${esc(t.id)}">${ticketButton(t)}${body}<span class="jl-${tone(t.state)}">${esc(label(t.state))}</span></div>`;
      }).join('')+(tickets.length>limit?'<button data-more>Show more tickets</button>':'');
    }
    function eventLog(events){
      const groups=new Map();for(const e of events.slice(0,limit*3)){const day=millis(e.at)?new Date(millis(e.at)).toLocaleDateString([], {day:'numeric',month:'long'}):'Date unknown';if(!groups.has(day))groups.set(day,[]);groups.get(day).push(e);}
      const compact=options.designMode?.(),count=compact&&limit===40?4:limit*3;
      return [...groups].map(([day,es],i)=>`<details class="jl-day" ${!compact||i===0?'open':''}><summary>${esc(day)} · ${es.length>count?'showing '+count+' of ':''}${es.length} events</summary>${es.slice(0,count).map(e=>compact?`<button class="jl-log-row jd-event-row" data-event="${esc(e.id)}"><time>${esc(time(e.at))}</time><div><strong class="jl-${tone(e.state)}">${esc(label(e.state||e.kind))} · ${esc(e.ticket)}</strong><small>${esc(e.detail||e.actor||'Recorded event')}</small></div><span aria-hidden="true">›</span></button>`:`<button class="jl-log-row" data-event="${esc(e.id)}"><time>${esc(time(e.at))}</time><strong>${esc(e.ticket)}</strong><span class="jl-${tone(e.state)}">${esc(label(e.state||e.kind))}</span><small>${esc(e.actor||'Actor unknown')}</small><span aria-hidden="true">›</span></button>`).join('')}${es.length>count?'<button data-more>Show more events ›</button>':''}</details>`).join('')+(events.length>limit*3?'<button data-more>Show more events</button>':'')||'<p class="jl-muted">No events recorded for this selection.</p>';
    }
    function mapView(tickets){
      if(options.designMode?.()){
        const as=data.continuity?.project===m.key?data.continuity.assignments||[]:[];
        const linked=tickets.map(t=>{const runs=as.filter(a=>a.ticket===t.id),run=runs.filter(a=>!a.next_run_id).at(-1)||runs.at(-1);return {t,run};}).filter(x=>x.run);
        const mapLimit=limit===40?3:limit;return linked.slice(0,mapLimit).map(({t,run})=>`<div class="jl-map-chain">${ticketButton(t)}<span>→</span><button class="jl-map-node" data-inspect-event="${esc(run.run_id)}"><strong>${esc(run.owner||'Agent not recorded')}</strong><small>Assigned run</small></button><span>→</span><div class="jl-map-node"><strong class="jl-${tone(run.run_state||run.state)}">${esc(label(run.run_state||run.state))}</strong><small>Recorded state</small></div></div>`).join('')+(!linked.length?'<p class="jl-muted">No recorded assignments in this scope.</p>':'')+`<p class="jl-map-caption">Recorded ticket → run assignments. Unlinked events remain visible in Event log.</p>`+(linked.length>mapLimit?'<button data-more>Show all '+linked.length+' relationships ›</button>':'');
      }
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
      const scopedTickets=scoped(m,scope),tickets=scopedTickets.filter(t=>!localTicket||t.id===localTicket),events=visibleEvents(tickets).filter(e=>!eventSearch||[e.ticket,e.kind,e.state,e.detail,e.actor].join(' ').toLowerCase().includes(eventSearch.toLowerCase())),e=events.find(e=>e.id===eventId);
      const selectedTicket=m.tickets.find(t=>t.id===selected);
      const coverage=!m.native?'Lifecycle history unavailable. Current ticket evidence remains accessible.':m.native.limited?'Recent history only · older events fall outside the loaded window.':m.native.coverage;
      return `<div class="jl-viewbar"><div role="tablist" aria-label="Lifecycle views">${['timeline','waterfall','map','event log'].map(v=>`<button role="tab" aria-selected="${view===v}" tabindex="${view===v?0:-1}" data-view="${v}">${v==='map'?'Activity Map':v[0].toUpperCase()+v.slice(1)}</button>`).join('')}</div><button data-expand>${expanded?'Close expanded view':'Expand ↗'}</button></div>${options.designMode?.()&&view!=='timeline'?`<div class="jl-screen-heading">${view==='waterfall'?'':`<h2>${view==='map'?'Activity Map':'Event log'}</h2>`}<p>${view==='map'?'Recorded relationships · select a node to inspect':view==='event log'?'Actions, recovery and decisions · newest first':'All tickets · work, waiting and recorded outcomes'}</p></div><div class="jd-lifecycle-filters"><select aria-label="Lifecycle ticket"><option value="">All tickets</option>${scopedTickets.map(t=>`<option value="${esc(t.id)}" ${localTicket===t.id?'selected':''}>${esc(t.id)}</option>`).join('')}</select>${view==='event log'?`<input aria-label="Filter events" placeholder="Filter events…" value="${esc(eventSearch)}">`:''}</div>`:''}${selected?`<button data-back>‹ Back to ${remembered==='project'?'project':remembered.split(':')[0]}</button>`:''}${periodFilter?`<p class="jl-period">Recorded activity · ${calendarMonth<0?calendarYear:new Date(Date.UTC(calendarYear,calendarMonth,1)).toLocaleDateString([], {month:'long',year:'numeric',timeZone:'UTC'})}${view==='map'?' · Map shows recorded relationships across dates':''}</p>`:''}<p class="jl-coverage">${esc(coverage)}${m.native?.unreadable?' '+m.native.unreadable+' source files unreadable.':''}</p>${e?`<section class="jl-detail jl-detail-${tone(e.state)}"><strong class="jl-${tone(e.state)}">${esc(e.ticket)} · ${esc(label(e.state||e.kind))}</strong><small>${esc(when(e.at))} · ${esc(e.authority||'Recorded report')}</small><p>${esc(e.detail||e.title||'No further details recorded.')}</p><small>Source: ${esc(e.source||'Journal')}</small>${e.run_id?`<button data-inspect-event="${esc(e.run_id)}">Execution record ↗</button>`:''}<button data-ticket="${esc(e.ticket)}">Ticket details ↗</button><button data-close-detail>Close details</button></section>`:''}<div class="jl-content">${view==='map'?mapView(tickets):view==='event log'?eventLog(events):ticketRows(tickets,events)}</div>${['waterfall','timeline'].includes(view)&&options.designMode?.()?'<div class="jd-timeline-legend"><span><i></i> Recorded work</span><span><i class="jd-wait-key"></i> Waiting / review</span><span class="jl-failure">● Failed / blocked</span><span class="jl-success">● Verified</span></div>':''}${view==='waterfall'?'<small class="jl-muted">Points: observations · bars: time between recorded status changes · hatched: waiting/review. Open ends have no invented duration.</small>':''}${selectedTicket&&!options.designMode?.()?`<section class="jl-ticket-detail"><h3>${esc(selectedTicket.id)} · ${esc(selectedTicket.title)}</h3><p>${esc(selectedTicket.next_action||'Inspect the recorded evidence.')}</p><div data-ticket-evidence></div></section>`:''}`;
    }
    function wire(target){
      target.querySelectorAll('[data-view]').forEach((b,i,bs)=>{b.onclick=()=>{view=b.dataset.view;limit=40;persist();render();options.onView?.(view);(expanded||host).querySelector('[data-view="'+view+'"]')?.focus();};b.onkeydown=e=>{if(!['ArrowLeft','ArrowRight'].includes(e.key))return;e.preventDefault();bs[(i+(e.key==='ArrowRight'?1:bs.length-1))%bs.length].click();};});
      target.querySelectorAll('[data-ticket]').forEach(b=>{b.onclick=()=>ticketClick(b.dataset.ticket);});
      target.querySelectorAll('[data-event]').forEach(b=>{b.onclick=()=>{eventId=b.dataset.event;render();};});
      target.querySelectorAll('[data-more]').forEach(b=>b.onclick=()=>{limit+=40;render();});
      target.querySelector('[aria-label="Lifecycle ticket"]')?.addEventListener('change',e=>{localTicket=e.target.value;eventId='';render();});
      target.querySelector('[aria-label="Filter events"]')?.addEventListener('input',e=>{eventSearch=e.target.value;const pos=e.target.selectionStart;render();const input=(expanded||host).querySelector('[aria-label="Filter events"]');input?.focus();input?.setSelectionRange(pos,pos);});
      target.querySelector('[data-close-detail]')?.addEventListener('click',()=>{eventId='';render();});
      target.querySelector('[data-back]')?.addEventListener('click',()=>selectScope(remembered));
      target.querySelectorAll('[data-inspect-event]').forEach(b=>{b.onclick=()=>options.onInspect?.(b.dataset.inspectEvent);});
      const evidence=target.querySelector('[data-ticket-evidence]');if(evidence)options.evidence?.(evidence,selected);
      target.querySelector('[data-expand]')?.addEventListener('click',()=>{
        if(expanded){expanded.close();return;}
        expanded=document.createElement('dialog');expanded.className=options.designMode?.()?'pj jd jl-expanded':'pj jl-expanded';expanded.style.setProperty('--pj-accent',getComputedStyle(host.closest('.pj')).getPropertyValue('--pj-accent'));document.body.append(expanded);expanded.addEventListener('close',()=>{expanded.remove();expanded=null;lastRender='';render();});render();expanded.showModal();
      });
    }
    function render(){if(!m)return;renderToolbar();renderCalendar();const html=markup();if(html===lastRender)return;lastRender=html;const target=expanded||host;target.dataset.lifecycleView=view;target.innerHTML=html;wire(target);}
    const listener=()=>{context();render();};window.addEventListener('xnaut:journal-context',listener);persist();
    return {update(next){data=next;m=model(data);context();render();},selectScope,setView(next){if(['timeline','waterfall','map','event log'].includes(next)){view=next;lastRender='';render();options.onView?.(view);}},view(){return view;},ticketIds(){return m?new Set(scoped(m,scope).map(t=>t.id)):null;},scope(){return scope;},dispose(){window.removeEventListener('xnaut:journal-context',listener);expanded?.close();toolbar.remove();calendar.remove();if(window.xnautJournalPin?.root===root)window.xnautJournalPin=null;}};
  }
  window.xnautLifecycle={mount,model,scoped,intervals};
})();
