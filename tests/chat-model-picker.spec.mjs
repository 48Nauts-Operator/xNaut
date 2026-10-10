import {test,expect} from '@playwright/test';

// Persist the fake backend independently from the legacy webview cache so a
// real page reload exercises startup reconciliation, not a direct helper call.
async function persistedSettings(page, durable, legacy) {
 await page.addInitScript(({legacy})=>{
  if (!localStorage.getItem('__settings_fixture_seeded')) {
   localStorage.setItem('xnaut-settings', JSON.stringify(legacy));
   localStorage.setItem('__settings_fixture_seeded', 'yes');
  }
 }, {legacy});
 await page.route('**/__stub.js', async route=>{
  const response=await route.fetch();
  await route.fulfill({response,body:await response.text()+`\n{
   window.__xnautStub.settings_get = JSON.parse(localStorage.getItem('__durable_settings_fixture')) || {...window.__xnautStub.settings_get, ...${JSON.stringify(durable)}};
   const invoke = window.__TAURI__.core.invoke;
   window.__TAURI__.core.invoke = async (name,args) => {
    const result = await invoke(name,args);
    if (name === 'settings_set') localStorage.setItem('__durable_settings_fixture', JSON.stringify(window.__xnautStub.settings_get));
    return result;
   };
  }`});
 });
 await page.goto('/?stub=1');
 await expect.poll(()=>page.evaluate(()=>window.xnautStartupHealth?.sealed())).toBe(true);
}

test('restart preserves native provider repairs despite conflicting legacy settings and explicit edits remain durable',async({page})=>{
 const durable={
  llm:{provider:'nautgate',endpoint:'https://gateway.fixture/v1',model:'claude-fixture',api_key:'native-token',harness_local:false},
  cloud_agent_model:{provider:'lmstudio',model:'default-qwen',worker_endpoint:'http://worker.fixture:1238'},
  llm_providers:[
   {name:'nautgate',endpoint:'https://gateway.fixture/v1',api_key:'native-token',enabled:true},
   {name:'lmstudio',endpoint:'http://desktop.fixture:1238/v1',api_key:null,enabled:false},
   {name:'ollama',endpoint:'http://ollama.fixture:11434/v1',api_key:null,enabled:false},
   {name:'openai',endpoint:'https://api.openai.com/v1',api_key:null,enabled:false},
  ]
 };
 await persistedSettings(page,durable,{nautgateUrl:'http://localhost:8090/v1',apiKeyNautGate:'stale-token',lmstudioUrl:'http://localhost:1234',llmProvider:'ollama',llmModel:'No models found',harnessLocal:true,apiKeyOpenAI:'revoked-token'});
 for (let n=0;n<2;n++) {
  expect(await page.evaluate(()=>window.__xnautStub.settings_get)).toMatchObject(durable);
  expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-settings')))).toMatchObject({nautgateUrl:'https://gateway.fixture/v1',apiKeyNautGate:'native-token',lmstudioUrl:'http://desktop.fixture:1238',apiKeyOpenAI:'',llmProvider:'nautgate',llmModel:'claude-fixture',harnessLocal:false});
  await page.reload();
  await expect.poll(()=>page.evaluate(()=>window.xnautStartupHealth?.sealed())).toBe(true);
 }
 await page.evaluate(()=>window.xnautOpenSettingsSection('ai'));
 await expect(page.locator('#set-nautgate-url')).toHaveValue('https://gateway.fixture/v1');
 await page.locator('#set-nautgate-url').fill('https://edited-gateway.fixture/v1');
 // The explicit save also works when discovery is unavailable: a user's
 // manually selected model must not turn into a discovery error placeholder.
 await page.locator('#set-default-model').evaluate(el=>{el.innerHTML='<option value="claude-fixture">Claude fixture</option>';el.value='claude-fixture';});
 await page.locator('#btn-save-ai').click();
 await expect.poll(()=>page.evaluate(()=>window.__xnautStub.settings_get.llm_providers.find(p=>p.name==='nautgate').endpoint)).toBe('https://edited-gateway.fixture/v1');
 await page.reload();
 await expect.poll(()=>page.evaluate(()=>window.xnautStartupHealth?.sealed())).toBe(true);
 const saved=await page.evaluate(()=>window.__xnautStub.settings_get);
 expect(saved.llm_providers.find(p=>p.name==='nautgate')).toMatchObject({endpoint:'https://edited-gateway.fixture/v1',api_key:'native-token',enabled:true});
 expect(saved.cloud_agent_model).toEqual(durable.cloud_agent_model);
 expect(saved.llm_providers.find(p=>p.name==='lmstudio').enabled).toBe(false);
});

test('first startup migrates a legacy connection once and later native edits survive reload',async({page})=>{
 await persistedSettings(page,{llm:{provider:'',model:'',endpoint:''},llm_providers:[]},{nautgateUrl:'https://legacy-gateway.fixture/v1',apiKeyNautGate:'legacy-token',llmProvider:'nautgate',llmModel:'legacy-model'});
 expect(await page.evaluate(()=>window.__xnautStub.settings_get.llm)).toMatchObject({provider:'nautgate',endpoint:'https://legacy-gateway.fixture/v1',model:'legacy-model'});
 await page.evaluate(async()=>{
  const current=await window.__TAURI__.core.invoke('settings_get');
  current.llm_providers.find(p=>p.name==='nautgate').endpoint='https://native-repair.fixture/v1';
  await window.__TAURI__.core.invoke('settings_set',{settings:current});
 });
 await page.reload();
 await expect.poll(()=>page.evaluate(()=>window.xnautStartupHealth?.sealed())).toBe(true);
 expect(await page.evaluate(()=>window.__xnautStub.settings_get.llm_providers.find(p=>p.name==='nautgate').endpoint)).toBe('https://native-repair.fixture/v1');
});
async function models(page){
 await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
 await page.evaluate(async()=>{
  window.__xnautStub.chat_list_provider_models=[{provider:'nautgate',model:'auto'},{provider:'nautgate',model:'gpt-5.6-sol'},{provider:'nautgate',model:'claude-fable-5'},{provider:'nautgate',model:'openrouter/google/gemini-3.7-flash'},{provider:'lmstudio',model:'local-qwen'}];
  await window.xnautModelCatalog.refresh();
 });
}
test('catalog refresh updates an open picker with display names and preserves selection',async({page})=>{
 await models(page);await page.evaluate(()=>window.xnautAttachChatTab({title:'Live catalog',chatKey:'live-catalog'}));
 const select=page.getByRole('combobox',{name:'Chat model',exact:true});
 await select.selectOption(JSON.stringify(['nautgate','claude-fable-5']));
 await page.evaluate(async()=>{
  window.__xnautStub.chat_list_provider_models=[{provider:'nautgate',model:'claude-fable-5',label:'Claude Fable 5'},{provider:'nautgate',model:'gpt-fixture-new',label:'Newly discovered model'}];
  await window.xnautModelCatalog.refresh();
 });
 await expect(select.locator('option',{hasText:'Newly discovered model'})).toHaveCount(1);
 await expect(select.locator('option',{hasText:'Claude Fable 5'})).toHaveCount(1);
 await expect(select).toHaveValue(JSON.stringify(['nautgate','claude-fable-5']));
 await expect(select.locator('option',{hasText:'gemini-3.7'})).toHaveCount(0);
});
test('catalog failure keeps the last successful list and timestamp',async({page})=>{
 await models(page);
 const before=await page.evaluate(()=>({at:window.xnautModelCatalog.at(),all:window.xnautModelCatalog.all()}));
 await page.evaluate(async()=>{
  const invoke=window.__TAURI__.core.invoke;
  window.__TAURI__.core.invoke=(name,...args)=>name==='chat_list_provider_models'?Promise.reject(new Error('Fixture provider unavailable')):invoke(name,...args);
  await window.xnautModelCatalog.refresh();
 });
 expect(await page.evaluate(()=>({at:window.xnautModelCatalog.at(),all:window.xnautModelCatalog.all()}))).toEqual(before);
 expect(await page.evaluate(()=>window.xnautModelCatalog.error())).toContain('Fixture provider unavailable');
});
test('legacy cloud settings use refreshed discovery instead of a baked-in model list',async({page})=>{
 await models(page);
 await page.evaluate(()=>{
  const provider=document.createElement('select');provider.id='set-default-provider';provider.innerHTML='<option value="openai">OpenAI</option>';
  const model=document.createElement('select');model.id='set-default-model';document.body.append(provider,model);
  window.updateModelDropdown();
 });
 const select=page.locator('#set-default-model');
 await expect(select.locator('option[value="gpt-5.6-sol"]')).toHaveCount(1);
 await expect(select.locator('option[value="gpt-3.5-turbo"]')).toHaveCount(0);
 await page.evaluate(async()=>{
  window.__xnautStub.chat_list_provider_models=[{provider:'nautgate',model:'gpt-fixture-new',label:'Fresh model label'}];
  await window.xnautModelCatalog.refresh();
 });
 await expect(select.locator('option[value="gpt-fixture-new"]')).toHaveText('Fresh model label');
});
test('a stale catalog checks again when the app returns to the foreground',async({page})=>{
 await page.addInitScript(()=>localStorage.setItem('xnaut-model-catalog',JSON.stringify({at:Date.now()-2*86400000,flat:[],byProvider:{}})));
 await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
 await page.evaluate(async()=>{
  await window.xnautModelCatalog.refresh();
  window.__xnautStub.chat_list_provider_models=[{provider:'nautgate',model:'wake-model',label:'Wake model'}];
  const now=Date.now;Date.now=()=>now()+2*86400000;
  window.dispatchEvent(new Event('focus'));
 });
 await expect.poll(()=>page.evaluate(()=>window.xnautModelCatalog.all().some(m=>m.id==='wake-model'))).toBe(true);
});
test('all gateway and local models are available and per-chat selection survives reopen',async({page})=>{
 await models(page);await page.evaluate(()=>window.xnautAttachChatTab({title:'Model test',chatKey:'model-test'}));
 const select=page.getByRole('combobox',{name:'Chat model',exact:true});
 await expect(select.locator('option')).toHaveCount(6);
 await select.selectOption(JSON.stringify(['nautgate','openrouter/google/gemini-3.7-flash']));
 expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-chat-model:model-test')))).toEqual({provider:'nautgate',model:'openrouter/google/gemini-3.7-flash'});
 await page.reload();await page.waitForSelector('#btn-help');await page.evaluate(()=>window.xnautAttachChatTab({title:'Model test',chatKey:'model-test'}));
 await expect(page.getByRole('combobox',{name:'Chat model',exact:true})).toHaveValue(JSON.stringify(['nautgate','openrouter/google/gemini-3.7-flash']));
});
test('automatic legacy sync cannot overwrite an explicit workspace chat choice',async({page})=>{
 await models(page);await page.evaluate(async()=>{
  window.__xnautStub.settings_get={...window.__xnautStub.settings_get,chat_model_source:'workspace',llm:{provider:'nautgate',endpoint:'http://gateway/v1',model:'claude-fable-5',api_key:'fixture-key'},unrelated:'keep'};
  await window.xnautSyncChatSettingsFromAiSettings();
 });
 expect(await page.evaluate(()=>window.__xnautStub.settings_get)).toMatchObject({chat_model_source:'workspace',llm:{provider:'nautgate',model:'claude-fable-5',api_key:'fixture-key'},unrelated:'keep'});
 expect(await page.evaluate(()=>window.__xnautStub.settings_get.llm_providers.find(p=>p.name==='nautgate'))).toMatchObject({endpoint:'http://gateway/v1',api_key:'fixture-key'});
});
test('agent chat choice stores its provider without changing the coding model',async({page})=>{
 await models(page);await page.getByRole('button',{name:'More surfaces'}).click();await page.getByText('Agent Space',{exact:true}).first().click();
 await page.locator('.asl-agent',{hasText:'Builder'}).first().click();await page.getByRole('button',{name:'Settings',exact:true}).click();
 const runtime=await page.locator('[name=model]').inputValue();const provider=await page.locator('[name=provider]').inputValue();
 await page.locator('[name=chat_model]').selectOption('openrouter/google/gemini-3.7-flash');
 await expect(page.locator('[name=chat_provider]')).toHaveValue('nautgate');await expect(page.locator('[name=model]')).toHaveValue(runtime);await expect(page.locator('[name=provider]')).toHaveValue(provider);
 await page.locator('form.as-form-page[data-form]').evaluate(form=>form.requestSubmit());
 await expect.poll(async()=>page.evaluate(()=>window.__xnautInvokes.filter(i=>i.cmd==='agent_profile_update').length)).toBeGreaterThan(0);
 const saved=await page.evaluate(()=>window.__xnautInvokes.findLast(i=>i.cmd==='agent_profile_update').args.profile);
 expect(saved).toMatchObject({chat_provider:'nautgate',chat_model:'openrouter/google/gemini-3.7-flash',model:runtime,provider});
});
test('Chat LLM settings save OpenRouter as a NautGate route and retain manual model IDs',async({page})=>{
 await models(page);await page.evaluate(async()=>{
  window.__xnautStub.settings_get.llm_providers=[{name:'nautgate',endpoint:'http://gateway.example/v1',api_key:'fixture',enabled:true}];
  const host=document.createElement('div');host.id='settings-fixture';host.style.cssText='position:fixed;inset:10px;z-index:99999;overflow:auto;background:#111';document.body.appendChild(host);
  await window.xnautRenderTasksModeSettings(host);
 });
 await expect(page.locator('#tm-llm-endpoint')).toHaveValue('http://gateway.example/v1');
 await page.getByRole('combobox',{name:'Chat provider route'}).selectOption('openrouter');
 await page.getByRole('combobox',{name:'Available chat models'}).selectOption('openrouter/google/gemini-3.7-flash');
 await expect(page.locator('#tm-llm-endpoint')).toHaveValue('http://gateway.example/v1');
 await page.locator('#tm-llm-model').fill('vendor/custom-model');await page.locator('#tm-llm-model').press('Tab');
 await page.locator('#tm-llm-endpoint').fill('http://new-gateway.example/v1');
 await page.locator('#tm-save').click();
 await expect.poll(()=>page.evaluate(()=>window.__xnautStub.settings_get.llm.model)).toBe('openrouter/vendor/custom-model');
 expect(await page.evaluate(()=>window.__xnautStub.settings_get)).toMatchObject({chat_model_source:'workspace',llm:{provider:'nautgate',model:'openrouter/vendor/custom-model'}});
 expect(await page.evaluate(()=>window.__xnautStub.settings_get.llm_providers.find(p=>p.name==='nautgate').endpoint)).toBe('http://new-gateway.example/v1');
});

test('disabled gateway saves a direct OpenAI route and legacy sync preserves the switch',async({page})=>{
 await models(page);await page.evaluate(async()=>{
  window.__xnautStub.settings_get.llm_providers=[{name:'nautgate',endpoint:'http://gateway.example/v1',api_key:'gateway-fixture',enabled:true}];
  const host=document.createElement('div');host.id='settings-fixture';host.style.cssText='position:fixed;inset:10px;z-index:99999;overflow:auto;background:#111';document.body.appendChild(host);
  await window.xnautRenderTasksModeSettings(host);
 });
 await page.getByLabel('Route model requests through NautGate').uncheck();
 await page.getByRole('combobox',{name:'Chat provider route'}).selectOption('codex');
 await expect(page.locator('#tm-llm-endpoint')).toHaveValue('https://api.openai.com/v1');
 await expect(page.locator('#tm-llm-key')).toHaveValue('');
 await page.locator('#tm-llm-key').fill('direct-fixture');
 await page.locator('#tm-llm-model').fill('gpt-6-astra');
 await page.locator('#tm-save').click();
 await expect.poll(()=>page.evaluate(()=>window.__xnautStub.settings_get.llm.provider)).toBe('openai');
 await page.evaluate(()=>window.xnautSyncChatSettingsFromAiSettings());
 const saved=await page.evaluate(()=>window.__xnautStub.settings_get);
 expect(saved.llm).toMatchObject({provider:'openai',model:'gpt-6-astra',api_key:'direct-fixture',endpoint:'https://api.openai.com/v1'});
 expect(saved.llm_providers.find(p=>p.name==='nautgate')).toMatchObject({enabled:false,api_key:'gateway-fixture'});
});

test('direct provider models remain selectable and an empty refreshed catalog removes stale routes',async({page})=>{
 await models(page);await page.evaluate(async()=>{
  window.__xnautStub.chat_list_provider_models=[{provider:'openai',model:'gpt-6-astra'},{provider:'openrouter',model:'vendor/model'}];
  await window.xnautModelCatalog.refresh();
  window.xnautAttachChatTab({title:'Direct models',chatKey:'direct-models'});
 });
 const select=page.getByRole('combobox',{name:'Chat model',exact:true});
 await select.selectOption(JSON.stringify(['openai','gpt-6-astra']));
 expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-chat-model:direct-models')))).toEqual({provider:'openai',model:'gpt-6-astra'});
 await page.evaluate(async()=>{window.__xnautStub.chat_list_provider_models=[];await window.xnautModelCatalog.refresh();});
 expect(await page.evaluate(()=>window.xnautModelCatalog.all())).toEqual([]);
});

test('opening gateway settings preserves a primary credential when the registry key is empty',async({page})=>{
 await models(page);await page.evaluate(async()=>{
  window.__xnautStub.settings_get.llm={provider:'nautgate',endpoint:'http://primary-gateway/v1',api_key:'primary-fixture-key',model:'gpt-6-astra'};
  window.__xnautStub.settings_get.llm_providers=[{name:'nautgate',endpoint:'',api_key:null,enabled:true}];
  const host=document.createElement('div');host.style.cssText='position:fixed;inset:10px;z-index:99999;overflow:auto;background:#111';document.body.appendChild(host);
  await window.xnautRenderTasksModeSettings(host);
 });
 await expect(page.locator('#tm-llm-endpoint')).toHaveValue('http://primary-gateway/v1');
 await expect(page.locator('#tm-llm-key')).toHaveValue('primary-fixture-key');
 await page.locator('#tm-save').click();
 await expect.poll(()=>page.evaluate(()=>window.__xnautStub.settings_get.llm_providers.find(p=>p.name==='nautgate').api_key)).toBe('primary-fixture-key');
});
