import {test,expect} from '@playwright/test';
async function open(page,threads){
 await page.addInitScript(threads=>{localStorage.setItem('xnaut-agent-threads:v1',JSON.stringify({builder:threads}));localStorage.setItem('xnaut-sidebar-visible','1');},threads);
 await page.goto('/?stub=1');
 await page.waitForFunction(()=>typeof window.xnautOpenAgentSpace==='function');
 await page.evaluate(()=>window.xnautOpenAgentSpace('builder','old'));
 if(!await page.locator('[data-thread-search]').count())await page.locator('[data-threads-toggle="builder"]').click();
 await expect(page.getByLabel('Find a conversation')).toBeVisible();
}
const thread=(id,date,title=id)=>({id,title,created_at:date,updated_at:'2026-10-11T09:00:00Z',messages:[{id:id+'-user',role:'user',text:'Keep **original** markdown\n\nSecond line',at:date},{id:id+'-agent',role:'agent',text:'## Answer\n\n`const x = 1`',at:date}]});
test('creation dates determine the tree; archive and restore retain entire transcript',async({page})=>{
 const old=thread('old','2025-06-03T12:00:00Z','Earlier conversation'),recent=thread('recent','2026-10-10T12:00:00Z','Recent conversation');
 await open(page,[recent,old]);
 await expect(page.locator('[data-thread-group="2025"]')).toContainText('Earlier conversation');
 await expect(page.locator('[data-thread-group="2026"]')).toContainText('Recent conversation');
 if(!await page.locator('[data-thread-group="2025"]').evaluate(e=>e.open))await page.locator('[data-thread-group="2025"] > summary').click();
 if(!await page.locator('[data-thread-group="2025/June"]').evaluate(e=>e.open))await page.locator('[data-thread-group="2025/June"] > summary').click();
 if(!await page.locator('[data-thread-group="2025/June/Jun 3"]').evaluate(e=>e.open))await page.locator('[data-thread-group="2025/June/Jun 3"] > summary').click();
 const row=page.locator('[data-library-thread="old"]');await row.getByRole('button',{name:/Actions for thread/}).click();await page.locator('[data-thread-menu]').getByRole('button',{name:'Archive',exact:true}).click();
 await page.getByRole('button',{name:'Archived · 1',exact:true}).click();
 await expect(page.locator('[data-library-thread="old"]')).toBeVisible();
 await page.locator('[data-library-thread="old"]').click();
 await expect(page.locator('[data-archive-thread]')).toHaveText('Restore');
 const saved=await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).builder.find(t=>t.id==='old'));
 expect(saved.messages).toEqual(old.messages);expect(saved.archived_at).toBeTruthy();
 await page.locator('[data-library-thread="old"] [data-thread-more]').click();await page.locator('[data-thread-menu]').getByRole('button',{name:'Restore',exact:true}).click();
 await page.getByRole('button',{name:'Active · 2',exact:true}).click();await page.locator('[data-library-thread="old"]').click();
 await expect(page.getByLabel('Message @builder')).toBeVisible();
 expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).builder.find(t=>t.id==='old').messages)).toEqual(old.messages);
});
test('large history is bounded, searchable and reveals additional pages',async({page})=>{
 const threads=Array.from({length:1200},(_,i)=>thread('t'+i,'2026-10-10T12:00:00Z','Conversation '+String(i).padStart(4,'0')));
 await open(page,threads);
 expect(await page.locator('[data-library-thread]').count()).toBeLessThanOrEqual(101);
 await page.getByLabel('Find a conversation').fill('1199');await expect(page.locator('[data-library-thread]')).toHaveCount(1);await expect(page.locator('[data-library-thread]')).toContainText('1199');
 await page.getByLabel('Find a conversation').fill('');await page.locator('[data-thread-more-rows]').click();expect(await page.locator('[data-library-thread]').count()).toBeGreaterThanOrEqual(200);
});
test('visible copy controls copy original text for both roles and report failures',async({page})=>{
 const saved=thread('old','2026-10-10T12:00:00Z');await open(page,[saved]);
 await page.evaluate(()=>{Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.copiedText=text;}}});});
 await page.getByRole('button',{name:'Copy your message',exact:true}).click();expect(await page.evaluate(()=>window.copiedText)).toBe(saved.messages[0].text);await expect(page.locator('[data-copy-message="old-user"]')).toContainText('Copied');
 await page.getByRole('button',{name:'Copy agent message',exact:true}).focus();await page.keyboard.press('Enter');expect(await page.evaluate(()=>window.copiedText)).toBe(saved.messages[1].text);
 await page.evaluate(()=>{navigator.clipboard.writeText=async()=>{throw Error('denied');};});await page.getByRole('button',{name:'Copy your message',exact:true}).click();await expect(page.locator('[data-copy-message="old-user"]')).toContainText('Copy failed');
});

test('narrow conversation header stays above its context bar',async({page})=>{
 await open(page,[thread('old','2026-10-10T12:00:00Z')]);
 await page.locator('.as-conv').evaluate(e=>{e.style.width='380px';e.style.height='750px';});
 const header=await page.locator('.as-head').boundingBox(),context=await page.locator('.as-context-bar').boundingBox();
 expect(header.y+header.height).toBeLessThanOrEqual(context.y+1);
 const archive=await page.locator('[data-archive-thread]').boundingBox();
 expect(archive.y+archive.height).toBeLessThanOrEqual(header.y+header.height);
});
