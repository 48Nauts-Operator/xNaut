import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
});


// NautBot is pinned first and therefore selected by default, so a test that
// wants the seeded agent has to say so.
async function openBuilder(page) {
  await page.getByText('Agent Space', { exact:true }).first().click();
  await page.locator('.asl-agent', { hasText:'Builder' }).first().click();
  await expect(page.locator('.as-title h1')).toHaveText('Builder');
}

// A build now takes a handshake: the agent asks WHERE, the owner answers, and
// only then does a harness start. Tests that need a running session go through
// this rather than pretending a message launches one.
async function startBuild(page, text, repo = '/tmp/smoke') {
  await page.evaluate(() => { window.__xnautStub.agent_chat_turn = 'BUILD-REQUEST\nDoing the work.'; });
  await page.getByLabel('Message @builder').fill(text);
  await page.getByLabel('Message @builder').press('Enter');
  const card = page.locator('.as-build');
  await expect(card).toBeVisible();
  await card.locator('[data-build-path]').fill(repo);
  await card.getByRole('button', { name:'Open worktree & build' }).click();
  await expect(page.getByText('Working…', { exact:true })).toBeVisible();
}

test('Agent Space owns the second-left library and bounded agent threads', async ({ page }) => {
  await page.getByText('Agent Space', { exact:true }).first().click();
  await expect(page.locator('.agent-space')).toBeVisible();
  await expect(page.getByRole('complementary', { name:'Agent Library' })).toBeVisible();
  // NautBot is pinned first as the orchestrator, so the library holds it plus
  // the seeded agent (ca37f9c). The spec asserted 1 long after that landed.
  await expect(page.locator('.asl-agent')).toHaveCount(2);
  await expect(page.locator('.sbar-agent')).toHaveCount(0);
  // NautBot is the master agent and opens first.
  await expect(page.locator('.as-title h1')).toHaveText('NautBot');
  await expect(page.locator('.as-handle')).toHaveText('@nautbot');
  await expect(page.getByLabel('Message @nautbot')).toBeVisible();
  // The compute view has a right-pane slot now; without one the quick pane
  // registered itself and had nowhere to render (XNAUT-144).
  await expect(page.locator('[data-rpane-view="agent"]')).toHaveCount(1);
  await expect(page.locator('[data-rpane-view="chat"]')).toHaveCount(0);
});

test('agent settings and the global create menu use the Agent Space shell', async ({ page }) => {
  await openBuilder(page);
  await page.getByRole('button', { name:'Settings', exact:true }).click();
  await expect(page.getByRole('heading', { name:'Agent settings.' })).toBeVisible();
  await expect(page.locator('input[name="handle"]')).toHaveValue('builder');

  await page.getByRole('button', { name:'New terminal' }).click();
  const createMenu = page.locator('#new-tab-menu');
  await expect(createMenu.locator('button').first()).toContainText('New Agent');
  await createMenu.getByText('New Agent', { exact:true }).click();
  await expect(page.getByRole('heading', { name:'Create a new agent.' })).toBeVisible();
  await expect(page.locator('input[name="tagline"]')).toBeVisible();
});

test('a message is a chat turn against the agent, not a coding session', async ({ page }) => {
  // The flow this replaces launched a CLI in a zellij worktree for every
  // message, including "what is the status" (XNAUT-159).
  await openBuilder(page);
  const composer = page.getByLabel('Message @builder');
  await composer.fill('What is the release status?');
  await composer.press('Enter');
  await expect(page.getByText('The release is tagged and the cask is on 1.15.0.', { exact:true })).toBeVisible();
  const invokes = await page.evaluate(() => window.__xnautInvokes.map((item) => item.cmd));
  expect(invokes).toContain('agent_chat_turn');
  expect(invokes).not.toContain('agent_profile_launch');
  expect(invokes).not.toContain('agent_build_workspace');
});

test('a build request asks where before it opens a worktree', async ({ page }) => {
  await page.evaluate(() => { window.__xnautStub.agent_chat_turn = 'BUILD-REQUEST\nA single-file HTML shooter.'; })
    .catch(() => {});
  await page.addInitScript(() => {
    // The stub is built per page load, so the override has to be re-applied
    // once it exists rather than before navigation.
    const apply = () => {
      if (!window.__xnautStub) return setTimeout(apply, 20);
      window.__xnautStub.agent_chat_turn = 'BUILD-REQUEST\nA single-file HTML shooter.';
    };
    apply();
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await openBuilder(page);
  const composer = page.getByLabel('Message @builder');
  await composer.fill('Build me a 3D shooter in one HTML file');
  await composer.press('Enter');

  // The marker never reaches the thread; the summary does, with the card.
  await expect(page.getByText('A single-file HTML shooter.', { exact:true })).toBeVisible();
  await expect(page.getByText('BUILD-REQUEST')).toHaveCount(0);
  const card = page.locator('.as-build');
  await expect(card).toBeVisible();
  // Nothing has launched yet: that is the whole point of the handshake.
  expect(await page.evaluate(() => window.__xnautInvokes.some((item) => item.cmd === 'agent_profile_launch'))).toBe(false);

  await card.locator('[data-build-path]').fill('/tmp/smoke');
  await card.getByRole('button', { name:'Open worktree & build' }).click();

  await expect(page.getByText('Working…', { exact:true })).toBeVisible();
  const workspace = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'agent_build_workspace').args);
  expect(workspace).toMatchObject({ handle:'builder', repoPath:'/tmp/smoke' });
  const launch = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'agent_profile_launch').args.req);
  // The run happens in the WORKTREE the backend returned, never in the repo
  // the owner has open.
  expect(launch).toMatchObject({ handle:'builder', worktree_path:'/tmp/smoke/.worktrees/run-the-checks', conversation_mode:true });
  expect(launch).not.toHaveProperty('worktreePath');
  expect(launch.prompt).toContain('Build me a 3D shooter in one HTML file');
  await expect(page.getByText('/tmp/smoke/.worktrees/run-the-checks')).toBeVisible();
});

test('each thread menu can archive and permanently delete a thread', async ({ page }) => {
  await page.evaluate(() => {
    localStorage.setItem('xnaut-agent-threads:v1', JSON.stringify({ builder:[
      { id:'keep-thread', title:'Keep me', created_at:'2026-08-14T09:00:00Z', updated_at:'2026-08-14T09:00:00Z', messages:[] },
      { id:'archive-thread', title:'Archive me', created_at:'2026-08-14T10:00:00Z', updated_at:'2026-08-14T10:00:00Z', messages:[] },
    ] }));
  });
  await openBuilder(page);
  // Threads live in a collapsed section under the agent, so a test has to open
  // it the way a person does.
  const threadRows = page.locator('[data-library-thread="archive-thread"]');
  if (!await threadRows.count()) await page.locator('[data-threads-toggle="builder"]').click();
  const archiveRow = threadRows;
  await archiveRow.getByRole('button', { name:/Actions for thread/ }).click();
  await page.getByRole('button', { name:'Archive', exact:true }).click();
  // The archived section is collapsible and counts what it holds.
  await expect(page.locator('.asl-archive-head')).toContainText('Archived · 1');
  await page.locator('.asl-archive-head').click();

  const archivedRow = page.locator('[data-library-thread="archive-thread"]');
  await archivedRow.getByRole('button', { name:/Actions for archived thread/ }).click();
  // window.confirm resolves to null in this webview WITHOUT rendering, which
  // is why deleting looked dead; the confirmation is in-app now.
  await page.getByRole('button', { name:'Delete…', exact:true }).click();
  await page.locator('.as-dialog').getByRole('button', { name:'Delete', exact:true }).click();
  await expect(page.locator('[data-library-thread="archive-thread"]')).toHaveCount(0);
  await expect(page.locator('[data-library-thread="keep-thread"]')).toBeVisible();
});

test('Gemini JSONL deltas become one clean assistant message', async ({ page }) => {
  await openBuilder(page);
  await startBuild(page, 'Build it');
  await page.evaluate(() => {
    const output = [
      JSON.stringify({ type:'init', session_id:'gemini-thread-1', model:'gemini-3-pro' }),
      JSON.stringify({ type:'message', role:'assistant', content:'Building ', delta:true }),
      JSON.stringify({ type:'message', role:'assistant', content:'now.', delta:true }),
      JSON.stringify({ type:'result', status:'success', stats:{} }),
      '',
    ].join('\n');
    window.__xnautEmit('terminal-output:smoke-agent', { data:btoa(output) });
    window.__xnautEmit('agent-status-changed', { session_id:'smoke-agent', status:'done' });
  });
  await expect(page.getByText('Building now.', { exact:true })).toBeVisible();
  await expect(page.getByText('[object Object]', { exact:true })).toHaveCount(0);
});

test('an agent with no project still answers, and a build never runs in home', async ({ page }) => {
  // The old flow interrogated the owner for a project before it would answer
  // anything. A question needs no repository; a BUILD needs one, and it must
  // never be the home directory (a coding CLI stops at a trust prompt there,
  // and the scope is far too broad).
  await page.evaluate(() => {
    window.__xnautStub.agent_profile_list[1].default_project = null;
    window.__xnautInvokes.length = 0;
    window.xnautOpenAgentSpace('builder');
  });
  await expect(page.getByLabel('Message @builder')).toBeVisible();
  await page.getByLabel('Message @builder').fill('What is the release status?');
  await page.getByLabel('Message @builder').press('Enter');
  await expect(page.getByText('The release is tagged and the cask is on 1.15.0.', { exact:true })).toBeVisible();
  expect(await page.evaluate(() => window.__xnautInvokes.some((item) => item.cmd === 'agent_profile_launch'))).toBe(false);

  await startBuild(page, 'Build the honey website', '/tmp/new-honey');
  const launch = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'agent_profile_launch'));
  expect(launch.args.req.worktree_path).toBe('/tmp/smoke/.worktrees/run-the-checks');
  expect(await page.evaluate(() => window.__xnautInvokes.some((item) => item.cmd === 'get_home_directory'))).toBe(false);
});

test('terminal inspection explicitly switches to the attached background session', async ({ page }) => {
  await openBuilder(page);
  await startBuild(page, 'Run the checks');
  await page.getByRole('button', { name:'Open terminal' }).click();
  await expect(page.locator('.terminal-output')).toBeVisible();
  await expect(page.locator('.tab.active')).toHaveAttribute('data-agent-session-id', 'smoke-agent');
});

test('a specialist receives the current Control Center conversation on handoff', async ({ page }) => {
  await page.evaluate(() => window.xnautSetChatHistory('control-center:nautbot', [
    { role:'user', content:'The release target is Friday.' },
    { role:'assistant', content:'I will keep Friday as the release target.' },
  ]));
  await openBuilder(page);
  await startBuild(page, 'Continue with the release work');
  const prompt = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'agent_profile_launch').args.req.prompt);
  expect(prompt).toContain('PORTABLE XNAUT CONVERSATION HANDOFF');
  expect(prompt).toContain('The release target is Friday.');
  expect(prompt).toContain('Active project/worktree: /tmp/smoke');
  expect(prompt).toContain('LATEST USER REQUEST\nContinue with the release work');
});

test('Settings-page NautGate credentials reach the backend before an agent answers', async ({ page }) => {
  // Ported from control-center.spec.mjs, which tested a surface André had
  // removed. The invariant outlived the page: AI Settings writes to the legacy
  // webview store, and the Rust provider registry only learns about a token
  // through the sync. Without it the first message after entering a key fails
  // with "provider is not configured".
  await page.evaluate(() => {
    localStorage.setItem('xnaut-settings', JSON.stringify({
      nautgateUrl:'http://localhost:8090/v1', apiKeyNautGate:'test-nautgate-token',
      llmProvider:'lmstudio', llmModel:'local-model', lmstudioUrl:'http://localhost:1238',
    }));
  });
  await page.reload();
  await page.waitForTimeout(900);
  await openBuilder(page);
  await page.getByLabel('Message @builder').fill('Use the configured route');
  await page.getByLabel('Message @builder').press('Enter');

  const sync = await page.evaluate(() => window.__xnautInvokes
    .filter((item) => item.cmd === 'settings_set')
    .map((item) => item.args.settings)
    .find((item) => item.llm_providers?.some((provider) => provider.name === 'nautgate' && provider.api_key === 'test-nautgate-token')));
  expect(sync).toBeTruthy();
  const nautgate = sync.llm_providers.find((provider) => provider.name === 'nautgate');
  expect(nautgate).toMatchObject({ endpoint:'http://localhost:8090/v1', enabled:true });
});
