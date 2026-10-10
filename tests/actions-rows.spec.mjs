// XNAUT-489: preserve actionable rows when the Agent timeline moves to Journal.
import {test,expect} from '@playwright/test';
import {startJournalConsole} from './helpers/journal-console-fixture.mjs';
async function start(page){
 await startJournalConsole(page);
 await page.evaluate(()=>{
  const row=(kind,detail,session='')=>({at:'2026-10-10T12:00:00Z',run_id:'run-2',ticket:'DEMO-3',agent:'pi',kind,detail,session});
  window.data.console.activity.entries=[row('registry_failed','Admission refused','session-0'),row('adopted','Recorded adoption','dead-session'),row('dispatched','Worker launched'),...Array.from({length:30},()=>row('launch_not_durable','Launch was not durable'))];
  window.data.console.activity.total=33;
  return window.instance.refresh();
 });
}
test('action ticket links navigate with exact project and ticket',async({page})=>{
 await start(page);await page.locator('[data-act-ticket]').first().click();
 expect(await page.evaluate(()=>window.openedTicket)).toEqual({project:'DEMO',ticket:'DEMO-3',tab:'tests'});
});
test('action run inspection uses project-scoped native evidence',async({page})=>{
 await start(page);await page.locator('[data-action-run]').first().click();
 await expect(page.locator('dialog')).toContainText('Exact run run-2');
 expect(await page.evaluate(()=>window.calls.find(c=>c.name==='project_wiki_source').args)).toEqual({project:'DEMO',kind:'run',id:'run-2'});
 await page.getByRole('button',{name:'Close',exact:true}).click();
 await expect(page.locator('[data-act-kind="dispatched"]')).toBeVisible();
});
test('missing run evidence reports failure without removing the action trail',async({page})=>{
 await start(page);await page.evaluate(()=>{window.failSource=true;});
 await page.locator('[data-action-run]').first().click();
 await expect(page.locator('[data-status]')).toContainText('Run record is unavailable');
 await expect(page.locator('[data-act-kind="registry_failed"]')).toBeVisible();
});
test('only session IDs known by the native tracker are attachable',async({page})=>{
 await start(page);await expect(page.locator('[data-act-session]')).toHaveCount(1);
 await page.locator('[data-act-session]').click();expect(await page.evaluate(()=>window.openedSession[0])).toBe('session-0');
});
test('replayed events are compact but every occurrence remains inspectable',async({page})=>{
 await start(page);await expect(page.locator('[data-act-kind="launch_not_durable"]')).toHaveCount(1);
 const repeats=page.locator('[data-action-repeat]');await expect(repeats.locator('summary')).toHaveText('30 occurrences');
 await repeats.locator('summary').click();await expect(repeats.locator('p')).toHaveCount(30);
 const node=await repeats.elementHandle();await page.evaluate(()=>window.instance.refresh());
 expect(await node.evaluate(n=>n.isConnected)).toBe(true);await expect(repeats).toHaveAttribute('open');
});
test('kind filter and clear are keyboard accessible and retain focus',async({page})=>{
 await start(page);await page.locator('[data-act-kind="dispatched"]').press('Enter');
 await expect(page.locator('[data-act-kind="dispatched"]')).toBeFocused();
 await expect(page.locator('[data-act-kind="registry_failed"]')).toHaveCount(0);
 await page.locator('[data-act-clear-kind]').press(' ');
 await expect(page.locator('[data-act-kind="registry_failed"]')).toHaveCount(1);
});
test('untrusted action details remain text and do not execute',async({page})=>{
 await start(page);await page.evaluate(()=>{window.data.console.activity.entries[0].detail='<img src=x onerror="window.pwned=true">';return window.instance.refresh();});
 await expect(page.locator('[data-actions] img')).toHaveCount(0);expect(await page.evaluate(()=>window.pwned)).toBeUndefined();
});
