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
   if(name==='project_wiki_source'){
    window.calls.push({name,args});if(window.sourceFails)throw Error('Source unavailable');
    if(window.sourceDelay)await new Promise(r=>setTimeout(r,window.sourceDelay));
    if(args.kind==='ticket')return {text:JSON.stringify({id:args.id,project:window.wrongProject?'FOREIGN':'DEMO',body:'## Description\nFix Unicode display width.\n- [x] Emoji aligns\n- [ ] CJK aligns',release:'2.0'})};
    if(args.kind==='log')return {text:args.id==='old-run'?'FAIL expected 18 got 22':'FAIL expected 18 got 20'};
    return {text:JSON.stringify({project:'DEMO',run_id:args.id,ticket:'DEMO-1',runtime_id:'claude',model:'routed-model',remote_env:'exe-dev',state:'failed',branch:'fix/one',last_commit:args.id==='old-run'?'aaa':'bbb',last_signal:'CJK cursor test failed',admission_refused:args.id==='old-run'})};
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
 await page.locator('[data-delivery]').click();expect(await page.evaluate(()=>window.openedTicket.ticket)).toBe('DEMO-1');
 await page.locator('[data-back-list]').click();await expect(page.locator('.jd-ticket-row')).toHaveCount(5);
});
test('trace and comparison retain admission failure, exact models, commits and saved outputs',async({page})=>{
 await start(page);await page.locator('.jd-ticket-row[data-open-ticket="DEMO-1"]').click();await page.locator('[data-open-trace]').click();
 await expect(page.locator('.jd-attempt[open]')).toContainText('routed-model');
 await page.locator('[data-load-log]').click();await expect(page.locator('.jd-output')).toContainText('got 20');
 await page.locator('[data-compare]').click();await expect(page.locator('.jd-compare-field')).toContainText(['claude','routed-model','exe-dev','failed','true','fix/one','aaa','CJK']);
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
