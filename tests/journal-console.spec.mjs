import {test,expect} from '@playwright/test';

import {startJournalConsole as start} from './helpers/journal-console-fixture.mjs';

test('five deployed agents stay visible across tabs, dates and ticket filters',async({page})=>{
 await start(page);
 await expect(page.getByRole('tab',{name:'Actions',exact:true})).toHaveAttribute('aria-selected','true');
 await expect(page.locator('[data-agent-run]')).toHaveCount(5);
 await page.getByLabel('Journal workstream').selectOption('DEMO-3');
 await page.getByLabel('Journal date').selectOption('Development/journal/2026-10-09.md');
 await expect(page.locator('[data-actions-date]')).toContainText('2026-10-09');
 for(const name of ['Tickets','Notes','Handoffs','Actions']){
  await page.getByRole('tab',{name,exact:true}).click();
  await expect(page.locator('[data-agent-run]')).toHaveCount(5);
  await expect(page.locator('[data-agent-run="run-0"]')).toBeVisible();
  await expect(page.locator('[data-panel]:visible')).toHaveCount(1);
 }
 await expect(page.locator('[data-continuity-time]')).toContainText('independent');
 await expect(page.locator('[data-summary]')).toContainText('1 blocked');
 await page.getByRole('button',{name:'Approvals ↗'}).click();
 expect(await page.evaluate(()=>window.meshOpened)).toBe(true);
 await expect(page.getByRole('button',{name:'Approve',exact:true})).toHaveCount(0);
});

test('only exact live sessions are attachable and stale workers retain inspection',async({page})=>{
 await start(page);
 await expect(page.locator('[data-session]')).toHaveCount(1);
 await page.locator('[data-session="session-0"]').click();
 expect(await page.evaluate(()=>window.openedSession[0])).toBe('session-0');
 await page.locator('[data-inspect="run-2"]').click();
 await expect(page.locator('dialog')).toContainText('Exact run run-2');
 await page.getByRole('button',{name:'Saved output',exact:true}).click();
 expect(await page.evaluate(()=>window.calls.filter(c=>c.name==='project_wiki_source').at(-1).args)).toEqual({project:'DEMO',kind:'log',id:'run-2'});
 await page.getByRole('button',{name:'Close',exact:true}).click();
 await page.evaluate(()=>{window.failSessions=true;return window.instance.refresh();});
 await expect(page.locator('[data-session]')).toHaveCount(0);
 await expect(page.locator('[data-agents]')).toContainText('Session status is unavailable');
});

test('foreign console payloads cannot appear after switching project',async({page})=>{
 await start(page);
 await page.evaluate(()=>{window.data.project={key:'OTHER',name:'Other'};window.instance=window.xnautJournal.mount(document.querySelector('#journal'),'OTHER',()=>{});});
 await expect(page.locator('[data-title]')).toHaveText('Other · Live Journal');
 await expect(page.locator('[data-agent-run]')).toHaveCount(0);
 await expect(page.locator('[data-actions]')).not.toContainText('Repository access failed');
 await expect(page.locator('[data-handoffs]')).not.toContainText('Routing handoff');
 await expect(page.locator('[data-filter] option')).toHaveCount(1);
});

test('handoffs retain evidence and exact wiki links; tabs work with keyboard',async({page})=>{
 await start(page);
 await page.getByRole('tab',{name:'Actions',exact:true}).press('End');
 await expect(page.getByRole('tab',{name:'Handoffs',exact:true})).toBeFocused();
 await page.locator('[data-handoff="run-2"] summary').click();
 await expect(page.locator('[data-handoffs]')).toContainText('Agent claim');
 await page.locator('[data-handoff-wiki]').click();
 expect(await page.evaluate(()=>window.openedWiki)).toBe('Development/handoffs/routing.md');
 await page.evaluate(()=>window.instance.refresh());
 await expect(page.locator('[data-handoff="run-2"]')).toHaveAttribute('open');
});

test('tools retain account SSH, web and project verification without approvals',async({page})=>{
 await start(page);
 await page.getByRole('button',{name:'Computers & verification',exact:true}).click();
 await expect(page.locator('dialog')).toContainText('whole account');
 await expect(page.locator('dialog')).not.toContainText('OTHER-1');
 await page.locator('dialog summary').click();
 await expect(page.locator('dialog')).toContainText('abc123');
 await page.getByRole('button',{name:'Terminal',exact:true}).click();
 expect(await page.evaluate(()=>window.calls.find(c=>c.name==='create_command_session').args.config)).toMatchObject({program:'ssh',args:['-t','-o','StrictHostKeyChecking=accept-new','testbox.exe.xyz']});
 await page.getByRole('button',{name:'Web ↗',exact:true}).click();
 expect(await page.evaluate(()=>window.openedWeb)).toBe('https://testbox.exe.xyz');
});

test('narrow pane keeps tabs and cards inside its bounds and polling preserves reading state',async({page})=>{
 await start(page);
 await page.locator('#journal').evaluate(e=>{e.style.width='420px';e.style.height='820px';});
 await page.getByRole('tab',{name:'Handoffs',exact:true}).click();
 await page.locator('[data-handoff] summary').click();
 const before=await page.locator('[data-handoff]').elementHandle();
 await page.evaluate(()=>window.instance.refresh());
 expect(await before.evaluate(el=>el.isConnected)).toBe(true);
 expect(await page.locator('.pj').evaluate(e=>e.scrollWidth<=e.clientWidth)).toBe(true);
 await page.screenshot({path:'test-results/journal-console-narrow.png'});
});

test('older action pages use the native cursor and a date change returns to latest',async({page})=>{
 await start(page);
 await page.evaluate(()=>{
  window.data.console.activity.next_before=100;
  window.data.console.activity.total=600;
  window.data.console.activity.limit=500;
  const invoke=window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke=async(name,args)=>{
   const result=await invoke(name,args);
   if(name==='project_journal_read'&&args.actionBefore===100){result.console.activity.next_before=null;result.console.activity.entries[0].detail='Earlier action';}
   return result;
  };
  return window.instance.refresh();
 });
 await page.getByRole('button',{name:'Older actions',exact:true}).click();
 await expect(page.locator('[data-actions]')).toContainText('Earlier action');
 await expect(page.getByRole('button',{name:'Latest actions',exact:true})).toBeVisible();
 await page.getByLabel('Journal date').selectOption('Development/journal/2026-10-09.md');
 await expect(page.locator('[data-actions]')).toContainText('Repository access failed');
 expect(await page.evaluate(()=>window.calls.filter(c=>c.name==='project_journal_read').at(-1).args.actionBefore)).toBe(null);
});

test('retired Agent links open the Journal while central Agent Space stays available',async({page})=>{
 await page.addInitScript(()=>{localStorage.setItem('xnaut-right-pane-visible','1');localStorage.setItem('xnaut-sidebar-visible','1');});
 await page.goto('/?stub=1');
 await page.getByRole('button',{name:'More surfaces'}).click();
 await page.getByText('Agent Space',{exact:true}).first().click();
 await expect(page.locator('.agent-space')).toBeVisible();
 await page.evaluate(()=>window.xnautRightPaneShow('agent'));
 await expect(page.locator('[data-page="journal"]')).toBeVisible();
 await expect(page.locator('[data-rpane-view="agent"]')).toHaveCount(0);
 await expect(page.locator('.agent-space')).toBeVisible();
});

test('historical actions remain filterable after a ticket leaves the current board',async({page})=>{
 await start(page);
 await page.evaluate(()=>{window.data.console.activity.entries[0].ticket='DEMO-99';return window.instance.refresh();});
 await page.getByLabel('Journal workstream').selectOption('DEMO-99');
 await expect(page.locator('[data-actions]')).toContainText('Repository access failed');
 await expect(page.locator('[data-agent-run]')).toHaveCount(5);
});
