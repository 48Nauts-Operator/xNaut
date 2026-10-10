// XNAUT-489: compatibility entry points and on-demand computer tools.
// Project activity and deployed workers live in the Journal. This drawer is
// deliberately account-wide for machines; it never pretends those are workers.
(function () {
  'use strict';
  const invoke=(...a)=>window.__TAURI__.core.invoke(...a);
  const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  let selected=null;
  let drawer=null;
  const artifacts=new Map();
  const web=url=>{if(!/^https?:\/\//i.test(url))throw Error('Invalid web address');window.xnautNewBrowserTab(url);};
  async function shell(destination){
    // Host names are data, never an interpolated shell program.
    if(!/^(?:[a-zA-Z0-9_.-]+@)?[a-zA-Z0-9][a-zA-Z0-9.-]*$/.test(destination))throw Error('Invalid SSH destination');
    const result=await invoke('create_command_session',{config:{program:'ssh',args:['-t','-o','StrictHostKeyChecking=accept-new',destination],workingDir:'~/',env:{}}});
    window.xnautAttachAgentTab(result.session_id,'sandbox · '+destination);
  }
  window.xnautOpenAgentTools=async project=>{
    drawer?.remove();
    const dialog=document.createElement('dialog');drawer=dialog;dialog.className='pj-dialog pj-tools-dialog';
    dialog.innerHTML='<button data-close>Close</button><h2>Computers & verification</h2><p data-tool-error role="status"></p><div data-tools-content>Loading…</div>';
    document.body.append(dialog);dialog.querySelector('[data-close]').onclick=()=>dialog.remove();dialog.addEventListener('close',()=>dialog.remove());dialog.showModal();
    const [machines,records]=await Promise.allSettled([invoke('exe_machines'),invoke('sandbox_verify_records')]);
    if(!dialog.isConnected)return;
    const rows=records.status==='fulfilled'?(records.value||[]).filter(r=>project&&(r.project===project.key||r.project===project.root)):[];
    const vms=machines.status==='fulfilled'?(machines.value||[]):[];
    const content=dialog.querySelector('[data-tools-content]');
    content.innerHTML=`${selected?`<p>@${esc(selected.handle)} <button data-settings>Agent settings</button>${artifacts.has(selected.handle)?'<button data-artifact>Latest artifact ↗</button>':''}</p>`:''}<h3>exe.dev computers · whole account</h3><p>These machines are account resources. Project workers appear in the Journal.</p>${machines.status==='rejected'?'<p>Computers could not be refreshed.</p>':vms.length?vms.map(vm=>`<div class="pj-tool-row"><strong>${esc(vm.vm_name)}</strong> · ${esc(vm.status)}<small>${esc(vm.ssh_dest || vm.vm_name+'.exe.xyz')}</small><button data-shell="${esc(vm.ssh_dest || vm.vm_name+'.exe.xyz')}">Terminal</button>${vm.https_url?`<button data-web="${esc(vm.https_url)}">Web ↗</button>`:''}</div>`).join(''):'<p>No computers found.</p>'}<h3>Verification · ${esc(project?.name || 'select a project')}</h3>${records.status==='rejected'?'<p>Verification records could not be refreshed.</p>':rows.length?rows.map(r=>`<details class="pj-tool-row"><summary>${esc(r.ticket_id || r.id)} · ${esc(r.status)}</summary><pre>${esc(JSON.stringify({status:r.status,commit:r.commit_sha,not_evidence:r.not_evidence,error:r.error,steps:r.steps,checks:r.checks},null,2))}</pre>${r.provider_kind==='exe-ssh'&&r.sandbox_id?`<button data-shell="${esc(r.sandbox_id+'.exe.xyz')}">Terminal</button>`:''}${r.public_url?`<button data-web="${esc(r.public_url)}">Open artifact ↗</button>`:''}</details>`).join(''):'<p>No verification records for this project.</p>'}`;
    const error=e=>{dialog.querySelector('[data-tool-error]').textContent=String(e);};
    content.querySelectorAll('[data-shell]').forEach(b=>{b.onclick=async()=>{b.disabled=true;try{await shell(b.dataset.shell);}catch(e){error(e);}finally{b.disabled=false;}};});
    content.querySelectorAll('[data-web]').forEach(b=>{b.onclick=()=>{try{web(b.dataset.web);}catch(e){error(e);}};});
    const settings=content.querySelector('[data-settings]');if(settings)settings.onclick=()=>window.xnautOpenAgentSettings(selected.handle);
    const artifact=content.querySelector('[data-artifact]');if(artifact)artifact.onclick=()=>{try{web(artifacts.get(selected.handle));}catch(e){error(e);}};
  };
  window.xnautAgentArtifactOpen=(agentId,url)=>{
    if(!agentId||!/^https?:\/\//i.test(url||''))return false;
    artifacts.set(agentId,url);
    // Native open requests should still produce a visible artifact.
    web(url);return true;
  };
  window.xnautRightPaneOpenAgent=(profile,options={})=>{
    selected=profile || null;
    const keepNotebook=options.preserveNotebook&&[...document.querySelectorAll('[data-page="wiki"], [data-page="journal"]')].some(n=>n.getClientRects().length&&!n.closest('[hidden]'));
    window.xnautEnsureRightPane?.();
    if(!keepNotebook)window.xnautOpenProjectJournal?.();
  };
})();
