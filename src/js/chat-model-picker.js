// Model choices use discovered IDs. Cloud inference stays on NautGate; local
// providers retain their own endpoint. Never infer billing from a model name.
(() => {
  'use strict';
  const routes = [
    ['nautgate','NautGate · all models'],['openrouter','OpenRouter · via NautGate'],
    ['codex','Codex / OpenAI · via NautGate'],['claude','Claude · via NautGate'],
    ['lmstudio','Local · LM Studio'],['ollama','Local · Ollama'],['custom','Custom endpoint'],
  ];
  function classify(model) {
    if (model.provider==='lmstudio'||model.provider==='ollama') return model.provider;
    if (model.provider!=='nautgate') return 'custom';
    if (model.id.startsWith('openrouter/')) return 'openrouter';
    if (/^(gpt-|o[134](?:-|$)|codex)/.test(model.id)) return 'codex';
    if (/^(claude-|anthropic\/)/.test(model.id)) return 'claude';
    return 'nautgate';
  }
  const choices = () => (window.xnautModelCatalog?.all()||[]).filter(m=>['nautgate','lmstudio','ollama'].includes(m.provider));
  function option(select,value,label) {const o=document.createElement('option');o.value=value;o.textContent=label;select.appendChild(o);return o;}
  function mountSettings(host,settings) {
    const endpoint=host.querySelector('#tm-llm-endpoint'),model=host.querySelector('#tm-llm-model'),key=host.querySelector('#tm-llm-key');
    let provider=settings.llm?.provider||'custom';
    const group=endpoint.closest('.settings-group');
    const row=document.createElement('div');row.className='settings-row';row.innerHTML='<label for="tm-llm-route">Provider / account route</label><select id="tm-llm-route" aria-label="Chat provider route"></select>';
    group.prepend(row);const route=row.querySelector('select');routes.forEach(([value,label])=>option(route,value,label));
    route.value=classify({id:model.value,provider});
    const pickerRow=document.createElement('div');pickerRow.className='settings-row';pickerRow.innerHTML='<label for="tm-llm-picker">Available models</label><select id="tm-llm-picker" aria-label="Available chat models"></select><button class="btn-test" type="button" data-refresh-chat-models>Refresh models</button>';
    row.after(pickerRow);const picker=pickerRow.querySelector('select');
    const hint=document.createElement('p');hint.style.cssText='color:var(--text-secondary);font-size:12px;line-height:1.5';hint.textContent='Codex / OpenAI and Claude models use your configured NautGate routes. Subscription coverage and account availability are controlled by the gateway, not the model name. OpenRouter uses API billing. Pick any discovered model, or enter an exact model ID below.';pickerRow.after(hint);
    function paint() {
      const selected=model.value;picker.replaceChildren();option(picker,'','Choose a model…');
      const list=choices().filter(m=>route.value==='nautgate'?m.provider==='nautgate':classify(m)===route.value);
      for(const m of list) option(picker,m.id,m.name||m.id);
      if(selected&&!list.some(m=>m.id===selected)) option(picker,selected,`${selected} (current / custom)`);
      picker.value=selected;
      if(!list.length) hint.textContent='No model list returned for this route. Refresh or enter an exact model ID. Inference still uses the chosen endpoint.';
    }
    route.onchange=()=>{
      provider=['openrouter','codex','claude','nautgate'].includes(route.value)?'nautgate':route.value;
      if(provider!=='custom') {
        const p=(settings.llm_providers||[]).find(p=>p.name===provider);
        if(p){endpoint.value=p.endpoint||'';key.value=p.api_key||'';}
        else if(settings.llm?.provider===provider){endpoint.value=settings.llm.endpoint||'';key.value=settings.llm.api_key||'';}
        else {endpoint.value=provider==='nautgate'?'http://localhost:8090/v1':provider==='ollama'?'http://localhost:11434/v1':'http://localhost:1238/v1';key.value='';}
      }
      model.value='';paint();
    };
    picker.onchange=()=>{if(picker.value)model.value=picker.value;};
    model.addEventListener('change',()=>{if(route.value==='openrouter'&&model.value&&!model.value.startsWith('openrouter/'))model.value='openrouter/'+model.value;paint();});
    pickerRow.querySelector('button').onclick=async event=>{event.target.disabled=true;try{await window.xnautModelCatalog.refresh();paint();}finally{event.target.disabled=false;}};
    paint();void window.xnautModelCatalog.refresh().then(()=>{if(host.isConnected)paint();});
    return {provider:()=>provider==='custom'?'custom':provider};
  }
  function mountChat(parent,{get,onChange}) {
    const select=document.createElement('select');select.className='chatp-model-select';select.setAttribute('aria-label','Chat model');select.title='Model and provider for this conversation';select.style.cssText='max-width:300px;font-size:11px;background:transparent;color:inherit;border:1px solid var(--border,#444);border-radius:5px;padding:4px;';parent.appendChild(select);
    function paint() {
      const current=get();select.replaceChildren();option(select,'','Workspace default');
      const list=choices();
      for(const [route,label] of routes.filter(r=>r[0]!=='custom')) {
        const entries=list.filter(m=>classify(m)===route);if(!entries.length)continue;
        const group=document.createElement('optgroup');group.label=label;
        for(const m of entries)option(group,JSON.stringify([m.provider,m.id]),m.name||m.id);
        select.appendChild(group);
      }
      const value=current.model?JSON.stringify([current.provider||'nautgate',current.model]):'';
      if(value&&![...select.options].some(o=>o.value===value))option(select,value,`${current.model} (current)`);
      select.value=value;
    }
    select.onchange=()=>{const [provider,model]=select.value?JSON.parse(select.value):['',''];onChange({provider,model});};
    paint();void window.xnautModelCatalog.refresh().then(()=>{if(parent.isConnected)paint();});
    return select;
  }
  window.xnautChatModelPicker={mountSettings,mountChat};
})();
