import {test,expect} from '@playwright/test';
async function models(page){
 await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
 await page.evaluate(async()=>{
  window.__xnautStub.chat_list_provider_models=[{provider:'nautgate',model:'auto'},{provider:'nautgate',model:'gpt-5.6-sol'},{provider:'nautgate',model:'claude-fable-5'},{provider:'nautgate',model:'openrouter/google/gemini-3.7-flash'},{provider:'lmstudio',model:'local-qwen'}];
  await window.xnautModelCatalog.refresh();
 });
}
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
 await page.locator('[data-form]').evaluate(form=>form.requestSubmit());
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
