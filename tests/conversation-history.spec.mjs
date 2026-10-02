import {test,expect} from '@playwright/test';

test('an earlier owner repository survives the model window and reload without assistant or partial voice grants', async ({page}) => {
 await page.addInitScript(() => {
  if (localStorage.getItem('__fixture_conversations')) return;
  const messages = [
   {id:'repo',role:'user',text:'/tmp/vynl-owner-project'},
   {id:'invented',role:'agent',text:'/tmp/assistant-invented-project'},
   {id:'partial',role:'user',text:'/tmp/unfinished-voice-project',voiceTranscript:true},
   ...Array.from({length:40},(_,i)=>({id:'later-'+i,role:i%2?'agent':'user',text:'Discussed feature '+i})),
  ];
  const value=JSON.stringify({builder:[{id:'repo-history',title:'Vynl',created_at:'2026-10-02',updated_at:'2026-10-02',messages}]});
  localStorage.setItem('__fixture_conversations',JSON.stringify({'xnaut-agent-threads:v1':{value,revision:1}}));
 });
 for (let pass=0;pass<2;pass++) {
  await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
  await page.evaluate(()=>window.xnautOpenAgentSpace('builder','repo-history'));
  const composer=page.getByLabel('Message @builder');
  await composer.fill('Launch the two developers for the project we discussed');await composer.press('Enter');
  await expect(page.getByText('The release is tagged and the cask is on 1.15.0.',{exact:true}).last()).toBeVisible();
  const request=await page.evaluate(()=>window.__xnautInvokes.findLast(i=>i.cmd==='agent_chat_turn'));
  expect(request.args.messages.length).toBeLessThanOrEqual(16);
  expect(request.args.messages.some(m=>m.content.includes('/tmp/vynl-owner-project'))).toBe(false);
  expect(request.args.repositoryContext).toContain('/tmp/vynl-owner-project');
  expect(request.args.repositoryContext).not.toContain('/tmp/assistant-invented-project');
  expect(request.args.repositoryContext).not.toContain('/tmp/unfinished-voice-project');
  await expect(page.locator('.as-build')).toHaveCount(0);
  await page.evaluate(()=>window.xnautConversationStorage.flush());
 }
});

test('new webview hydrates native agent history before selecting a thread and keeps over 80 messages',async({page})=>{
 await page.addInitScript(()=>{
  const messages=Array.from({length:90},(_,i)=>({id:'m'+i,role:i%2?'agent':'user',text:'Retained message '+i,at:'2026-09-29T08:00:00Z'}));
  const value=JSON.stringify({builder:[{id:'saved',title:'Test xNaut',created_at:'2026-09-29',updated_at:'2026-09-29',messages}]});
  localStorage.setItem('__fixture_conversations',JSON.stringify({'xnaut-agent-threads:v1':{value,revision:3}}));
 });
 await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
 await page.evaluate(()=>window.xnautOpenAgentSpace('builder','saved'));
 await expect(page.getByText('Retained message 0',{exact:true})).toBeVisible();
 await page.getByLabel('Message @builder').fill('Please retain the entire conversation');await page.getByLabel('Message @builder').press('Enter');
 await expect(page.getByText('The release is tagged and the cask is on 1.15.0.',{exact:true})).toBeVisible();
 await page.evaluate(()=>window.xnautConversationStorage.flush());
 const saved=await page.evaluate(()=>JSON.parse(JSON.parse(localStorage.getItem('__fixture_conversations'))['xnaut-agent-threads:v1'].value));
 expect(saved.builder.find(t=>t.id==='saved').messages).toHaveLength(92);
 expect(saved.builder.find(t=>t.id==='saved').messages[0].text).toBe('Retained message 0');
 const request=await page.evaluate(()=>window.__xnautInvokes.findLast(i=>i.cmd==='agent_chat_turn'));
 expect(request.args.messages.length).toBeLessThan(30);
});

test('history picker reopens old chat in a fresh profile and subsequent writes survive reload',async({page})=>{
 await page.addInitScript(()=>{
  if (!localStorage.getItem('__fixture_conversations')) localStorage.setItem('__fixture_conversations',JSON.stringify({
   'xnaut-chat-history:previous':{value:JSON.stringify([{role:'user',content:'Earlier test conversation'},{role:'assistant',content:'Saved reply'}]),revision:1},
   'xnaut-chat-title:previous':{value:JSON.stringify('Test xNaut'),revision:1}
  }));
 });
 await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
 await page.evaluate(()=>window.xnautAttachChatTab({chatKey:'fresh'}));
 await page.getByRole('combobox',{name:'Open saved chat'}).selectOption('previous');
 await expect(page.getByText('Saved reply',{exact:true})).toBeVisible();
 await page.evaluate(async()=>{window.xnautSetChatHistory('previous',[{role:'user',content:'Earlier test conversation'},{role:'assistant',content:'Saved reply'},{role:'user',content:'New retained message'}]);await window.xnautConversationStorage.flush();localStorage.removeItem('xnaut-chat-history:previous');});
 await page.reload();await page.waitForSelector('#btn-help');await page.evaluate(()=>window.xnautAttachChatTab({chatKey:'previous'}));
 await expect(page.getByText('New retained message',{exact:true})).toBeVisible();
});

test('failed native writes are visible and keep an outbox instead of claiming success',async({page})=>{
 await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
 await page.evaluate(async()=>{
  window.__xnautStub.conversation_store_put={__reject:'Disk full'};
  window.xnautConversationStorage.setItem('xnaut-chat-history:failure',JSON.stringify([{role:'user',content:'Keep this'}]));
  await window.xnautConversationStorage.flush();
 });
 await expect(page.getByRole('alert').filter({hasText:'Conversation storage:'})).toContainText('Disk full');
 expect(await page.evaluate(()=>localStorage.getItem('xnaut-conversation-pending:xnaut-chat-history:failure'))).toContain('Keep this');
});
