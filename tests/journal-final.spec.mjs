import {test,expect} from '@playwright/test';
import {startJournalConsole} from './helpers/journal-console-fixture.mjs';
async function start(page,width=600){
 await startJournalConsole(page);
 await page.addScriptTag({url:'/js/journal-lifecycle.js'});
 await page.addScriptTag({url:'/js/journal-design.js'});
 await page.evaluate(width=>{
  window.instance.dispose();document.querySelector('#journal').style.width=width+'px';
  const now=new Date().toISOString(),old=new Date(Date.now()-60000).toISOString();
  window.data.continuity.tickets.forEach((t,i)=>{t.state=['blocked','review','active','verified','unknown'][i];t.next_action=i===0?'Retry budget used; owner decision required.':'Inspect recorded work';t.owner='claude';});
  window.data.continuity.assignments=[{run_id:'old-run',ticket:'DEMO-1',owner:'claude',state:'blocked',run_state:'failed',next_run_id:'new-run',branch:'fix/one',last_commit:'aaa',evidence:[]},{run_id:'new-run',ticket:'DEMO-1',owner:'claude',state:'blocked',run_state:'failed',previous_run_id:'old-run',branch:'fix/one',last_commit:'bbb',evidence:[{kind:'verification',source:'check.json',detail:'Test failed on current revision.'}]},{run_id:'other-run',ticket:'DEMO-5',owner:'pi',state:'review',pr_url:'https://example.com/5',evidence:[{kind:'handback',source:'other.json',detail:'Other release handoff'}]}];
  window.data.lifecycle={project:'DEMO',tickets:window.data.continuity.tickets.map((t,i)=>({...t,release:i<3?'2.0':'3.0'})),events:[{id:'old',project:'DEMO',ticket:'DEMO-1',run_id:'old-run',at:old,kind:'run.observed',state:'failed',actor:'claude',detail:'Previous failure'},{id:'new',project:'DEMO',ticket:'DEMO-1',run_id:'new-run',at:now,kind:'run.observed',state:'failed',actor:'claude',detail:'Current test failed'},{id:'other',project:'DEMO',ticket:'DEMO-5',run_id:'other-run',at:now,kind:'run.observed',state:'done',detail:'Other release handoff'}],links:[{from:'old-run',to:'new-run',kind:'continuation',ticket:'DEMO-1'}],limited:true,coverage:'Recorded events only'};
  window.data.entries=[{id:'note-one',ticket:'DEMO-1',kind:'note',actor:'You',at:now,title:'Keep the test',content:'Keep **the original assertion**.'},{id:'note-other',ticket:'DEMO-5',kind:'note',actor:'NautBot',at:now,title:'Other release',content:'Other release note'}];
  const original=window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke=async(name,args)=>{
   if(name==='project_journal_notes_month'){window.calls.push({name,args});return {project:args.project,month:args.month,entries:structuredClone(window.data.entries).map(e=>({...e,document_path:window.data.path})),limited:false,warnings:[]};}
   if(name==='project_wiki_source'){
    window.calls.push({name,args});if(window.sourceFails)throw Error('Source unavailable');
    if(window.sourceDelay)await new Promise(r=>setTimeout(r,window.sourceDelay));
    if(args.kind==='ticket')return {text:JSON.stringify({id:args.id,project:window.wrongProject?'FOREIGN':'DEMO',body:'## Description\nFix Unicode display width.\n- [x] Emoji aligns\n- [ ] CJK aligns',release:'2.0'})};
    if(args.kind==='log')return {text:args.id==='old-run'?'FAIL expected 18 got 22':'FAIL expected 18 got 20'};
    return {text:JSON.stringify({project:'DEMO',run_id:args.id,ticket:window.data.continuity.assignments.find(a=>a.run_id===args.id)?.ticket||'DEMO-1',runtime_id:'claude',model:'routed-model',remote_env:'exe-dev',state:window.data.continuity.assignments.find(a=>a.run_id===args.id)?.run_state||'failed',branch:'fix/one',last_commit:args.id==='old-run'?'aaa':'bbb',last_signal:['old-run','new-run'].includes(args.id)?'CJK cursor test failed':'Recorded run outcome; inspect its evidence.',admission_refused:args.id==='old-run'})};
   }
   return original(name,args);
  };
  window.instance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',path=>window.openedWiki=path);
 },width);
 await expect(page.locator('.jd-ticket-row')).toHaveCount(5);
}
test('approved navigation, decision summary and grouped rows fit 420px without changing palette',async({page})=>{
 await start(page,420);await expect(page.locator('.pj-tabs [role=tab]')).toHaveText(['Tickets','Timeline','Agents','Insights','Notes','Handoffs']);
 await expect(page.locator('[data-needs-heading]')).toHaveText('Needs you · 2 decisions');
 expect(await page.locator('.jd').evaluate(el=>getComputedStyle(el).backgroundColor)).toBe('rgb(23, 23, 23)');
 expect(await page.locator('.pj-scroll').evaluate(el=>el.scrollWidth<=el.clientWidth+1)).toBe(true);
 await page.locator('.jd').screenshot({path:test.info().outputPath('R1-glance.png')});
});
test('ticket details load authoritative description and checklist with reported-evidence label',async({page})=>{
 await start(page);await page.locator('.jd-ticket-row[data-open-ticket="DEMO-1"]').click();
 await expect(page.locator('[data-ticket-body]')).toContainText('Unicode display width');
 await expect(page.locator('.jd-tickets')).toContainText('Acceptance · 1 of 2 checked');
 await expect(page.locator('.jd-tickets')).toContainText('not independent verification');
 await expect(page.getByRole('img',{name:'Checked by author',exact:true})).toHaveText('✓');
 await expect(page.getByRole('img',{name:'Not checked',exact:true})).toHaveText('–');
 await page.locator('[data-delivery]').click();expect(await page.evaluate(()=>window.openedTicket.ticket)).toBe('DEMO-1');
 await page.locator('[data-back-list]').click();await expect(page.locator('.jd-ticket-row')).toHaveCount(5);
});
test('trace and comparison retain admission failure, exact models, commits and saved outputs',async({page})=>{
 await start(page);await page.locator('.jd-ticket-row[data-open-ticket="DEMO-1"]').click();await page.locator('[data-open-trace]').click();
 await expect(page.locator('.jd-attempt[open]')).toContainText('routed-model');
 await page.locator('[data-load-log]').click();await expect(page.locator('.jd-output')).toContainText('got 20');
 await page.locator('[data-compare]').click();await expect(page.locator('.jd-compare-field')).toHaveCount(8);for(const value of ['claude','routed-model','exe-dev','failed','true','fix/one','aaa','CJK'])await expect(page.locator('.jd-tickets')).toContainText(value);
 await page.locator('[data-compare-output]').click();await expect(page.locator('.jd-compare-logs')).toContainText('got 22');await expect(page.locator('.jd-compare-logs')).toContainText('got 20');
 await page.locator('[data-back-trace]').click();await page.locator('[data-attempt="old-run"]>summary').click();await expect(page.locator('.jd-attempt[open]')).toContainText('Refused before execution');
});
test('release scope applies to tickets, notes, handoffs and agents',async({page})=>{
 await start(page);await page.getByLabel('Journal scope').selectOption('release:2.0');await expect(page.locator('.jd-ticket-row')).toHaveCount(3);
 await page.getByRole('tab',{name:'Notes',exact:true}).click();await expect(page.locator('[data-entries]')).toContainText('original assertion');await expect(page.locator('[data-entries]')).not.toContainText('Other release note');
 await page.getByRole('tab',{name:'Handoffs',exact:true}).click();await expect(page.locator('[data-handoffs]')).not.toContainText('Other release handoff');
 await page.getByRole('tab',{name:'Agents',exact:true}).click();await expect(page.locator('[data-agent-run]')).toHaveCount(3);
});
test('activity map selection stays inline and never opens a raw modal',async({page})=>{
 await start(page);await page.getByRole('tab',{name:'Timeline',exact:true}).click();await page.getByRole('tab',{name:'Activity Map',exact:true}).click();
 await page.locator('[data-inspect-event="new-run"]').click();await expect(page.locator('.jd-map-inspector')).toContainText('routed-model');await expect(page.locator('dialog')).toHaveCount(0);
 await expect(page.locator('[data-panel="timeline"]')).toBeVisible();await page.locator('[data-inline-trace]').click();await expect(page.locator('.jd-attempt')).toHaveCount(2);
});
test('insights retain full-year graph, selected-month filtering and honest missing metrics',async({page})=>{
 await start(page);await page.getByRole('tab',{name:'Insights',exact:true}).click();await expect(page.locator('.jl-year-graph [data-day]')).toHaveCount(365);
 await expect(page.locator('[data-insights-summary]')).toContainText('No evidence-backed completion forecast');await expect(page.locator('[data-insights-summary]')).toContainText('Not recorded for this scope');
 await page.getByLabel('Activity month').selectOption('0');await expect(page.locator('.jl-year-graph [data-in-month="true"]')).toHaveCount(31);
 await page.locator('.jd').screenshot({path:test.info().outputPath('R5-insights.png')});
});
test('heatmap fills partial weeks while keeping UTC dates on their real weekdays',async({page})=>{
 await start(page);await page.getByRole('tab',{name:'Insights',exact:true}).click();
 await page.getByLabel('Activity year').selectOption('2026');
 const january=page.locator('[data-day="2026-01-01"]');
 expect(await january.evaluate(e=>e.style.gridRow)).toBe('5');
 const cells=await page.locator('.jl-year-graph').evaluate(e=>[...e.querySelectorAll('[data-day],.jl-graph-padding')].map(c=>[c.style.gridColumn,c.style.gridRow]));
 expect(cells).toHaveLength(371);expect(new Set(cells.map(c=>c.join(':'))).size).toBe(371);
 expect(cells.filter(c=>c[0]==='2').map(c=>Number(c[1])).sort()).toEqual([2,3,4,5,6,7,8]);
 await page.getByLabel('Activity year').selectOption('2024');
 await expect(page.locator('.jl-year-graph [data-day]')).toHaveCount(366);
 expect(await page.locator('[data-day="2024-02-29"]').evaluate(e=>e.style.gridRow)).toBe('5');
});
test('progress uses ticket completion transitions, handles reopening and respects release scope',async({page})=>{
 await start(page);
 await page.evaluate(()=>{
  const event=(id,ticket,state,hour,kind='ticket.updated')=>({id,project:'DEMO',ticket,state,kind,at:`2026-10-10T${hour}:00:00Z`});
  window.data.lifecycle.events=[event('a','DEMO-1','in_progress','08'),event('b','DEMO-1','complete','09'),event('c','DEMO-1','complete','10'),event('d','DEMO-1','review','11'),event('e','DEMO-2','done','12'),event('f','DEMO-2','complete','13','run.observed'),event('g','DEMO-5','complete','14')];
  return window.instance.refresh();
 });
 await page.getByRole('tab',{name:'Insights',exact:true}).click();
 await expect(page.locator('.jd-progress-chart')).toHaveAttribute('aria-label',/1 marked complete/);
 await page.getByLabel('Journal scope').selectOption('release:2.0');
 await expect(page.locator('.jd-progress-chart')).toHaveAttribute('aria-label',/0 marked complete.*3 tickets/);
 const path=await page.locator('.jd-chart-completion').getAttribute('d');
 expect(path).toContain('V80');expect(path).toContain('V112');
 await expect(page.locator('.jd-activity-line')).toHaveCount(0);
 await page.locator('#journal').evaluate(e=>e.style.width='900px');
 const chart=await page.locator('.jd-progress-chart svg').boundingBox(),parent=await page.locator('.jd-progress-chart').boundingBox();
 expect(chart.width).toBeCloseTo(parent.width,0);
 await page.locator('.jd').screenshot({path:test.info().outputPath('R5-wide-progress.png')});
});
test('notes group by date, filter author and copy original markdown after refresh',async({page})=>{
 await start(page);await page.evaluate(()=>{navigator.clipboard.writeText=async text=>window.copied=text;});await page.getByRole('tab',{name:'Notes',exact:true}).click();
 await expect(page.locator('.jd-note-day')).toHaveCount(1);await page.getByLabel('Note author').selectOption('You');await expect(page.locator('.pj-entry:visible')).toHaveCount(1);
 await page.locator('.pj-entry:visible [data-copy-note]').click();expect(await page.evaluate(()=>window.copied)).toBe('Keep **the original assertion**.');
 await page.evaluate(()=>window.instance.refresh());await expect(page.locator('.pj-entry:visible')).toHaveCount(1);await expect(page.locator('.jd-note-day')).toHaveCount(1);
});
test('unavailable and mismatched sources are visible and recoverable',async({page})=>{
 await start(page);await page.evaluate(()=>window.sourceFails=true);await page.locator('.jd-ticket-row[data-open-ticket="DEMO-1"]').click();await expect(page.locator('.jd-tickets [role=status]')).toContainText('Source unavailable');
 await page.evaluate(()=>{window.sourceFails=false;window.wrongProject=true;});await page.locator('[data-reload]').click();await expect(page.locator('.jd-tickets [role=status]')).toContainText('does not belong');
 await page.evaluate(()=>window.wrongProject=false);await page.locator('[data-reload]').click();await expect(page.locator('[data-ticket-body]')).toContainText('Unicode');
});
test('refresh preserves selected trace and disposal ignores late sources',async({page})=>{
 await start(page);await page.locator('.jd-ticket-row[data-open-ticket="DEMO-1"]').click();await page.locator('[data-open-trace]').click();await expect(page.locator('.jd-attempt[open]')).toContainText('routed-model');
 await page.evaluate(()=>window.instance.refresh());await expect(page.locator('.jd-attempt[open]')).toContainText('routed-model');
 await page.evaluate(()=>{window.sourceDelay=200;document.querySelector('[data-load-log]').click();window.instance.dispose();document.querySelector('#journal').innerHTML='<p>Unmounted</p>';});await page.waitForTimeout(250);await expect(page.locator('#journal')).toHaveText('Unmounted');
});
test('secondary screens collapse decisions and use a single document scroll',async({page})=>{
 await start(page,420);await page.getByRole('tab',{name:'Agents',exact:true}).click();await expect(page.locator('.jd-needs')).not.toHaveAttribute('open','');
 await expect(page.locator('.jd-agent-evidence')).toHaveCount(5);await page.getByRole('tab',{name:'Timeline',exact:true}).click();
 expect(await page.locator('.jl-content').evaluate(el=>getComputedStyle(el).maxHeight)).toBe('none');
 await page.getByRole('tab',{name:'Event log',exact:true}).click();await expect(page.locator('.jd-dispatch-history')).toBeVisible();
 expect(await page.locator('.pj-scroll').evaluate(el=>el.scrollWidth<=el.clientWidth+1)).toBe(true);
});
test('ticket drill-down from release timeline returns to the original release',async({page})=>{
 await start(page);await page.getByLabel('Journal scope').selectOption('release:2.0');await page.locator('.pj-tabs [data-journal-tab="timeline"]').click();
 await page.locator('.jl-ticket-link[data-ticket="DEMO-1"]').click();await expect(page.locator('[data-ticket-body]')).toContainText('Unicode');
 await page.locator('[data-back-list]').click();await expect(page.getByLabel('Journal scope')).toHaveValue('release:2.0');await expect(page.locator('.jd-ticket-row')).toHaveCount(3);
});
test('structured tool records retain exact calls and results; plain output is not guessed',async({page})=>{
 await start(page);
 const result=await page.evaluate(()=>window.xnautJournalDesign.toolEvents('just a command-like string\n'+JSON.stringify({type:'assistant',message:{content:[{type:'tool_use',name:'bash',input:{command:'git status'}}]}})+'\n'+JSON.stringify({type:'response_item',payload:{type:'function_call_output',call_id:'call1',output:'exit 1'}})));
 expect(result).toHaveLength(2);expect(result[0].name).toBe('bash');expect(result[1].body).toContain('exit 1');
});
test('large projects bound initial ticket DOM and preserve keyboard navigation',async({page})=>{
 await start(page,420);
 await page.evaluate(()=>{for(let i=6;i<606;i++)window.data.continuity.tickets.push({id:'DEMO-'+i,title:'Historical ticket '+i,state:'unknown',status:'ready',assignment_ids:[]});return window.instance.refresh();});
 expect(await page.locator('.jd-ticket-row').count()).toBeLessThanOrEqual(44);
 await page.locator('.pj-tabs [data-journal-tab="tickets"]').focus();await page.keyboard.press('End');await expect(page.locator('.pj-tabs [data-journal-tab="handoffs"]')).toBeFocused();await expect(page.locator('[data-panel="handoffs"]')).toBeVisible();
});


test('unattributed shared diagnostics go to logs once; project failures remain visible',async({page})=>{
 const logged=[];page.on('console',m=>{if(m.text().includes('[Journal system diagnostics; no project attribution]'))logged.push(m.text());});
 await start(page);
 await page.evaluate(()=>{window.data.continuity.diagnostics=[{blocking:false,source:'launch receipts',message:'Shared receipts have no project attribution.'}];return window.instance.refresh();});
 await expect(page.locator('.pj-diagnostics')).toHaveCount(0);
 await expect(page.locator('.pj')).not.toContainText('Shared receipts have no project attribution.');
 expect(logged).toHaveLength(1);
 await page.evaluate(()=>window.instance.refresh());expect(logged).toHaveLength(1);
 await page.locator('.jd-ticket-row').first().click();
 await expect(page.locator('.pj')).not.toContainText('Shared receipts have no project attribution.');
 await page.evaluate(()=>{window.data.continuity.diagnostics.push({blocking:true,source:'project run',message:'Project evidence cannot be read.'},{source:'legacy project source',message:'Legacy failure remains visible.'});return window.instance.refresh();});
 await expect(page.locator('.pj-diagnostics')).toContainText('Project evidence cannot be read.');
 await expect(page.locator('.pj-diagnostics')).toContainText('Legacy failure remains visible.');
 expect(logged).toHaveLength(1);
});

test('Paper visual acceptance captures R1–R12 at the approved 420px width',async({page})=>{
 test.setTimeout(90000);await start(page,420);
 await page.evaluate(()=>{
  const titles=['Unicode width wrong in shell prompt','Stale diff after rebase','Persist theme choice','Merge verification passed','Clean up crashed sandboxes','Cap agent retries per ticket','Retain worker handoffs','Recorded release completion'];
  const states=['blocked','review','active','verified','unknown','active','verified','verified'];
  const rows=titles.map((title,i)=>({id:'DEMO-'+(i+1),title,state:states[i],owner:['cinder','boreal','atlas','verifier-1'][i%4],release:'v1.31.0',next_action:i===0?'Retry budget used; owner decision required.':i===1?'Review the recorded pull request.':'Inspect the recorded evidence.'}));
  window.data.continuity.tickets=rows;window.data.lifecycle.tickets=rows;
  const midnight=new Date();midnight.setHours(0,0,0,0);const start=+midnight;
  window.data.lifecycle.events=rows.flatMap((t,i)=>[{id:t.id+'start',project:'DEMO',ticket:t.id,run_id:i===0?'old-run':'run-'+i,at:new Date(start+(i+1)*600000).toISOString(),kind:'ticket.updated',state:'active',actor:t.owner,detail:'Implementation started'},{id:t.id+'end',project:'DEMO',ticket:t.id,run_id:i===0?'new-run':'run-'+i,at:new Date(start+(i+8)*600000).toISOString(),kind:'ticket.updated',state:t.state==='verified'?'complete':t.state,actor:t.owner,detail:i===0?'Verification failed: expected column 18, observed 20.':'Recorded ticket transition'}]);
  window.data.continuity.assignments.push(...rows.slice(1).map((t,i)=>({ticket:t.id,owner:t.owner,run_id:'run-'+(i+1),state:t.state,run_state:t.state,branch:'fix/'+t.id,evidence:[]})));
  window.data.project.name='xNaut';document.querySelector('.jd').style.setProperty('--pj-accent','#8f8c00');
  return window.instance.refresh();
 });
 await page.getByLabel('Journal scope').selectOption('release:v1.31.0');
 const capture=async(n,h=980)=>{await page.locator('#journal').evaluate((e,h)=>e.style.height=h+'px',h);await page.locator('.pj-scroll').evaluate(e=>e.scrollTop=0);await page.locator('.jd').screenshot({path:test.info().outputPath('R'+n+'.png')});};
 await capture(1);
 await page.locator('.jd-ticket-row[data-open-ticket="DEMO-1"]').click();await expect(page.locator('[data-ticket-body]')).toContainText('Unicode');await expect(page.locator('.pj-header')).toBeHidden();await capture(2,1090);
 await page.locator('[data-open-trace]').click();await expect(page.locator('.jd-attempt[open]')).toContainText('routed-model');await page.locator('[data-load-log]').click();await expect(page.locator('.jd-output')).toContainText('got 20');await capture(3,1020);
 await page.locator('[data-compare]').click();await expect(page.locator('.jd-compare-field')).toHaveCount(8);for(const value of ['claude','routed-model','exe-dev','failed','true','fix/one','aaa','CJK'])await expect(page.locator('.jd-tickets')).toContainText(value);await page.locator('[data-compare-output]').click();await expect(page.locator('.jd-compare-logs')).toContainText('got 22');await capture(7,980);
 await page.locator('[data-back-trace]').click();await page.locator('[data-back-ticket]').click();await page.locator('[data-back-list]').click();
 for(const [tab,n] of [['Agents',4],['Insights',5],['Notes',8],['Handoffs',9]]){await page.getByRole('tab',{name:tab,exact:true}).click();await capture(n,1040);}
 await page.getByRole('tab',{name:'Timeline',exact:true}).click();
 for(const [view,n] of [['Timeline',6],['Activity Map',10],['Event log',11],['Waterfall',12]]){await page.getByRole('tab',{name:view,exact:true}).last().click();if(n===10){await page.locator('[data-inspect-event]').first().click();await expect(page.locator('.jd-map-inspector')).toContainText('routed-model');}else if(n===6){await expect(page.locator('.jl-complete-point')).toHaveCount(3);expect(await page.locator('.jl-complete-point').first().evaluate(e=>getComputedStyle(e).backgroundColor)).toBe('rgb(115, 201, 161)');expect(await page.locator('.jl-complete-point').first().evaluate(e=>getComputedStyle(e).borderRadius)).toBe('50%');}else if(n===11||n===12){await page.locator('.jl-content [data-event]').first().click();}await capture(n,1040);}
 await page.getByLabel('Journal scope').selectOption('project');await expect(page.locator('.jd-hero')).toBeHidden();
});


test('monthly notes read across days, filter scope and reject wrong-project responses',async({page})=>{
 await start(page);
 await page.evaluate(()=>{
  window.data.documents.push({path:'Development/journal/2026-09-02.md'});
  const base=window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke=async(name,args)=>{
   if(name!=='project_journal_notes_month')return base(name,args);
   window.calls.push({name,args});
   if(window.badMonth)return {project:'FOREIGN',month:args.month,entries:[]};
   return {project:'DEMO',month:args.month,entries:args.month==='2026-09'?[{id:'september',actor:'You',kind:'note',ticket:'DEMO-1',at:'2026-09-02T12:00:00Z',content:'September decision',document_path:'Development/journal/2026-09-02.md'}]:[{id:'october',actor:'NautBot',kind:'note',ticket:'DEMO-1',at:'2026-10-01T12:00:00Z',content:'Earlier October decision',document_path:'Development/journal/2026-10-01.md'}]};
  };return window.instance.refresh();
 });
 await page.getByRole('tab',{name:'Notes',exact:true}).click();await expect(page.locator('[data-entries]')).toContainText('Earlier October decision');
 await page.getByLabel('Notes month').selectOption('2026-09');await expect(page.locator('[data-entries]')).toContainText('September decision');await expect(page.locator('[data-entries]')).not.toContainText('Earlier October');
 await page.evaluate(()=>window.badMonth=true);await page.getByLabel('Notes month').selectOption('2026-10');await expect(page.locator('[data-history-date]')).toContainText('could not be loaded');await expect(page.locator('[data-entries]')).not.toContainText('September decision');
});


test('compact event log filters and expands without leaving the release',async({page})=>{
 await start(page);await page.getByLabel('Journal scope').selectOption('release:2.0');await page.getByRole('tab',{name:'Timeline',exact:true}).click();await page.getByRole('tab',{name:'Event log',exact:true}).click();
 await page.getByLabel('Filter events').fill('Current test failed');await expect(page.locator('.jd-event-row')).toHaveCount(1);await page.locator('.jd-event-row').click();await expect(page.locator('.jl-detail')).toContainText('Current test failed');
 await page.getByLabel('Filter events').fill('');await page.getByLabel('Lifecycle ticket').selectOption('DEMO-2');await expect(page.locator('.jd-event-row')).toHaveCount(0);await expect(page.getByLabel('Journal scope')).toHaveValue('release:2.0');
});
test('handoff links open the exact ticket and return from its trace',async({page})=>{
 await start(page);await page.getByRole('tab',{name:'Handoffs',exact:true}).click();await page.getByLabel('Handoff ticket').selectOption('DEMO-1');await expect(page.locator('.jd-handoff')).toHaveCount(1);
 await page.locator('[data-handoff-trace]').click();await expect(page.locator('.jd-attempt[open]')).toContainText('routed-model');await page.locator('[data-back-ticket]').click();await expect(page.locator('[data-panel="handoffs"]')).toBeVisible();await expect(page.getByLabel('Handoff ticket')).toHaveValue('DEMO-1');
});
