import {test,expect} from '@playwright/test';

test.beforeEach(async ({page}) => { await page.addInitScript(() => {localStorage.setItem('xnaut-right-pane-visible','1');localStorage.setItem('xnaut-sidebar-visible','1');}); });

async function openNotes(page, chatKey='notes-a') {
  await page.evaluate(key => window.xnautAttachChatTab({chatKey:key,title:key}), chatKey);
  await page.evaluate(() => window.xnautRightPaneShow('workspace'));
  await page.locator('[data-sub="wiki"]').click();
  await expect(page.getByRole('button',{name:'+ Add note',exact:true})).toBeEnabled();
  return page.locator('[data-notebook-host]');
}
async function addNote(pane, title, text) {
  await pane.getByRole('button',{name:'+ Add note',exact:true}).click();
  await pane.getByLabel('Note title').first().fill(title);
  await pane.getByLabel('Note text').first().fill(text);
  await pane.getByRole('button',{name:'Done',exact:true}).click();
}
test('personal notes and checklists survive native hydration and stay with their conversation', async({page}) => {
  await page.goto('/?stub=1'); await page.waitForSelector('#btn-help');
  let pane = await openNotes(page);
  await addNote(pane,'Audit reminders','Keep the review read-only.\n- [ ] Read the report');
  await pane.getByRole('checkbox',{name:'Read the report'}).check();
  await page.evaluate(() => window.xnautConversationStorage.flush());
  await openNotes(page,'notes-b');
  await expect(pane.getByLabel('Note title')).toHaveCount(0);
  await addNote(pane,'Other conversation','Different note');
  await page.evaluate(async() => {await window.xnautConversationStorage.flush(); Object.keys(localStorage).filter(k=>k.startsWith('xnaut-notebook:')).forEach(k=>localStorage.removeItem(k));});
  await page.reload(); await page.waitForSelector('#btn-help'); pane=await openNotes(page);
  await expect(pane.getByLabel('Note title')).toHaveValue('Audit reminders');
  await expect(pane.getByRole('checkbox',{name:'Read the report'})).toBeChecked();
  expect(await page.evaluate(()=>window.__xnautInvokes.filter(i=>i.cmd==='notebook_distill').length)).toBe(0);
});

test('summary stays with its source after navigation and preserves edits made while it runs',async({page})=>{
  await page.goto('/?stub=1'); await page.waitForSelector('#btn-help');
  const pane=await openNotes(page);
  await page.evaluate(()=>{
    window.xnautSetChatHistory('notes-a',[{role:'user',content:'How do I review JobUp?'},{role:'assistant',content:'Read the report; do not change production.'}]);
    const invoke=window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke=(cmd,args)=>cmd==='notebook_distill'?new Promise(resolve=>{window.__summaryArgs=args;window.__summaryResolve=resolve;}):invoke(cmd,args);
  });
  await pane.getByRole('button',{name:'Summarize conversation'}).click();
  await addNote(pane,'My extra note','Do not lose this while summarizing');
  await openNotes(page,'notes-b');
  await page.evaluate(()=>window.__summaryResolve({title:'Review JobUp',summary:'Audit results still need review.',tasks:['Read findings'],remember:['Production unchanged']}));
  await expect(pane.getByLabel('Note title')).toHaveCount(0);
  await openNotes(page);
  await expect(pane.getByLabel('Note title')).toHaveCount(2);
  await expect(pane.getByRole('checkbox',{name:'Read findings'})).toBeVisible();
  await expect(pane).toContainText('Do not lose this while summarizing');
  expect(await page.evaluate(()=>window.__summaryArgs.messages[0].content)).toBe('How do I review JobUp?');
});

test('agent thread notes are separate from ordinary chat and failed saves stay visible',async({page})=>{
  await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
  const pane=await openNotes(page);
  await addNote(pane,'Chat note','This is the ordinary chat');
  await page.evaluate(()=>window.xnautOpenAgentSpace('builder'));
  await expect(pane.getByLabel('Note title')).toHaveCount(0);
  await addNote(pane,'Agent note','Ask builder for evidence');
  const keys=await page.evaluate(()=>window.xnautConversationStorage.keys());
  expect(keys.some(k=>k.startsWith('xnaut-notebook:agent:builder:'))).toBe(true);
  await page.evaluate(()=>{window.__xnautStub.conversation_store_put={__reject:'Disk full'};});
  await pane.getByLabel('Note title').fill('Preserve my edit');
  await expect(pane.getByRole('status')).toContainText('Could not save');
  expect(await page.evaluate(()=>Object.keys(localStorage).filter(k=>k.startsWith('xnaut-conversation-pending:xnaut-notebook:')).length)).toBeGreaterThan(0);
});

test('failed summaries do not replace notes and documentation remains available',async({page})=>{
  await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
  const pane=await openNotes(page);
  await addNote(pane,'Remember','Keep this note');
  await page.evaluate(()=>{
    window.xnautSetChatHistory('notes-a',[{role:'user',content:'Summarize this task'}]);
    window.__xnautStub.notebook_distill={__reject:'Gateway unavailable'};
  });
  await pane.getByRole('button',{name:'Summarize conversation'}).click();
  await expect(pane.getByRole('status')).toContainText('Gateway unavailable');
  await expect(pane.getByLabel('Note title')).toHaveValue('Remember');
  await page.getByRole('button',{name:'Documentation',exact:true}).click();
  await expect(page.locator('[data-wiki-url]')).toBeVisible();
  await page.getByRole('button',{name:'Notes',exact:true}).click();
  await expect(pane.getByLabel('Note title')).toHaveValue('Remember');
});
