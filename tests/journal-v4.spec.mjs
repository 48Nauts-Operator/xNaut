import {test,expect} from '@playwright/test';
import {startJournalConsole} from './helpers/journal-console-fixture.mjs';
async function start(page){
 await startJournalConsole(page);
 await page.addScriptTag({url:'/js/journal-lifecycle.js'});
 await page.evaluate(()=>{
  window.instance.dispose();
  const stamp='2026-10-11T';
  window.data.continuity.tickets[0].state='verified';
  window.data.continuity.tickets[1].status='done';window.data.continuity.tickets[1].state='review';
  window.data.lifecycle={project:'DEMO',tickets:window.data.continuity.tickets.map((t,i)=>({id:t.id,release:i<3?'2.0':'3.0',title:t.title})),events:[
   {id:'a',project:'DEMO',ticket:'DEMO-1',at:stamp+'08:00:00Z',kind:'ticket.updated',state:'in_progress',source:'events/a.json'},
   {id:'b',project:'DEMO',ticket:'DEMO-1',at:stamp+'09:00:00Z',kind:'ticket.updated',state:'review',source:'events/b.json'},
   {id:'c',project:'DEMO',ticket:'DEMO-2',at:stamp+'08:00:00Z',kind:'run.observed',state:'done',run_id:'run-1',detail:'Agent reported completion',source:'run-1'},
   {id:'d',project:'DEMO',ticket:'DEMO-3',at:stamp+'08:02:00Z',kind:'run.observed',state:'failed',run_id:'run-2',detail:'Authentication failed',source:'run-2'},
   {id:'foreign',project:'OTHER',ticket:'OTHER-1',at:stamp+'08:00:00Z',kind:'ticket.updated',state:'verified',detail:'Foreign data'},
  ],links:[{from:'DEMO-1',to:'DEMO-2',kind:'parent'}],limited:true,coverage:'Recorded events only'};
  window.instance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',path=>{window.openedWiki=path;});
 });
 await expect(page.getByLabel('Journal scope')).toBeVisible();
 await page.getByRole('tab',{name:'Tickets',exact:true}).click();
}
test('release scope uses canonical membership and reported done stays neutral',async({page})=>{
 await start(page);
 await page.getByLabel('Journal scope').selectOption('release:2.0');
 await expect(page.locator('[data-lifecycle-ticket]')).toHaveCount(3);
 await expect(page.locator('[data-lifecycle-ticket="DEMO-1"] .jl-success')).toHaveText('Verified');
 await expect(page.locator('[data-lifecycle-ticket="DEMO-2"] .jl-success')).toHaveCount(0);
 await expect(page.locator('.jl-coverage')).toContainText('Recent history only');
 await expect(page.locator('[data-agent-run]')).toHaveCount(3);
 await page.getByRole('tab',{name:'Event log',exact:true}).click();
 await expect(page.locator('.jl-content')).not.toContainText('Foreign data');
 await expect(page.locator('.jl-log-row')).toHaveCount(4);
});
test('waterfall uses bounded recorded intervals, drill-down/back and native source inspection',async({page})=>{
 await start(page);
 await expect(page.locator('[data-lifecycle-ticket="DEMO-1"] .jl-bar')).toHaveCount(1);
 await expect(page.locator('[data-lifecycle-ticket="DEMO-2"] .jl-bar')).toHaveCount(0);
 await page.locator('[data-event="d"]').click();
 await expect(page.locator('.jl-detail')).toContainText('Authentication failed');
 await page.getByRole('button',{name:'Execution record ↗',exact:true}).click();
 await expect(page.locator('.pj-dialog')).toContainText('Exact run run-2');
 await page.locator('.pj-dialog [data-close]').click();
 await page.getByRole('button',{name:'Ticket details ↗',exact:true}).click();
 await expect(page.getByLabel('Journal scope')).toHaveValue('ticket:DEMO-3');
 await expect(page.locator('[data-ticket-evidence]')).toContainText('run-2');
 await page.locator('[data-back]').click();
 await expect(page.locator('[data-lifecycle-ticket]')).toHaveCount(5);
});
test('pin holds release while conversation changes, survives remount, unpin follows explicit ticket',async({page})=>{
 await start(page);await page.getByLabel('Journal scope').selectOption('release:2.0');await page.getByRole('button',{name:'Pin view',exact:true}).click();
 await page.evaluate(()=>{window.xnautJournalContext={project:'DEMO',thread:'one',text:'Inspect DEMO-3'};window.dispatchEvent(new CustomEvent('xnaut:journal-context'));});
 await expect(page.getByLabel('Journal scope')).toHaveValue('release:2.0');
 await page.evaluate(()=>{window.instance.dispose();window.instance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO');});
 await expect(page.getByLabel('Journal scope')).toHaveValue('release:2.0');
 await page.getByRole('button',{name:'Unpin',exact:true}).click();
 await expect(page.getByLabel('Journal scope')).toHaveValue('ticket:DEMO-3');
});
test('all four views, expansion, map evidence and refresh preserve chosen scope',async({page})=>{
 await start(page);await page.getByLabel('Journal scope').selectOption('release:2.0');
 await page.getByRole('tab',{name:'Activity Map',exact:true}).click();await expect(page.locator('.jl-map-edge')).toHaveCount(1);await expect(page.locator('.jl-content')).toContainText('parent');
 await page.getByRole('button',{name:'Expand ↗',exact:true}).click();await expect(page.locator('.jl-expanded')).toBeVisible();
 await page.locator('.jl-expanded [data-view="timeline"]').click();await expect(page.locator('.jl-expanded .jl-movements')).toHaveCount(3);
 await page.evaluate(()=>window.instance.refresh());await expect(page.locator('.jl-expanded [data-view="timeline"]')).toHaveAttribute('aria-selected','true');
 await page.keyboard.press('Escape');await expect(page.locator('.jl-expanded')).toHaveCount(0);await expect(page.getByLabel('Journal scope')).toHaveValue('release:2.0');
});
test('compact waterfall fits a 600px right pane and selected detail remains actionable',async({page})=>{
 await page.setViewportSize({width:1440,height:1000});await start(page);
 await page.evaluate(()=>{const h=document.querySelector('#journal');h.style.width='600px';h.style.height='950px';});
 await page.locator('[data-event="d"]').click();
 expect(await page.locator('.jl-row').first().evaluate(el=>el.scrollWidth<=el.clientWidth+1)).toBe(true);
 await expect(page.locator('.jl-detail')).toBeInViewport();
 await page.screenshot({path:test.info().outputPath('journal-600.png')});
});
test('unknown timestamps do not produce bars and historical failure cannot repaint current success',async({page})=>{
 await start(page);await page.evaluate(()=>{window.data.lifecycle.events.push({id:'old-failure',ticket:'DEMO-1',project:'DEMO',state:'failed',at:'unknown',kind:'run.observed'});return window.instance.refresh();});
 await expect(page.locator('[data-lifecycle-ticket="DEMO-1"]>span.jl-success')).toHaveText('Verified');
 expect(await page.evaluate(()=>window.xnautLifecycle.intervals([{state:'running',kind:'ticket.updated',at:'bad'}]))).toEqual([]);
 await page.getByRole('tab',{name:'Event log',exact:true}).click();await expect(page.locator('.jl-content')).toContainText('Date unknown');
});

test('selected event stays visible above a long real-size event log',async({page})=>{
 await page.setViewportSize({width:1440,height:1000});await start(page);
 await page.evaluate(()=>{document.querySelector('#journal').style.height='950px';for(let i=0;i<140;i++)window.data.lifecycle.events.push({id:'extra-'+i,project:'DEMO',ticket:'DEMO-2',at:'2026-10-11T07:00:00Z',kind:'run.observed',state:'done',detail:'Recorded report '+i});return window.instance.refresh();});
 await page.getByRole('tab',{name:'Event log',exact:true}).click();
 await page.locator('.jl-log-row').first().click();
 await expect(page.locator('.jl-detail')).toBeInViewport();
});

test('polling keeps the scope control intact while it is being used',async({page})=>{
 await start(page);await page.getByLabel('Journal scope').focus();
 await page.evaluate(()=>{window.scopeElement=document.querySelector('[data-scope]');return window.instance.refresh();});
 expect(await page.evaluate(()=>window.scopeElement===document.querySelector('[data-scope]'))).toBe(true);
 await expect(page.getByLabel('Journal scope')).toBeFocused();
});


test('visible activity calendar follows scope and keeps labelled weeks and days',async({page})=>{
 await start(page);
 await page.evaluate(()=>{const day=new Date().toISOString().slice(0,10);window.data.lifecycle.events=[{id:'one',project:'DEMO',ticket:'DEMO-1',at:day+'T08:00:00Z'},{id:'two',project:'DEMO',ticket:'DEMO-4',at:day+'T09:00:00Z'}];return window.instance.refresh();});
 const calendar=page.getByRole('region',{name:'Activity calendar'});
 await expect(calendar).toBeVisible();
 await expect(calendar.locator('.jl-week-number')).toHaveCount(12);
 await expect(calendar.locator('.jl-weekdays')).toContainText('MonTueWedThuFriSatSun');
 await expect(calendar.getByRole('button',{name:/2 recorded events/})).toHaveCount(1);
 await page.getByLabel('Journal scope').selectOption('release:2.0');
 await expect(calendar.getByRole('button',{name:/1 recorded events/})).toHaveCount(1);
 await page.getByLabel('Journal scope').selectOption('ticket:DEMO-2');
 await expect(calendar.locator('button:not(:disabled)')).toHaveCount(0);
 await page.getByLabel('Journal scope').selectOption('project');
 await expect(calendar.getByRole('button',{name:/2 recorded events/})).toHaveCount(1);
});

test('one Journal scroll area moves header and content together',async({page})=>{
 await start(page);
 await page.evaluate(()=>{document.querySelector('#journal').style.height='700px';});
 const before=await page.locator('.pj-header').boundingBox();
 await page.locator('.pj-scroll').evaluate(el=>{el.scrollTop=250;});
 const after=await page.locator('.pj-header').boundingBox();
 expect(after.y).toBeLessThan(before.y-200);
 expect(await page.locator('.pj-live-context').evaluate(el=>getComputedStyle(el).overflowY)).toBe('visible');
 expect(await page.locator('.jl-content').evaluate(el=>getComputedStyle(el).overflowY)).toBe('visible');
});


test('relationship map adapts to release and ticket dropdowns',async({page})=>{
 await start(page);await page.getByRole('tab',{name:'Activity Map',exact:true}).click();
 await expect(page.locator('.jl-map-edge')).toHaveCount(1);
 await page.getByLabel('Journal scope').selectOption('release:3.0');
 await expect(page.locator('.jl-map-edge')).toHaveCount(0);
 await expect(page.locator('.jl-content')).toContainText('No explicit relationships');
 await page.getByLabel('Journal scope').selectOption('ticket:DEMO-1');
 await expect(page.locator('.jl-map-edge')).toHaveCount(1);
 await page.getByLabel('Journal scope').selectOption('ticket:DEMO-3');
 await expect(page.locator('.jl-map-edge')).toHaveCount(0);
});
