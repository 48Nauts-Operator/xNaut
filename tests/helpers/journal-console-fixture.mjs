import {expect} from '@playwright/test';
export async function startJournalConsole(page){
 await page.goto('/wiki-preview.html');
 await page.addScriptTag({url:'/js/project-journal.js'});
 await page.addScriptTag({url:'/js/agent-quick-pane.js'});
 await page.addStyleTag({url:'/css/project-journal.css'});
 await page.evaluate(()=>{
  const now=Date.now();
  window.calls=[];
  const workers=Array.from({length:5},(_,i)=>({run_id:'run-'+i,ticket:'DEMO-'+(i+1),agent:['claude','codex','pi','rudi','stark'][i],destination:'exe-dev',machine:'Tron',state:i===2?'blocked':'running',pty_session:'session-'+i,observed_at:now,signal:i===2?'Waiting for repository access':'Implementing the assigned ticket'}));
  window.data={project:{key:'DEMO',name:'NautGate',root:'/projects/demo'},path:'Development/journal/2026-10-10.md',documents:[{path:'Development/journal/2026-10-09.md'}],opening:'Earlier design notes',entries:[],runs:[],groups:[],observed_at:now,
    continuity:{project:'DEMO',observed_at:now,tickets:workers.map(r=>({id:r.ticket,title:'Implement routing',state:r.state==='blocked'?'blocked':'active',status:'in_progress',assignment_ids:[r.run_id],owner:r.agent,next_action:'Inspect the worker'})),assignments:[{run_id:'run-2',ticket:'DEMO-3',owner:'pi',state:'review',next_action:'Verify the submitted revision',pr_url:'https://example.com/3',evidence:[{kind:'handback',source:'run-2/handback.json',detail:'Agent claims implementation complete'}]}],diagnostics:[]},
    console:{project:'DEMO',deployed:workers,handoffs:[{title:'Routing handoff',path:'Development/handoffs/routing.md'}],activity:{entries:[{run_id:'run-2',ticket:'DEMO-3',agent:'pi',kind:'sweep_refused',at:new Date(now).toISOString(),detail:'Repository access failed',session:'not-a-pty'}],total:1,dates:['2026-10-09']}}};
  window.sessions=[{session_id:'session-0',status:'working',last_output_at_ms:now},{session_id:'foreign',agent_id:'pi',status:'working'}];
  window.__TAURI__={core:{invoke:async(name,args)=>{window.calls.push({name,args});
   if(name==='project_journal_read')return {...structuredClone(window.data),path:args.path||window.data.path};
   if(name==='agent_sessions_list'){if(window.failSessions)throw Error('failed');return window.sessions;}
   if(name==='project_wiki_source'){if(window.failSource)throw Error('Run record is unavailable');return {title:'Run record',text:'Exact run '+args.id};}
   if(name==='exe_machines')return [{vm_name:'testbox',status:'running',ssh_dest:'testbox.exe.xyz',https_url:'https://testbox.exe.xyz'}];
   if(name==='sandbox_verify_records')return [{id:'v1',project:'DEMO',ticket_id:'DEMO-3',status:'passed',commit_sha:'abc123',steps:[{name:'unit',exit_code:0}]},{id:'foreign',project:'OTHER',ticket_id:'OTHER-1',status:'failed'}];
   if(name==='create_command_session')return {session_id:'new-ssh'};
  }}};
  window.xnautOpenAgentSession=(...args)=>{window.openedSession=args;};
  window.xnautOpenDelivery=args=>{window.openedTicket=args;};
  window.xnautOpenMesh=()=>{window.meshOpened=true;};
  window.xnautOpenAgentSettings=agent=>{window.openedSettings=agent;};
  window.xnautNewBrowserTab=url=>{window.openedWeb=url;};
  window.xnautAttachAgentTab=(...args)=>{window.attached=args;};
  document.body.innerHTML='<div id="journal" style="height:960px;width:740px"></div>';
  window.instance=window.xnautJournal.mount(document.querySelector('#journal'),'DEMO',path=>{window.openedWiki=path;});
 });
 await expect(page.locator('[data-title]')).toHaveText('NautGate · Live Journal');
}
