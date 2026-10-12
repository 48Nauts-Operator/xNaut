import { test, expect } from '@playwright/test';
const at = '2026-10-12T07:00:00Z';
const messages = [
  { id:'first', role:'user', text:'First question\nSecond line', at },
  { id:'reply', role:'agent', text:'## First answer\n\nKeep **markdown** intact.', at },
  { id:'action', kind:'action', label:'Started', detail:'Existing task', at },
  { id:'last', role:'user', text:'Latest question', at },
];
async function open(page, seed = messages) {
  await page.addInitScript(({seed,at}) => {
    if (!localStorage.getItem('sort-test-seeded')) {
      localStorage.setItem('xnaut-agent-threads:v1', JSON.stringify({builder:[
        {id:'sort',title:'Sort test',created_at:at,updated_at:at,messages:seed},
        {id:'other',title:'Other',created_at:at,updated_at:at,messages:[]},
      ]}));
      localStorage.setItem('sort-test-seeded','1');
    }
  }, {seed,at});
  await page.goto('/?stub=1');
  await page.waitForFunction(() => typeof window.xnautOpenAgentSpace === 'function');
  await page.evaluate(() => window.xnautOpenAgentSpace('builder','sort'));
  await expect(page.getByLabel('Message order')).toBeVisible();
}
const keys = page => page.locator('[data-messages] > [data-message-key]').evaluateAll(nodes=>nodes.map(n=>n.dataset.messageKey));
test('sort reverses complete blocks, preserves storage and copy, and persists across threads and restart', async ({page}) => {
  await open(page);
  await expect(page.getByLabel('Message order')).toHaveValue('oldest');
  expect(await keys(page)).toEqual(['first','reply','action','last']);
  await page.getByLabel('Message order').selectOption('newest');
  expect(await keys(page)).toEqual(['last','action','reply','first']);
  await expect(page.locator('[data-message-id="reply"] h2')).toHaveText('First answer');
  await page.evaluate(() => Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.sortCopied=text;}}}));
  await page.locator('[data-copy-message="first"]').click();
  expect(await page.evaluate(()=>window.sortCopied)).toBe(messages[0].text);
  expect(await page.evaluate(()=>JSON.parse(window.xnautConversationStorage.getItem('xnaut-agent-threads:v1')).builder.find(t=>t.id==='sort').messages)).toEqual(messages);
  await page.evaluate(()=>window.xnautOpenAgentSpace('builder','other'));
  await expect(page.getByLabel('Message order')).toHaveValue('newest');
  await page.reload();
  await page.waitForFunction(()=>typeof window.xnautOpenAgentSpace==='function');
  await page.evaluate(()=>window.xnautOpenAgentSpace('builder','sort'));
  await expect(page.getByLabel('Message order')).toHaveValue('newest');
  expect(await keys(page)).toEqual(['last','action','reply','first']);
  await page.getByLabel('Message order').selectOption('oldest');
  expect(await keys(page)).toEqual(['first','reply','action','last']);
});
test('newest-first streams into one top reply and still sends chronological model context', async ({page}) => {
  await open(page); await page.getByLabel('Message order').selectOption('newest');
  await page.evaluate(()=>{
    const invoke=window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke=async(name,args)=>{
      if(name!=='agent_chat_turn')return invoke(name,args);
      window.sortRequest=args;
      window.__xnautEmit('chat://chunk',{requestId:args.requestId,delta:'Streaming answer'});
      await new Promise(resolve=>{window.finishSortReply=resolve;});
      return 'Final answer';
    };
  });
  await page.getByLabel('Message @builder').fill('New question');
  await page.getByLabel('Message @builder').press('Enter');
  await expect(page.locator('[data-messages] > [data-message-key]').first()).toContainText('Streaming answer');
  await page.getByLabel('Message order').selectOption('oldest');
  await expect(page.locator('[data-messages] > [data-message-key]').last()).toContainText('Streaming answer');
  await page.getByLabel('Message order').selectOption('newest');
  await expect(page.locator('[data-messages] > [data-message-key]').first()).toContainText('Streaming answer');
  const context=await page.evaluate(()=>window.sortRequest.messages);
  expect(context.filter(m=>['First question\nSecond line','Latest question','New question'].includes(m.content)).map(m=>m.content)).toEqual(['First question\nSecond line','Latest question','New question']);
  await page.evaluate(()=>window.finishSortReply());
  await expect(page.locator('[data-messages] > [data-message-key]').first()).toContainText('Final answer');
  expect((await keys(page)).length).toBe(6);
  expect(new Set(await keys(page)).size).toBe(6);
});
test('empty threads support both orders and keyboard focus',async({page})=>{
  await open(page,[]); await page.getByLabel('Message order').selectOption('newest');
  await expect(page.locator('.as-empty')).toBeVisible();
  await page.getByLabel('Message order').focus(); await expect(page.getByLabel('Message order')).toBeFocused(); await page.getByLabel('Message order').selectOption('oldest');
  await expect(page.getByLabel('Message order')).toHaveValue('oldest');
});

test('single-message order and narrow pane controls remain usable',async({page})=>{
  await open(page,[messages[0]]);
  await page.locator('.as-conv').evaluate(e=>{e.style.width='380px';e.style.height='750px';});
  await page.getByLabel('Message order').selectOption('newest');
  expect(await keys(page)).toEqual(['first']);
  await page.getByLabel('Message order').selectOption('oldest');
  expect(await keys(page)).toEqual(['first']);
  const control=await page.getByLabel('Message order').boundingBox(),pane=await page.locator('.as-conv').boundingBox();
  expect(control.x).toBeGreaterThanOrEqual(pane.x);expect(control.x+control.width).toBeLessThanOrEqual(pane.x+pane.width);
});

test('reading older messages stays anchored while a top reply grows and finishes',async({page})=>{
  const history=Array.from({length:25},(_,i)=>({id:'history-'+i,role:i%2?'agent':'user',text:('History '+i+' line\n').repeat(15),at}));
  await open(page,history);await page.getByLabel('Message order').selectOption('newest');
  await page.evaluate(()=>{
    const invoke=window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke=async(name,args)=>{
      if(name!=='agent_chat_turn')return invoke(name,args);
      window.sortRequest=args;
      await new Promise(resolve=>{window.finishSortReply=resolve;});return 'Final answer';
    };
  });
  await page.getByLabel('Message @builder').fill('New question');await page.getByLabel('Message @builder').press('Enter');
  await page.waitForFunction(()=>typeof window.finishSortReply==='function');
  await expect.poll(()=>page.locator('.as-thread').evaluate(e=>e.scrollTop)).toBe(0);
  // Allow the bounded layout settling pass to finish before the human scroll.
  await page.waitForTimeout(100);
  await page.locator('.as-thread').evaluate(e=>{e.scrollTop=2000;});
  await page.waitForTimeout(100);
  const anchor=await page.locator('[data-messages]').evaluate(e=>{
    const top=e.closest('.as-thread').getBoundingClientRect().top;
    const node=[...e.children].find(n=>n.getBoundingClientRect().bottom>top);
    return {id:node.dataset.messageKey,top:node.getBoundingClientRect().top};
  });
  await page.evaluate(()=>window.__xnautEmit('chat://chunk',{requestId:window.sortRequest.requestId,delta:'Streaming line\n'.repeat(100)}));
  let position=await page.locator(`[data-message-key="${anchor.id}"]`).evaluate(e=>e.getBoundingClientRect().top);
  expect(Math.abs(position-anchor.top)).toBeLessThan(3);
  await page.evaluate(()=>window.finishSortReply());
  await expect(page.locator('[data-messages] > [data-message-key]').first()).toContainText('Final answer');
  position=await page.locator(`[data-message-key="${anchor.id}"]`).evaluate(e=>e.getBoundingClientRect().top);
  expect(Math.abs(position-anchor.top)).toBeLessThan(3);
});
