import { test, expect } from '@playwright/test';

async function mount(page) {
  await page.evaluate(async () => {
    let host = document.getElementById('cloud-model-settings-test');
    if (!host) {
      host = document.createElement('div');
      host.id = 'cloud-model-settings-test';
      host.style.cssText = 'position:fixed;inset:10px;z-index:99999;overflow:auto;background:#111';
      document.body.append(host);
    }
    await window.xnautRenderTasksModeSettings(host);
  });
}

test.beforeEach(async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => window.xnautRenderTasksModeSettings && window.__xnautStub);
  await expect(page.locator('.mesh')).toBeVisible();
  await page.evaluate(() => {
    const settings = window.__xnautStub.settings_get;
    settings.llm_providers = [{ name: 'cloud-test', endpoint: 'https://models.example/v1', api_key: 'fixture-credential', enabled: true }];
    settings.cloud_agent_model = { provider: '', model: '', worker_endpoint: '' };
    settings.future_key = { keep: 42 };
    settings.sandboxes = [{ kind: 'exe-dev', base_url: '' }, { kind: 'gitvm', base_url: '' }];
    settings.worker_network = { auth_key: 'fixture-enrollment', tags: 'tag:workers' };
    window.__xnautStub.chat_list_provider_models = [{ provider: 'cloud-test', model: 'gpt-cloud-test', label: 'Cloud Test' }];
  });
  await page.evaluate(() => window.xnautModelCatalog.refresh());
  await mount(page);
});

test('one cloud model survives save and reopen without changing chat, workers or unknown settings', async ({ page }) => {
  const original = await page.evaluate(() => structuredClone(window.__xnautStub.settings_get));
  await expect(page.getByLabel('Model', { exact: true })).toBeDisabled();
  await page.getByLabel('Provider connection').selectOption('cloud-test');
  await expect(page.locator('#tm-cloud-models option')).toHaveAttribute('value', 'gpt-cloud-test');
  await page.locator('#tm-cloud-model').fill('gpt-cloud-test');
  await page.getByLabel('Worker endpoint (optional)').fill('https://worker-models.example/v1');
  await page.locator('#tm-save').click();
  await expect(page.locator('#tm-status')).toContainText('Settings saved');
  const saved = await page.evaluate(() => window.__xnautStub.settings_get);
  expect(saved.cloud_agent_model).toEqual({ provider: 'cloud-test', model: 'gpt-cloud-test', worker_endpoint: 'https://worker-models.example/v1' });
  expect(saved.sandboxes).toEqual(original.sandboxes);
  expect(saved.worker_network).toEqual(original.worker_network);
  expect(saved.future_key).toEqual(original.future_key);
  expect(saved.llm.model).toBe(original.llm.model);
  expect(saved.llm_providers.find(p => p.name === 'cloud-test').api_key).toBe('fixture-credential');
  await mount(page);
  await expect(page.getByLabel('Provider connection')).toHaveValue('cloud-test');
  await expect(page.locator('#tm-cloud-model')).toHaveValue('gpt-cloud-test');
  await expect(page.getByLabel('Worker endpoint (optional)')).toHaveValue('https://worker-models.example/v1');
});

test('clearing the shared selection removes its entire override and leaves profiles in control', async ({ page }) => {
  await page.getByLabel('Provider connection').selectOption('cloud-test');
  await page.locator('#tm-cloud-model').fill('gpt-cloud-test');
  await page.getByLabel('Worker endpoint (optional)').fill('https://worker-models.example/v1');
  await page.locator('#tm-save').click();
  await page.getByLabel('Provider connection').selectOption('');
  await page.locator('#tm-save').click();
  await expect.poll(() => page.evaluate(() => window.__xnautStub.settings_get.cloud_agent_model))
    .toEqual({ provider: '', model: '', worker_endpoint: '' });
  await mount(page);
  await expect(page.locator('#tm-cloud-model')).toBeDisabled();
  await expect(page.locator('#tm-cloud-model')).toHaveValue('');
});

test('native validation refusal is shown without claiming the settings were saved', async ({ page }) => {
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (cmd, args) => cmd === 'settings_set'
      ? Promise.reject('The cloud model endpoint points to localhost.') : invoke(cmd, args);
  });
  await page.getByLabel('Provider connection').selectOption('cloud-test');
  await page.locator('#tm-cloud-model').fill('gpt-cloud-test');
  await page.getByLabel('Worker endpoint (optional)').fill('http://localhost:8090/v1');
  await page.locator('#tm-save').click();
  await expect(page.locator('#tm-status')).toContainText('Save failed: The cloud model endpoint points to localhost.');
  expect(await page.evaluate(() => window.__xnautStub.settings_get.cloud_agent_model.model)).toBe('');
});
