// Model choices use discovered IDs. NautGate is mandatory while enabled;
// otherwise the configured direct endpoint owns inference and credentials.
(() => {
  'use strict';
  const routes = [
    ['nautgate','NautGate · all models'],['openrouter','OpenRouter · via NautGate'],
    ['codex','OpenAI · via NautGate'],['claude','Claude · via NautGate'],
    ['lmstudio','Local · LM Studio'],['ollama','Local · Ollama'],['custom','Custom endpoint'],
  ];
  function classify(model) {
    if (model.provider==='lmstudio'||model.provider==='ollama') return model.provider;
    if (model.provider==='openai') return 'codex';
    if (model.provider==='openrouter') return 'openrouter';
    if (model.provider!=='nautgate') return 'custom';
    if (model.id.startsWith('openrouter/')) return 'openrouter';
    if (/^(gpt-|o[134](?:-|$)|codex)/.test(model.id)) return 'codex';
    if (/^(claude-|anthropic\/)/.test(model.id)) return 'claude';
    return 'nautgate';
  }
  const choices = () => (window.xnautModelCatalog?.all()||[]);
  function watchCatalog(element, paint) {
    const update = () => {
      if (element.isConnected) paint();
      else window.removeEventListener('xnaut-model-catalog-update', update);
    };
    window.addEventListener('xnaut-model-catalog-update', update);
  }
  function option(select,value,label) {const o=document.createElement('option');o.value=value;o.textContent=label;select.appendChild(o);return o;}
  function mountSettings(host,settings) {
    const endpoint=host.querySelector('#tm-llm-endpoint'),model=host.querySelector('#tm-llm-model'),key=host.querySelector('#tm-llm-key');
    let provider=settings.llm?.provider||'custom';
    const gateway=(settings.llm_providers||[]).find(p=>p.name.toLowerCase()==='nautgate');
    let useGateway=gateway?!!gateway.enabled:provider==='nautgate';
    if(useGateway){
      provider='nautgate';
      if(gateway){
        const primary=settings.llm?.provider==='nautgate'?settings.llm:null;
        endpoint.value=gateway.endpoint||primary?.endpoint||'http://localhost:8090/v1';key.value=gateway.api_key||primary?.api_key||'';
      }
    }
    const group=endpoint.closest('.settings-group');
    const row=document.createElement('div');row.className='settings-row';row.innerHTML='<label for="tm-llm-route">Provider / account route</label><select id="tm-llm-route" aria-label="Chat provider route"></select>';
    group.prepend(row);const route=row.querySelector('select');
    const switchRow=document.createElement('div');switchRow.className='settings-row';
    switchRow.innerHTML='<label><input type="checkbox" id="tm-use-nautgate"> Route model requests through NautGate</label>';
    row.before(switchRow);const toggle=switchRow.querySelector('input');toggle.checked=useGateway;
    function routeOptions(){
      const before=route.value;route.replaceChildren();
      for(const [value,label] of routes){
        if(!useGateway&&['nautgate','claude'].includes(value))continue;
        option(route,value,useGateway?label.replace('Local ·','Via NautGate ·'):label.replace(' · via NautGate',' · direct API'));
      }
      if([...route.options].some(o=>o.value===before))route.value=before;
    }
    routeOptions();
    route.value=classify({id:model.value,provider});
    if(!route.value)route.value='codex';
    const pickerRow=document.createElement('div');pickerRow.className='settings-row';pickerRow.innerHTML='<label for="tm-llm-picker">Available models</label><select id="tm-llm-picker" aria-label="Available chat models"></select><button class="btn-test" type="button" data-refresh-chat-models>Refresh models</button>';
    row.after(pickerRow);const picker=pickerRow.querySelector('select');
    const catalogStatus=document.createElement('p');catalogStatus.dataset.modelCatalogStatus='';catalogStatus.style.cssText='color:var(--text-secondary);font-size:12px;line-height:1.5';pickerRow.after(catalogStatus);
    const hint=document.createElement('p');hint.style.cssText='color:var(--text-secondary);font-size:12px;line-height:1.5';hint.textContent='Codex / OpenAI and Claude models use your configured NautGate routes. Subscription coverage and account availability are controlled by the gateway, not the model name. OpenRouter uses API billing. Pick any discovered model, or enter an exact model ID below.';pickerRow.after(hint);
    function paint() {
      const selected=model.value;picker.replaceChildren();option(picker,'','Choose a model…');
      const list=choices().filter(m=>useGateway?m.provider==='nautgate'&&(route.value==='nautgate'||classify(m)===route.value):m.provider!=='nautgate'&&classify(m)===route.value);
      for(const m of list) option(picker,m.id,m.name||m.id);
      if(selected&&!list.some(m=>m.id===selected)) option(picker,selected,`${selected} (current / custom)`);
      picker.value=selected;
      const catalog=window.xnautModelCatalog,at=catalog?.at(),failure=catalog?.error?.();
      catalogStatus.textContent=`Provider catalog: ${at?'last updated '+new Date(at).toLocaleString():'not loaded yet'}. ${failure?'Refresh failed; showing the previous list. '+failure:'Refreshes daily while xNaut is running and checks again on wake.'}`;
      hint.textContent=useGateway?'Chat and agent model requests use NautGate. A failed gateway request will not fall back to a direct provider. Account and subscription availability are controlled by your gateway.':'xNaut calls the selected endpoint directly using its API key. OpenAI API billing is separate from a Codex subscription. Astra uses native Responses; the endpoint must support it.';
    }
    route.onchange=()=>{
      provider=useGateway?'nautgate':route.value==='codex'?'openai':route.value;
      if(provider!=='custom') {
        const p=(settings.llm_providers||[]).find(p=>p.name===provider);
        if(p){endpoint.value=p.endpoint||'';key.value=p.api_key||'';}
        else if(settings.llm?.provider===provider){endpoint.value=settings.llm.endpoint||'';key.value=settings.llm.api_key||'';}
        else {endpoint.value=({nautgate:'http://localhost:8090/v1',openai:'https://api.openai.com/v1',openrouter:'https://openrouter.ai/api/v1',ollama:'http://localhost:11434/v1',lmstudio:'http://localhost:1238/v1'})[provider]||'';key.value='';}
      }
      model.value='';paint();
    };
    toggle.onchange=()=>{useGateway=toggle.checked;routeOptions();route.onchange();};
    picker.onchange=()=>{if(picker.value)model.value=picker.value;};
    model.addEventListener('change',()=>{if(useGateway&&route.value==='openrouter'&&model.value&&!model.value.startsWith('openrouter/'))model.value='openrouter/'+model.value;paint();});
    pickerRow.querySelector('button').onclick=async event=>{event.target.disabled=true;try{await window.xnautModelCatalog.refresh();paint();}finally{event.target.disabled=false;}};
    watchCatalog(host,paint);
    paint();void window.xnautModelCatalog.refreshIfStale().then(()=>{if(host.isConnected)paint();});
    return {provider:()=>provider, gatewayEnabled:()=>useGateway};
  }
  function mountChat(parent,{get,onChange}) {
    const select=document.createElement('select');select.className='chatp-model-select';select.setAttribute('aria-label','Chat model');select.title='Model and provider for this conversation';select.style.cssText='max-width:300px;font-size:11px;background:transparent;color:inherit;border:1px solid var(--border,#444);border-radius:5px;padding:4px;';parent.appendChild(select);
    function paint() {
      const current=get();select.replaceChildren();option(select,'','Workspace default');
      const list=choices();
      for(const [route,label] of routes) {
        const entries=list.filter(m=>classify(m)===route);if(!entries.length)continue;
        const group=document.createElement('optgroup');group.label=entries.every(m=>m.provider!=='nautgate')?label.replace(' · via NautGate',' · direct'):label;
        for(const m of entries)option(group,JSON.stringify([m.provider,m.id]),m.name||m.id);
        select.appendChild(group);
      }
      const value=current.model?JSON.stringify([current.provider||'nautgate',current.model]):'';
      if(value&&![...select.options].some(o=>o.value===value))option(select,value,`${current.model} (current)`);
      select.value=value;
    }
    select.onchange=()=>{const [provider,model]=select.value?JSON.parse(select.value):['',''];onChange({provider,model});};
    watchCatalog(parent,paint);
    paint();void window.xnautModelCatalog.refreshIfStale().then(()=>{if(parent.isConnected)paint();});
    return select;
  }
  window.xnautChatModelPicker={mountSettings,mountChat};
})();
