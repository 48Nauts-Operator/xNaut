import {test,expect} from '@playwright/test';

test('hidden Journal stops polling and resumes when shown', async ({ page }) => {
  await page.clock.install();
  await start(page);
  const reads = () => page.evaluate(() => window.journalCalls.filter(c => c.name === 'project_journal_read').length);
  const before = await reads();
  await page.evaluate(() => { document.querySelector('#journal').hidden = true; });
  await page.clock.fastForward(20_000);
  expect(await reads()).toBe(before);
  await page.evaluate(() => { document.querySelector('#journal').hidden = false; });
  await page.clock.fastForward(5_000);
  await expect.poll(reads).toBe(before + 1);
});

test('closed execution evidence is rendered on demand and survives refresh', async ({ page }) => {
  await start(page);
  await page.evaluate(() => {
    window.journalData.entries = Array.from({ length: 200 }, (_, i) => ({
      id: `receipt-${i}`, kind: 'execution', actor: 'Builder', at: '2026-10-08T12:00:00Z',
      run_id: `run-${i}`, ticket: 'DEMO-1', preview: `### Run ${i}\n\nSummary ${i}`,
      content: `### Detailed evidence ${i}\n\n` + 'Retained evidence with exact source attribution.\n\n'.repeat(100),
    }));
    window.__renderedEvidence = 0;
    const render = window.xnautMarkdown.render;
    window.xnautMarkdown.render = text => {
      if (text.includes('Detailed evidence')) window.__renderedEvidence++;
      return render(text);
    };
    return window.journalInstance.refresh();
  });
  await page.getByRole('tab',{name:'Actions',exact:true}).click();
  await expect(page.locator('[data-worker-history]')).toHaveCount(200);
  expect(await page.evaluate(() => window.__renderedEvidence)).toBe(0);
  const first = page.locator('[data-worker-history]').first();
  await first.getByText('Recorded details and evidence', { exact: true }).click();
  await expect(first.locator('[data-worker-body]')).toContainText('Detailed evidence 0');
  expect(await page.evaluate(() => window.__renderedEvidence)).toBe(1);
  await page.evaluate(() => window.journalInstance.refresh());
  expect(await page.evaluate(() => window.__renderedEvidence)).toBe(1);
  await expect(first).toHaveAttribute('open');
});

async function start(page){
 await page.goto('/wiki-preview.html');
 await page.addScriptTag({url:'/js/project-journal.js'});
 await page.addStyleTag({url:'/css/project-journal.css'});
 await page.evaluate(()=>{
  const entry=(id,kind,content,ticket='DEMO-1')=>({id,kind,content,ticket,title:id,at:'2026-10-03T20:00:00Z',actor:kind==='note'?'André':'@Reviewer',thread_id:'thread-one',agent:'reviewer'});
  window.journalData={project:{key:'DEMO',name:'Backup review',purpose:'Review the existing system and preserve recovery evidence.'},path:'Development/journal/2026-10-03.md',documents:[{path:'Development/journal/2026-10-03.md'}],opening:'## Previous work\n\nInventory completed. Restore validation remains open.\n\n[Previous Journal](../../Development/journal/2026-10-02.md)\n\n[Design reference](https://example.com/design)',entries:[entry('one','decision','### Preserve existing archives\n\nAgreed by André. Retention changes need a separate review.'),entry('two','finding','### Missing validation\n\nThe script does not check the result.\n\n```js\n'+Array.from({length:18},(_,i)=>`check(${i});`).join('\n')+'\n```'),entry('three','note','### Your question\n\nCan we verify recovery before changing the schedule?','DEMO-2')],runs:[],observed_at:'2026-10-03T20:02:00Z',warning:''};
  window.journalCalls=[];window.__TAURI__={core:{invoke:async(name,args)=>{
   window.journalCalls.push({name,args});
   if(name==='project_journal_read'){if(args.project==='OTHER')return {...window.journalData,project:{key:'OTHER',name:'Other'},entries:[],opening:'Other project history',documents:[]};return structuredClone(window.journalData);}
   if(name==='project_journal_add'){if(window.failSave)throw Error('Ticket is outside this project');window.journalData.entries.push(entry('saved','note','### Your note\n\n'+args.request.content,args.request.ticket));return {ok:true};}
   if(name==='project_wiki_source')return {title:'Run receipt',text:'Recorded state'};
  }}};
  window.xnautOpenAgentSpace=(...args)=>{window.openedConversation=args;};
  document.body.innerHTML='<div id="journal" style="height:900px;width:660px"></div>';
  window.journalInstance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',path=>{window.openedWiki=path;});
 });
 await expect(page.locator('[data-title]')).toHaveText('Backup review · Live Journal');
 await page.getByRole('tab',{name:'Notes',exact:true}).click();
}
test('working document shows history, highlighted decisions, human notes and expandable code',async({page})=>{await start(page);await expect(page.locator('[data-opening]')).toContainText('Restore validation remains open');await expect(page.locator('.pj-decision')).toContainText('Agreed by André');await expect(page.locator('.pj-note')).toContainText('André');await expect(page.locator('.pj-body details')).not.toHaveAttribute('open');await page.locator('.pj-body summary').click();await expect(page.locator('pre')).toContainText('check(17)');await page.locator('[data-wiki]').click();expect(await page.evaluate(()=>window.openedWiki)).toBe('Development/journal/2026-10-03.md');});
test('live refresh retains expanded code, reading position and note draft',async({page})=>{await page.clock.install();await start(page);await page.locator('.pj-body summary').click();await page.getByText('Add your note or question',{exact:true}).click();await page.locator('[data-note]').fill('Keep my draft');await page.locator('.pj-scroll').evaluate(e=>{e.scrollTop=170;});const scroll=await page.locator('.pj-scroll').evaluate(e=>e.scrollTop);await page.evaluate(()=>window.journalData.entries.push({...window.journalData.entries[0],id:'four',kind:'fix',content:'### Fix recorded\n\nPreserve the previous key.'}));await page.clock.fastForward(5500);await expect(page.locator('.pj-fix')).toContainText('Preserve the previous key');await expect(page.locator('.pj-body details')).toHaveAttribute('open');await expect(page.locator('[data-note]')).toHaveValue('Keep my draft');expect(await page.locator('.pj-scroll').evaluate(e=>e.scrollTop)).toBe(scroll);});
test('project switching isolates entries and preserves drafts',async({page})=>{await start(page);await page.getByText('Add your note or question',{exact:true}).click();await page.locator('[data-note]').fill('Project A draft');await page.evaluate(()=>window.xnautJournal.mount(document.querySelector('#journal'),'OTHER',()=>{}));await expect(page.locator('[data-title]')).toHaveText('Other · Live Journal');await expect(page.locator('[data-opening]')).toHaveText('Other project history');await expect(page.locator('.pj-entry')).toHaveCount(0);await expect(page.locator('[data-note]')).toHaveValue('');await page.evaluate(()=>window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',()=>{}));await expect(page.locator('[data-note]')).toHaveValue('Project A draft');});
test('failed note save retains input and valid save appears with attribution',async({page})=>{await start(page);await page.getByText('Add your note or question',{exact:true}).click();await page.locator('[data-ticket]').fill('OTHER-1');await page.locator('[data-note]').fill('Verify the restore before deletion');await page.evaluate(()=>{window.failSave=true;});await page.getByRole('button',{name:'Save to Journal'}).click();await expect(page.locator('[data-status]')).toContainText('Ticket is outside');await expect(page.locator('[data-note]')).toHaveValue('Verify the restore before deletion');await page.evaluate(()=>{window.failSave=false;});await page.locator('[data-ticket]').fill('DEMO-1');await page.getByRole('button',{name:'Save to Journal'}).click();await expect(page.locator('[data-id="saved"]')).toContainText('Verify the restore before deletion');await expect(page.locator('[data-note]')).toHaveValue('');});
test('workstream filtering and source links keep exact identity',async({page})=>{await start(page);await page.getByLabel('Journal workstream').selectOption('DEMO-2');await expect(page.locator('.pj-entry')).toHaveCount(1);await page.getByRole('button',{name:'Source conversation ↗'}).click();expect(await page.evaluate(()=>window.openedConversation)).toEqual(['reviewer','thread-one']);});
test('untrusted content cannot execute and narrow panes do not overflow',async({page})=>{await start(page);await page.evaluate(()=>{window.journalData.entries.push({...window.journalData.entries[0],id:'unsafe',kind:'finding',content:'<script>window.pwned=true</script>\n<img src=x onerror="window.pwned=true">\n[bad](javascript:alert(1))'});return window.journalInstance.refresh();});expect(await page.evaluate(()=>window.pwned)).toBeUndefined();await expect(page.locator('.pj script,.pj [onerror],.pj a[href^="javascript:"]')).toHaveCount(0);await page.locator('#journal').evaluate(e=>{e.style.width='420px';});expect(await page.locator('.pj-scroll').evaluate(e=>e.scrollWidth<=e.clientWidth)).toBeTruthy();});
test('slow polling does not overlap requests',async({page})=>{await page.clock.install();await start(page);await page.evaluate(()=>{const invoke=window.__TAURI__.core.invoke;window.pendingCount=0;window.__TAURI__.core.invoke=(name,args)=>{if(name==='project_journal_read'){window.pendingCount++;return new Promise(resolve=>{window.finishRead=async()=>resolve(await invoke(name,args));});}return invoke(name,args);};});await page.clock.fastForward(21000);expect(await page.evaluate(()=>window.pendingCount)).toBe(1);await page.evaluate(()=>window.finishRead());});

test('saved context links open the exact Wiki page offline',async({page})=>{await start(page);await page.getByText('Saved context & previous journals',{exact:true}).click();await page.locator('[data-opening]').getByRole('button',{name:'Previous Journal'}).click();expect(await page.evaluate(()=>window.openedWiki)).toBe('Development/journal/2026-10-02.md');});

test('run details are folded behind a readable review summary',async({page})=>{await start(page);await page.evaluate(()=>{window.journalData.entries.push({...window.journalData.entries[0],id:'receipt',run_id:'run-1',preview:'### Backup review\n\nReview: changes requested. The diff is missing.',content:'### Full record\n\n```json\n{"input_hash":"technical-evidence"}\n```'});return window.journalInstance.refresh();});await expect(page.locator('[data-id="receipt"]')).toContainText('Review: changes requested');await expect(page.locator('[data-id="receipt"] pre')).not.toBeVisible();await page.getByText('Execution details and evidence',{exact:true}).click();await expect(page.locator('[data-id="receipt"] pre')).toContainText('technical-evidence');});

async function currentWork(page) {
 await page.evaluate(()=>{
  window.journalData.continuity={project:'DEMO',observed_at:1791230520000,diagnostics:[],tickets:[
   {id:'DEMO-1',title:'Verify restore',status:'in_progress',owner:'reviewer',state:'blocked',assignment_ids:['run-1'],next_action:'Inspect the missing restore proof.',evidence:[{kind:'ticket',source:'projects/DEMO/tickets/DEMO-1.json',detail:'Status in_progress, updated 2026-10-03T20:00:00Z'}]},
   {id:'DEMO-3',title:'Assigned without a worker',status:'in_progress',owner:'codex',state:'unknown',assignment_ids:[],next_action:'Inspect or recover the assignment before dispatch.',evidence:[]}
  ],assignments:[{run_id:'run-1',ticket:'DEMO-1',owner:'reviewer',branch:'fix/restore',worktree:'/isolated/restore',run_state:'done',last_commit:'abc123',pr_url:'https://example.com/pull/1',review_state:'changes_requested',state:'blocked',next_action:'Supply independent evidence.',evidence:[{kind:'handback',source:'run-1/handback.json',detail:'Agent reports all checks passed. Submitted 2026-10-03T19:00:00Z'},{kind:'review',source:'run-1/review.json',detail:'Changes requested: missing restore proof.'}]}]};
  return window.journalInstance.refresh();
 });
}

test('Tickets includes ticket-only assignments, blockers and attributed claims',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await expect(page.locator('[data-continuity]')).toContainText('Verify restore');
 await expect(page.locator('[data-continuity]')).toContainText('Owner: codex');
 await expect(page.locator('[data-continuity]')).toContainText('Inspect the missing restore proof.');
 await page.locator('[data-continuity-ticket="DEMO-1"] summary').click();
 await expect(page.locator('[data-continuity]')).toContainText('Agent claim');
 await expect(page.locator('[data-continuity]')).toContainText('Submitted 2026-10-03T19:00:00Z');
 await expect(page.locator('[data-continuity]')).toContainText('Source: run-1/handback.json');
 await expect(page.locator('[data-continuity]')).not.toContainText('Verification recorded');
 await page.locator('[data-continuity] [data-continuity-run="run-1"]').click();
 await expect(page.locator('dialog')).toContainText('Recorded state');
 const source=await page.evaluate(()=>window.journalCalls.find(c=>c.name==='project_wiki_source'));
 expect(source.args).toEqual({project:'DEMO',kind:'run',id:'run-1'});
});

test('snapshot-only changes refresh and reopen while preserving historical notes and drafts',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await page.locator('[data-continuity-ticket="DEMO-1"] summary').click();
 await page.getByRole('tab',{name:'Notes',exact:true}).click();
 await page.getByText('Add your note or question',{exact:true}).click();await page.locator('[data-note]').fill('Keep my recovery question');
 const opening=await page.locator('[data-opening]').textContent();
 await page.evaluate(()=>{
  const ticket=window.journalData.continuity.tickets[0];ticket.state='review';ticket.next_action='Review the independently saved restore result.';
  window.journalData.continuity.assignments[0].evidence.push({kind:'verification',source:'run-1/verification.json',detail:'Restore checked against abc123 at 2026-10-05T20:00:00Z'});
  return window.journalInstance.refresh();
 });
 await expect(page.locator('[data-continuity-ticket="DEMO-1"]')).toContainText('Review needed');
 await expect(page.locator('[data-continuity-ticket="DEMO-1"] details')).toHaveAttribute('open');
 await expect(page.locator('[data-continuity]')).toContainText('Verification evidence');
 expect(await page.locator('[data-opening]').textContent()).toBe(opening);
 await expect(page.locator('[data-note]')).toHaveValue('Keep my recovery question');
 await page.evaluate(()=>{window.journalInstance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',()=>{});});
 await expect(page.locator('[data-continuity]')).toContainText('Review the independently saved restore result.');
 await expect(page.locator('[data-id="three"]')).toContainText('Can we verify recovery');
 await expect(page.locator('[data-note]')).toHaveValue('Keep my recovery question');
});

test('historical Journal date keeps current state explicitly separate and workstream filter includes assignments',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await page.getByLabel('Journal date').selectOption('Development/journal/2026-10-03.md');
 await expect(page.locator('[data-continuity-time]')).toContainText('independent of the selected Journal date');
 await expect(page.locator('[data-history-date]')).toContainText('2026-10-03');
 await page.getByLabel('Journal workstream').selectOption('DEMO-3');
 await expect(page.locator('[data-continuity-ticket]')).toHaveCount(1);
 await expect(page.locator('[data-continuity]')).toContainText('Assigned without a worker');
});

test('foreign project snapshots are excluded from both summary and filter',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await page.evaluate(()=>{window.journalInstance=window.xnautJournal.mount(document.querySelector('#journal'),'OTHER',()=>{});});
 await expect(page.locator('[data-title]')).toHaveText('Other · Live Journal');
 await expect(page.locator('[data-continuity]')).not.toContainText('Verify restore');
 await expect(page.locator('[data-filter] option')).toHaveCount(1);
 await expect(page.locator('[data-continuity-time]')).toContainText('unavailable');
});

test('unavailable, incomplete and failed refresh states cannot look like no current work',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await page.evaluate(()=>{window.journalData.continuity={project:'DEMO',observed_at:1791230520000,tickets:[],assignments:[],diagnostics:[{source:'runs/broken/manifest.json',message:'Run record is unreadable'}]};return window.journalInstance.refresh();});
 await expect(page.locator('[data-continuity]')).toContainText('Current state is incomplete.');
 await expect(page.locator('[data-continuity]')).toContainText('Work could not be established');
 await page.evaluate(()=>{window.journalData.continuity=null;window.journalData.continuity_error='Registry path unavailable';return window.journalInstance.refresh();});
 await expect(page.locator('[data-continuity]')).toContainText('Registry path unavailable');
 await expect(page.locator('[data-opening]')).toContainText('Inventory completed');
 await page.evaluate(()=>{window.__TAURI__.core.invoke=async()=>{throw Error('read failed')};return window.journalInstance.refresh();});
 await expect(page.locator('[data-continuity-time]')).toContainText('last successful read');
 await expect(page.locator('[data-status]')).toContainText('Refresh failed');
});

test('outcomes and orphaned run evidence remain inspectable, escaped and scoped',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await page.evaluate(()=>{
  const t=window.journalData.continuity.tickets[0];t.state='verified';t.status='done';t.title='<img src=x onerror="window.pwned=true">';
  window.journalData.continuity.assignments.push({run_id:'orphan',ticket:null,owner:'codex',state:'stalled',next_action:'Inspect missing ticket reference',evidence:[{kind:'handback',source:'<script>window.pwned=true</script>',detail:'<img src=x onerror="window.pwned=true">'}]});
  return window.journalInstance.refresh();
 });
 await expect(page.locator('[data-continuity-detail="other"]')).not.toHaveAttribute('open');
 await page.locator('[data-continuity-detail="other"] > summary').click();
 await expect(page.locator('[data-continuity-ticket="DEMO-1"]')).toContainText('Verification recorded');
 await page.locator('[data-continuity-detail="unlinked"] > summary').click();
 await expect(page.locator('[data-continuity-detail="unlinked"]')).toContainText('Inspect missing ticket reference');
 expect(await page.evaluate(()=>window.pwned)).toBeUndefined();
 await expect(page.locator('[data-continuity] script,[data-continuity] img')).toHaveCount(0);
 await page.locator('#journal').evaluate(e=>{e.style.width='420px';});
 expect(await page.locator('.pj-scroll').evaluate(e=>e.scrollWidth<=e.clientWidth)).toBeTruthy();
});

test('date changes during a slow read queue the selected document without showing stale context',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await page.evaluate(()=>{
  const invoke=window.__TAURI__.core.invoke;let first=true;
  window.__TAURI__.core.invoke=async(name,args)=>{
   if(name==='project_journal_read'){
    if(first){first=false;await new Promise(resolve=>{window.releaseJournalRead=resolve;});}
    const data=await invoke(name,args);
    return {...data,path:args.path || data.path,opening:args.path?'Selected historical context':'Stale current context'};
   }
   return invoke(name,args);
  };
  void window.journalInstance.refresh();
 });
 await page.getByLabel('Journal date').selectOption('Development/journal/2026-10-03.md');
 await page.evaluate(()=>window.releaseJournalRead());
 await expect(page.locator('[data-opening]')).toHaveText('Selected historical context');
 await expect(page.locator('[data-continuity-time]')).toContainText('independent of the selected Journal date');
});

test('recent project work precedes older reviews while live workers stay first',async({page})=>{
 await start(page);await currentWork(page);await page.getByRole('tab',{name:'Tickets',exact:true}).click();
 await page.evaluate(()=>{
  const sample=window.journalData.continuity.tickets[0];
  window.journalData.continuity.tickets=[
   {...sample,id:'DEMO-OLD',state:'review',status:'done',updated_at:'2025-01-01T00:00:00Z'},
   {...sample,id:'DEMO-RECENT',state:'unknown',status:'in_progress',updated_at:'2026-10-06T00:00:00Z'},
   {...sample,id:'DEMO-LIVE',state:'active',updated_at:'2026-10-05T00:00:00Z'}
  ];
  return window.journalInstance.refresh();
 });
 expect(await page.locator('[data-continuity-ticket]').evaluateAll(rows=>rows.map(row=>row.dataset.continuityTicket))).toEqual(['DEMO-LIVE','DEMO-RECENT','DEMO-OLD']);
 await expect(page.locator('[data-continuity]')).toContainText('3 recorded work items needing attention');
});

test('review and repair history keeps source attribution and evidence across refresh and reopen', async ({page}) => {
 await start(page);
 await page.evaluate(() => {
  window.journalData.entries = [
   {id:'activity:review',kind:'finding',title:'DEMO-1 · findings',ticket:'DEMO-1',run_id:'review-run',actor:'xNAUT coordinator',at:'2026-10-03T20:00:00Z',preview:'### Review finding\n\nParser loses the last item. Revision: `abc`.',content:'### Review finding\n\nParser loses the last item.\n\n[Source evidence](../../Development/evidence/journal/review.md)'},
   {id:'activity:repair',kind:'fix',title:'DEMO-1 · repair published',ticket:'DEMO-1',run_id:'repair-run',actor:'xNAUT coordinator',at:'2026-10-03T20:05:00Z',preview:'### Fix recorded\n\nPreserved the last item. Revision: `def`.',content:'### Fix recorded\n\nPreserved the last item.\n\n[Source evidence](../../Development/evidence/journal/repair.md)'},
  ];
 });
 await page.getByRole('button',{name:'Refresh',exact:true}).click();
 await page.getByRole('tab',{name:'Actions',exact:true}).click();
 const finding=page.locator('.pj-action').filter({has:page.locator('[data-worker-history="activity:review"]')});
 await expect(finding).toContainText('xNAUT coordinator');
 await finding.getByText('Recorded details and evidence',{exact:true}).click();
 await expect(finding).toContainText('Parser loses the last item');
 await finding.getByRole('button',{name:'Execution record ↗'}).click();
 expect(await page.evaluate(()=>window.journalCalls.filter(c=>c.name==='project_wiki_source').at(-1).args)).toEqual({project:'DEMO',kind:'run',id:'review-run'});
 await page.getByRole('button',{name:'Close',exact:true}).click();
 await finding.getByRole('button',{name:'Source evidence'}).click();
 expect(await page.evaluate(()=>window.openedWiki)).toBe('Development/evidence/journal/review.md');
 await page.getByRole('button',{name:'Refresh',exact:true}).click();
 await expect(finding.locator('details')).toHaveAttribute('open');
 await expect(page.locator('[data-worker-history]')).toHaveCount(2);
 await expect(page.locator('[data-entries] .pj-entry')).toHaveCount(0);
 await page.evaluate(()=>window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',p=>{window.openedWiki=p;}));
 await expect(page.locator('[data-worker-history]')).toHaveCount(2);
 await page.locator('[data-worker-history="activity:repair"] summary').click();
 await expect(page.locator('[data-worker-history="activity:repair"]')).toContainText('Preserved the last item');
});

async function approvedGroups(page) {
 await page.evaluate(()=>{
  const group=project=>({id:project+'-approved',project,scope:'private scope hash do not show',approved_at:1791288000000,stopped_at:null,counts:{queued:1,running:1,blocked:1,verified:0},members:[{ticket:project+'-10',state:'queued',reason:'Waiting for capacity'},{ticket:project+'-11',state:'tracking',run_id:'worker-one',reason:'Current task retained'},{ticket:project+'-12',state:'blocked',reason:'Inspect missing evidence'}]});
  window.journalData.groups=[group('DEMO'),group('OTHER'),{...group('DEMO'),id:'unapproved',approved_at:null}];
  const invoke=window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke=async(name,args)=>{
   if(name==='swarm_plan_stop'){
    window.journalCalls.push({name,args});
    if(window.delayGroupStop)await new Promise(resolve=>{window.releaseGroupStop=resolve;});
    if(window.failGroupStop)throw Error('Coordinator store is busy');
    const g=window.journalData.groups.find(g=>g.id===args.planId);
    g.stopped_at=1791288060000;
    return {plan:{id:g.id,project:g.project},stopped_at:g.stopped_at,members:g.members};
   }
   return invoke(name,args);
  };
  return window.journalInstance.refresh();
 });
}

test('approved groups show queued work and scoped controls without internal approval scope',async({page})=>{
 await start(page);await approvedGroups(page);await page.getByRole('tab',{name:'Actions',exact:true}).click();
 await expect(page.locator('[data-group]')).toHaveCount(1);
 await expect(page.locator('[data-groups]')).toContainText('1 queued · 1 active · 1 blocked');
 await expect(page.locator('[data-groups]')).toContainText('Active workers retain their current work.');
 await expect(page.getByRole('button',{name:'Stop further dispatch',exact:true})).toHaveCount(1);
 await expect(page.locator('[data-groups]')).not.toContainText('OTHER');
 await expect(page.locator('[data-groups]')).not.toContainText('private scope hash');
 await page.getByLabel('Journal workstream').selectOption('DEMO-10');
 await expect(page.locator('[data-group]')).toHaveCount(1);
 await page.locator('[data-group] summary').click();
 await expect(page.locator('[data-group]')).toContainText('Waiting for capacity');
 await page.locator('#journal').evaluate(e=>{e.style.width='420px';});
 expect(await page.locator('.pj-scroll').evaluate(e=>e.scrollWidth<=e.clientWidth)).toBeTruthy();
});

test('group stop uses the native command once and survives polling, reopen and project switching',async({page})=>{
 await start(page);await approvedGroups(page);await page.getByRole('tab',{name:'Actions',exact:true}).click();
 await page.evaluate(()=>{window.delayGroupStop=true;});
 await page.getByRole('button',{name:'Stop further dispatch',exact:true}).click();
 await expect(page.getByRole('button',{name:'Stopping…',exact:true})).toBeDisabled();
 await page.evaluate(()=>{document.querySelector('[data-stop-group]').dispatchEvent(new MouseEvent('click'));return window.journalInstance.refresh();});
 await page.evaluate(()=>{window.journalInstance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',()=>{});});
 await expect(page.getByRole('button',{name:'Stopping…',exact:true})).toBeDisabled();
 expect(await page.evaluate(()=>window.journalCalls.filter(c=>c.name==='swarm_plan_stop'))).toEqual([{name:'swarm_plan_stop',args:{planId:'DEMO-approved'}}]);
 await page.evaluate(()=>window.releaseGroupStop());
 await page.getByRole('button',{name:'Refresh',exact:true}).click();
 await expect(page.locator('[data-groups]')).toContainText('Further dispatch stopped');
 await expect(page.locator('[data-stop-group]')).toHaveCount(0);
 await page.evaluate(()=>{window.journalInstance=window.xnautJournal.mount(document.querySelector('#journal'),'OTHER',()=>{});});
 await expect(page.locator('[data-title]')).toHaveText('Other · Live Journal');
 await expect(page.locator('[data-group]')).toHaveAttribute('data-group','OTHER-approved');
 await expect(page.getByRole('button',{name:'Stop further dispatch',exact:true})).toBeEnabled();
 await expect(page.locator('[data-groups]')).not.toContainText('DEMO-');
 await page.evaluate(()=>{window.journalInstance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',()=>{});});
 await expect(page.locator('[data-groups]')).toContainText('Further dispatch stopped');
 await expect(page.locator('[data-stop-group]')).toHaveCount(0);
 expect(await page.evaluate(()=>window.journalCalls.filter(c=>c.name==='swarm_plan_stop').length)).toBe(1);
});

test('failed group stop stays retryable and does not claim dispatch stopped',async({page})=>{
 await start(page);await approvedGroups(page);await page.getByRole('tab',{name:'Actions',exact:true}).click();
 await page.evaluate(()=>{window.failGroupStop=true;});
 await page.getByRole('button',{name:'Stop further dispatch',exact:true}).click();
 await expect(page.locator('[data-group] [role="status"]')).toContainText('Coordinator store is busy');
 await expect(page.getByRole('button',{name:'Stop further dispatch',exact:true})).toBeEnabled();
 await page.getByRole('button',{name:'Refresh',exact:true}).click();
 await expect(page.locator('[data-group]')).not.toContainText('Further dispatch stopped');
 await page.evaluate(()=>{window.failGroupStop=false;});
 await page.getByRole('button',{name:'Stop further dispatch',exact:true}).click();
 await expect(page.locator('[data-groups]')).toContainText('Further dispatch stopped');
 expect(await page.evaluate(()=>window.journalCalls.filter(c=>c.name==='swarm_plan_stop').length)).toBe(2);
});

test('group source failure is visible and offers no guessed stop controls',async({page})=>{
 await start(page);
 await page.evaluate(()=>{window.journalData.groups=[];window.journalData.groups_error='Approved groups are unavailable; refresh to retry.';return window.journalInstance.refresh();});
 await expect(page.locator('[data-groups]')).toContainText('Approved groups are unavailable');
 await expect(page.locator('[data-stop-group]')).toHaveCount(0);
 await expect(page.locator('[data-opening]')).toContainText('Inventory completed');
});
