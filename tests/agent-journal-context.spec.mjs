import {test, expect} from '@playwright/test';
const projects = [
 {key:'XNAUT',name:'xnaut',root:'/work/xnaut'},
 {key:'CHESSTRAINER',name:'ChessTrainer',root:'/work/ChessTrainer'},
 {key:'VYNL',name:'Vynl',root:'/work/Vynl'},
];
async function setup(page, threads = []) {
 await page.addInitScript(threads => {
  localStorage.setItem('xnaut-agent-threads:v1',JSON.stringify({nautbot:threads}));
  localStorage.setItem('xnaut-right-pane-visible','1');
 }, threads);
 await page.goto('/?stub=1');
 await page.waitForFunction(()=>window.xnautOpenAgentSpace && window.__xnautStub);
 await page.evaluate(projects=>{
  const invoke=window.__TAURI__.core.invoke;
  window.projectCalls=[];
  window.__xnautStub.voice_live_ready=true;
  window.__TAURI__.core.invoke=async(name,args)=>{
   if(name==='project_wiki_projects')return projects;
   if(name==='project_journal_read')return {project:projects.find(p=>[p.key,p.root].includes(args.project)),path:'Development/journal/2026-10-03.md',documents:[],entries:[],runs:[],opening:'Saved project context',observed_at:new Date().toISOString()};
   if(name==='agent_chat_turn') {window.projectCalls.push(args);if(window.holdReply)return new Promise(resolve=>{window.finishReply=resolve;});return 'Recorded agent reply.';}
   return invoke(name,args);
  };
  window.xnautActiveProjectPath=()=>'/work/xnaut';
 },projects);
 await page.evaluate(()=>window.xnautOpenAgentSpace('nautbot'));
 await expect(page.getByLabel('Message @nautbot')).toBeVisible();
}
async function send(page,text) {
 await page.getByLabel('Message @nautbot').fill(text);
 await page.getByLabel('Message @nautbot').press('Enter');
 await expect(page.locator('[data-send]')).toBeEnabled();
}
const thread=(id,text)=>({id,title:text,updated_at:'2026-10-03T21:00:00Z',messages:[{id:id+'-user',role:'user',text,at:'2026-10-03T21:00:00Z',journalProject:'/work/xnaut'}]});

test('Chess App overrides stale workspace before capture and follow-ups retain project',async({page})=>{
 await setup(page);
 await send(page,'whats the status of the Chess App?');
 await expect(page.locator('[data-title]')).toHaveText('ChessTrainer · Live Journal');
 await expect(page.locator('[data-journal-project] option:checked')).toHaveText('Auto · ChessTrainer');
 await send(page,'What remains to finish it?');
 expect(await page.evaluate(()=>window.projectCalls.map(c=>c.projectScope))).toEqual(['CHESSTRAINER','CHESSTRAINER']);
 const messages=await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).nautbot[0].messages);
 expect(messages.map(m=>m.journalProject)).toEqual(Array(4).fill('CHESSTRAINER'));
 await send(page,'Now check VYNL-2');
 await expect(page.locator('[data-title]')).toHaveText('Vynl · Live Journal');
});
test('opening saved conversations follows owner context without rewriting old messages',async({page})=>{
 await setup(page,[thread('chess','Status of the Chess App?'),thread('vynl','Review VYNL-2')]);
 await expect(page.locator('[data-title]')).toHaveText('ChessTrainer · Live Journal');
 await page.evaluate(()=>window.xnautOpenAgentSpace('nautbot','vynl'));
 await expect(page.locator('[data-title]')).toHaveText('Vynl · Live Journal');
 await page.evaluate(()=>window.xnautOpenAgentSpace('nautbot','chess'));
 await expect(page.locator('[data-title]')).toHaveText('ChessTrainer · Live Journal');
 expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).nautbot.find(t=>t.id==='chess').messages[0].journalProject)).toBe('/work/xnaut');
});
test('pin survives reopening and auto resumes owner context',async({page})=>{
 await setup(page,[thread('chess','Status of the Chess App?')]);
 await page.getByLabel('Conversation project').selectOption('VYNL');
 await expect(page.locator('[data-title]')).toHaveText('Vynl · Live Journal');
 await send(page,'Check ChessTrainer as a comparison');
 expect(await page.evaluate(()=>window.projectCalls[0].projectScope)).toBe('VYNL');
 await page.evaluate(()=>window.xnautOpenAgentSpace('nautbot','chess'));
 await expect(page.getByLabel('Conversation project')).toHaveValue('VYNL');
 await page.getByLabel('Conversation project').selectOption('');
 await expect(page.locator('[data-title]')).toHaveText('ChessTrainer · Live Journal');
});
test('multiple-project discussion is unassigned, assistant references do not choose scope',async({page})=>{
 await setup(page);
 await send(page,'Compare ChessTrainer and Vynl');
 expect(await page.evaluate(()=>window.projectCalls[0].projectScope)).toBe('');
 await expect(page.locator('[data-journal-project] option:checked')).toHaveText('Multiple projects · choose');
 await send(page,'Continue the comparison');
 expect(await page.evaluate(()=>window.projectCalls[1].projectScope)).toBe('');
 const result=await page.evaluate(projects=>window.xnautAgentProjectContext.resolve({projects,thread:{messages:[{role:'user',text:'Chess App status?'},{role:'agent',text:'Use xnaut and Vynl'}]}}),projects);
 expect(result.project.key).toBe('CHESSTRAINER');
});
test('delayed background answer retains original scope and cannot move visible Journal',async({page})=>{
 await setup(page,[thread('chess','Status of the Chess App?'),thread('vynl','Review VYNL-2')]);
 await page.evaluate(()=>{window.holdReply=true;});
 await page.getByLabel('Message @nautbot').fill('Check it');
 await page.getByLabel('Message @nautbot').press('Enter');
 await expect.poll(()=>page.evaluate(()=>Boolean(window.finishReply))).toBe(true);
 await page.evaluate(()=>window.xnautOpenAgentSpace('nautbot','vynl'));
 await expect(page.locator('[data-title]')).toHaveText('Vynl · Live Journal');
 await page.evaluate(()=>window.finishReply('Chess check completed.'));
 await expect.poll(()=>page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).nautbot.find(t=>t.id==='chess').messages.at(-1).text)).toBe('Chess check completed.');
 await expect(page.locator('[data-title]')).toHaveText('Vynl · Live Journal');
 expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).nautbot.find(t=>t.id==='chess').messages.at(-1).journalProject)).toBe('CHESSTRAINER');
});
test('qualified aliases are conservative and collisions remain unresolved',async({page})=>{
 await setup(page);
 const results=await page.evaluate(projects=>{
  const resolve=text=>window.xnautAgentProjectContext.resolve({text,projects});
  return [resolve('play chess'),resolve('Chess project'),window.xnautAgentProjectContext.resolve({text:'Chess App',projects:[...projects,{key:'CHESSAPP',name:'ChessApp',root:'/other'}]})];
 },projects);
 expect(results[0].project).toBeNull();expect(results[1].project.key).toBe('CHESSTRAINER');expect(results[2].reason).toBe('multiple');
});

test('ordinary keep instruction does not switch to the Keep project; xNAUT spelling resolves',async({page})=>{
 await setup(page);
 const results=await page.evaluate(projects=>{
  projects.push({key:'KEEP',name:'Keep',root:'/work/Keep'});
  const resolve=text=>window.xnautAgentProjectContext.resolve({text,projects});
  return ['keep the existing changes','Keep app','KEEP-42','check xNAUT'].map(resolve);
 },projects);
 expect(results[0].project).toBeNull();expect(results[1].project.key).toBe('KEEP');expect(results[2].project.key).toBe('KEEP');expect(results[3].project.key).toBe('XNAUT');
});
test('returning from another top-level tab restores the conversation Journal',async({page})=>{
 await setup(page,[thread('chess','Status of the Chess App?')]);
 await expect(page.locator('[data-title]')).toHaveText('ChessTrainer · Live Journal');
 await page.evaluate(()=>window.xnautOpenWorkspace({project:'XNAUT'}));
 await page.evaluate(()=>window.xnautRightPaneSetRoot('/work/xnaut'));
 await page.locator('.tab').filter({hasText:'Agent Space'}).click();
 await expect(page.locator('[data-title]')).toHaveText('ChessTrainer · Live Journal');
});
test('voice commit resolves before capture and dispatch uses the same project',async({page})=>{
 await setup(page);
 await page.evaluate(()=>{window.__xnautStub.voice_live_ready=true;});
 await page.locator('.agent-space .voice-live-button').click();
 await page.getByRole('menuitem',{name:'STS · Speech to Speech',exact:true}).click();
 await expect.poll(()=>page.evaluate(()=>window.__xnautInvokes.filter(i=>i.cmd==='voice_live_open').length)).toBe(1);
 await page.evaluate(()=>{
  const id=window.__xnautInvokes.find(i=>i.cmd==='voice_live_open').args.sessionId;
  for(const event of [{kind:'ready'},{kind:'commit',role:'user',text:'What is happening in the Chess App?',turn:0},{kind:'dispatch',turn:0,epoch:0}])window.__xnautEmit(`voice-live://${id}`,event);
 });
 await expect(page.locator('[data-title]')).toHaveText('ChessTrainer · Live Journal');
 await expect.poll(()=>page.evaluate(()=>window.projectCalls.length)).toBe(1);
 expect(await page.evaluate(()=>window.projectCalls[0].projectScope)).toBe('CHESSTRAINER');
 expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).nautbot[0].messages.find(m=>m.role==='user').journalProject)).toBe('CHESSTRAINER');
});
