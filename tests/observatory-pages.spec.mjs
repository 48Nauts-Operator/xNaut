import { test, expect } from '@playwright/test';
const summary = {configured:true, active:true, month:'2026-09',monthCostUsd:1.5,monthWords:1490,unconfirmedSessions:1,unpricedSessions:0,
  session:{id:'test',model:'gpt-live-1',seconds:90,words:149,ratePerMinute:0.05,finalized:false}};
async function open(page, overrides={}) {
  await page.goto('/?stub=1'); await page.waitForSelector('#btn-help');
  await page.evaluate(({summary,overrides}) => Object.assign(window.__xnautStub, {
    voice_live_usage:summary,
    max_usage:{five_hour_pct:2,seven_day_pct:51,per_model:[{name:'Opus',percent:12},{name:'Fable',percent:56}]},
    ledger_recent:Array.from({length:27},(_,i)=>({at:'2026-09-29T08:00:00Z',kind:'dispatch',ticket:`XNAUT-${i}`,detail:`Entry ${i}`})),
    run_registry_list:Array.from({length:17},(_,i)=>({kind:'agent',ticket:`XNAUT-${i}`,project:'XNAUT',state:'done'})),
    agent_sessions_list:Array.from({length:12},(_,i)=>({session_id:`s${i}`,agent_id:'claude',label:`Task ${i}`,status:'working',started_at_ms:Date.now()-i*1000})),
    zellij_sessions_info:[], loom_runs_list:[], ...overrides
  }),{summary,overrides});
  await page.evaluate(()=>window.xnautAttachObservatoryTab());
  await expect(page.locator('.obs')).toBeVisible();
}
test('General and Agents separate content; ledger and agent lists paginate and survive refresh',async({page})=>{
  await open(page);
  await expect(page.locator('[data-ledger-list] .obs-led:visible')).toHaveCount(5);
  await expect(page.locator('[data-rows]')).not.toBeVisible();
  const ledger=page.locator('[data-pager="Ledger"]');
  await ledger.getByRole('button',{name:'Next Ledger page'}).click();
  await expect(page.locator('[data-ledger-list] .obs-led:visible').first()).toContainText('Entry 5');
  await ledger.getByRole('combobox').selectOption('10');
  await expect(page.locator('[data-ledger-list] .obs-led:visible')).toHaveCount(10);
  await ledger.getByRole('combobox').selectOption('25');
  await expect(page.locator('[data-ledger-list] .obs-led:visible')).toHaveCount(25);
  await ledger.getByRole('button',{name:'Next Ledger page'}).click();
  await expect(page.locator('[data-ledger-list] .obs-led:visible')).toHaveCount(2);
  // Repeated select commit/blur events must not reset a later page.
  await ledger.getByRole('combobox').dispatchEvent('change');
  await expect(ledger).toContainText('26–27 of 27');
  await page.locator('[data-refresh]').click();
  await expect(ledger).toContainText('26–27 of 27');
  await page.getByRole('tab',{name:'Agents',exact:true}).click();
  await expect(page.locator('[data-strip]')).not.toBeVisible();
  await expect(page.locator('[data-rows] .obs-row:visible')).toHaveCount(5);
  await expect(page.locator('[data-swarm-pills] .obs-pill:visible')).toHaveCount(5);
  await page.getByRole('button',{name:'Next Running agents page',exact:true}).click();
  await expect(page.locator('[data-rows] .obs-row:visible').first()).toContainText('Task 5');
  await page.getByRole('button',{name:'Next Dispatched runs page',exact:true}).click();
  await expect(page.locator('[data-swarm-pills] .obs-pill:visible').first()).toContainText('XNAUT-5');
  await page.evaluate(()=>{window.__xnautStub.agent_sessions_list=[];window.__xnautStub.run_registry_list=[];});
  await page.locator('[data-refresh]').click();
  await expect(page.locator('[data-pager="Running agents"]')).toContainText('0 items');
  await expect(page.getByRole('button',{name:'Next Running agents page',exact:true})).toBeDisabled();
});
test('voice cost uses provider duration, savings have editable assumptions, key gates the card',async({page})=>{
  await open(page);
  const card=page.locator('.obs-voice-cost');
  await expect(card).toBeVisible(); await expect(card).toContainText('~$0.075');
  await expect(card).toContainText('1.5 min estimated input time saved');
  await expect(page.locator('[data-voice-cost-footer]')).toContainText('~$0.075');
  await card.locator('summary').click();
  await card.getByRole('spinbutton',{name:'Typing words per minute'}).fill('149');
  await card.getByRole('spinbutton',{name:'Typing words per minute'}).press('Tab');
  await expect(card).toContainText('0.0 min estimated input time saved');
  await page.evaluate(()=>{window.__xnautStub.voice_live_usage.session.finalized=true;window.__xnautStub.voice_live_usage.active=false;return window.xnautVoiceCost.refresh();});
  await expect(card.locator('[data-voice-price]')).toHaveText('$0.075');
  await page.evaluate(()=>{window.__xnautStub.voice_live_usage={__reject:'Cannot read voice usage history'};return window.xnautVoiceCost.refresh();});
  await expect(card).toContainText('Unavailable'); await expect(card).toContainText('Cannot read voice usage history');
  await page.evaluate(()=>{window.__xnautStub.voice_live_usage={configured:false};return window.xnautVoiceCost.refresh();});
  await expect(card).not.toBeVisible(); await expect(page.locator('[data-voice-cost-footer]')).not.toBeVisible();
});
test('Opus appears below Fable; no invented counter when account omits it',async({page})=>{
  await open(page); const rows=page.locator('.obs-mrow');
  await expect(rows.nth(0)).toContainText('Fable'); await expect(rows.nth(1)).toContainText('Opus'); await expect(rows.nth(1)).toContainText('12%');
  await page.evaluate(()=>{window.__xnautStub.max_usage.per_model=[{name:'Fable',percent:56}];});
  await page.locator('[data-refresh]').click();
  await expect(rows.nth(1)).toContainText('Not reported separately');
});
test('cards retain their height and the page scrolls at a laptop viewport',async({page})=>{
  await page.setViewportSize({width:1280,height:800}); await open(page);
  const geometry=await page.locator('.obs').evaluate(pane=>({scroll:pane.scrollHeight,client:pane.clientHeight,ledger:pane.querySelector('[data-ledger-list]').getBoundingClientRect().height}));
  expect(geometry.ledger).toBeGreaterThan(100); expect(geometry.scroll).toBeGreaterThan(geometry.client);
  await page.locator('[data-pager="Ledger"]').scrollIntoViewIfNeeded();
  await expect(page.getByRole('button',{name:'Next Ledger page'})).toBeInViewport();
});


test('a named remote worker opens its real session instead of a guessed CLI command',async({page})=>{
  await open(page,{agent_sessions_list:[{session_id:'cortana-pty',agent_id:'cortana',label:'Cortana',remote_env:'exe-dev',status:'working',started_at_ms:Date.now()}]});
  await page.evaluate(()=>{window.xnautOpenAgentSession=(id,label)=>{window.openedAgent={id,label};};});
  await page.getByRole('tab',{name:'Agents',exact:true}).click();
  await expect(page.locator('[data-rows]')).toContainText('exe-dev');
  await expect(page.locator('[data-rows]')).not.toContainText('claudeps');
  await page.locator('[data-rows] .c-name').click();
  expect(await page.evaluate(()=>window.openedAgent.id)).toBe('cortana-pty');
});


test('Flow Watch opens the real terminal and identifies an authentication screen', async ({page}) => {
  await page.goto('/?stub=1'); await page.waitForSelector('#btn-help');
  await page.evaluate(() => {
    window.__xnautStub.agent_sessions_list=[{session_id:'remote-cortana',agent_id:'cortana',label:'Cortana',status:'working'}];
    window.__xnautStub.terminal_output_snapshot=btoa('2. Sign in with Device Code\n3. Provide your own API key');
    window.xnautFlowWatchView.destroy();
    const host=document.createElement('div'); host.id='flow-test'; document.body.appendChild(host);
    host.style.cssText='position:fixed;inset:100px;background:#111;z-index:99999';
    window.xnautFlowWatchView.mount(host);
    window.xnautOpenAgentSession=(id)=>{window.openedAgent=id;};
  });
  const row=page.locator('#flow-test .fw-row');
  await row.locator('.fw-head').click();
  await expect(row.locator('.fw-status')).toHaveText('sign-in required');
  await page.evaluate(()=>window.__xnautEmit('agent-status-changed',{session_id:'remote-cortana',agent_id:'cortana',label:'Cortana',status:'working'}));
  await expect(row.locator('.fw-status')).toHaveText('sign-in required');
  await row.getByRole('button',{name:'Open session'}).click();
  expect(await page.evaluate(()=>window.openedAgent)).toBe('remote-cortana');
});


test('voice cost resumes updates after the Observatory tab was detached', async ({page}) => {
  await open(page);
  const card=page.locator('.obs-voice-cost');
  await expect(card.locator('[data-voice-price]')).toHaveText('~$0.075');
  await page.evaluate(async()=>{
    window.costPane=document.querySelector('.obs'); window.costParent=window.costPane.parentNode;
    window.costPane.remove(); await window.xnautVoiceCost.refresh();
    window.costParent.appendChild(window.costPane);
    window.__xnautStub.voice_live_usage.session.seconds=120;
    await window.xnautVoiceCost.refresh();
  });
  await expect(card.locator('[data-voice-price]')).toHaveText('~$0.100');
});
