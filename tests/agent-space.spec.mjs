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
  await page.getByRole('button', { name: 'More surfaces' }).click();
  await page.getByText('Agent Space', { exact:true }).first().click();
  await page.locator('.asl-agent', { hasText:'Builder' }).first().click();
  await expect(page.locator('.as-title h1')).toHaveText('Builder');
}

// The chooser supplies a root to the ticket-aware chat loop. It never launches directly.
async function startBuild(page, text, repo = '/tmp/smoke') {
  await page.evaluate(repo => {
    const invoke=window.__TAURI__.core.invoke;
    let turn=0;
    window.__TAURI__.core.invoke=async(name,args)=>{
      if(name!=='agent_chat_turn') return invoke(name,args);
      window.__xnautInvokes.push({cmd:name,args});
      if(turn++===0) return 'BUILD-REQUEST\nDoing the work.';
      window.__xnautEmit('agent-task-started',{requestId:args.requestId,agent_id:'builder',receipt:{
        ok:true,ticket:'FIXTURE-1',task:'Run the checks',worktree_path:repo+'/.worktrees/ticket-task',
        launch:{session_id:'smoke-agent',run_id:'fixture-run'}
      }});
      return 'Worker launched for FIXTURE-1; completion is not yet verified.';
    };
  },repo);
  await page.getByLabel('Message @builder').fill(text);
  await page.getByLabel('Message @builder').press('Enter');
  const card = page.locator('.as-build');
  await expect(card).toBeVisible();
  await card.locator('[data-build-path]').fill(repo);
  await card.getByRole('button', { name:'Open worktree & build' }).click();
  await expect(page.getByText('Worker launched for FIXTURE-1; completion is not yet verified.',{exact:true})).toBeVisible();
}

test('Agent Space owns the second-left library and bounded agent threads', async ({ page }) => {
  await page.getByRole('button', { name: 'More surfaces' }).click();
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
  // The Journal replaces the duplicate Agent overview (XNAUT-489).
  await expect(page.locator('[data-rpane-view="agent"]')).toHaveCount(0);
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

test('the repository picker returns through ticketed chat and cannot launch a bare worktree', async ({ page }) => {
  await openBuilder(page);
  await startBuild(page,'Build me a 3D shooter in one HTML file');
  const calls=await page.evaluate(()=>window.__xnautInvokes);
  expect(calls.some(c=>c.cmd==='agent_build_workspace'||c.cmd==='agent_profile_launch')).toBe(false);
  const request=calls.filter(c=>c.cmd==='agent_chat_turn').at(-1).args;
  expect(request.messages.at(-1).content).toContain('Repository: /tmp/smoke');
  expect(request.messages.at(-1).content).toContain('Build me a 3D shooter');
  expect(request.threadId).toBeTruthy();
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
  await page.locator('.xdlg').getByRole('button', { name:'Delete', exact:true }).click();
  await expect(page.locator('[data-library-thread="archive-thread"]')).toHaveCount(0);
  await expect(page.locator('[data-library-thread="keep-thread"]')).toBeVisible();
});

test('ticketed worker receipts persist without being mistaken for completion', async ({ page }) => {
  await openBuilder(page);
  await startBuild(page,'Build it');
  await page.evaluate(()=>window.xnautConversationStorage.flush());
  const receipt=await page.evaluate(()=>Object.values(JSON.parse(window.xnautConversationStorage.getItem('xnaut-agent-threads:v1'))).flat().flatMap(t=>t.messages).find(m=>m.executionReceipt)?.executionReceipt);
  expect(receipt.ticket).toBe('FIXTURE-1');
  expect(receipt.launch.run_id).toBe('fixture-run');
  await expect(page.getByText('[object Object]',{exact:true})).toHaveCount(0);
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
  expect(await page.evaluate(() => window.__xnautInvokes.some(i => i.cmd === 'agent_profile_launch' || i.cmd === 'agent_build_workspace'))).toBe(false);
  await expect(page.getByText('/tmp/new-honey/.worktrees/ticket-task',{exact:true})).toBeVisible();
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
  const prompt = await page.evaluate(() => window.__xnautInvokes.filter(i=>i.cmd==='agent_chat_turn').at(-1).args.messages.map(m=>m.content).join('\n'));
  expect(prompt).toContain('PORTABLE XNAUT CONVERSATION HANDOFF');
  expect(prompt).toContain('The release target is Friday.');
  expect(prompt).toContain('Active project/worktree: /tmp/smoke');
  expect(prompt).toContain('Continue with the release work');
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

test('the + opens the Plugins modal and connects one to THIS agent', async ({ page }) => {
  // Plugins are configured once and handed out per agent: a planner must not
  // hold a payments server because Stripe was connected for something else
  // (the deviation XNAUT-147 called out).
  await openBuilder(page);
  await page.getByRole('button', { name:'Plugins', exact:true }).click();
  const modal = page.locator('.as-plug');
  await expect(modal).toBeVisible();
  await expect(modal.getByRole('button', { name:'Marketplace' })).toBeVisible();

  const row = modal.locator('[data-row="context7"]');
  await expect(row).toBeVisible();
  await expect(row.locator('svg')).toBeVisible(); // the brand mark, not a placeholder

  // The backend does the whole connect, so the UI makes ONE call and then
  // re-reads the profile rather than assembling capabilities itself.
  await page.evaluate(() => {
    window.__xnautStub.agent_profile_get = { ...window.__xnautStub.agent_profile_list[1],
      capabilities: ['plugin:context7'] };
  });
  await row.getByRole('button', { name:'Add' }).click();
  const sent = await page.evaluate(() => window.__xnautInvokes
    .filter((item) => item.cmd === 'plugin_connect').at(-1));
  expect(sent.args.id).toBe('context7');
  expect(sent.args.agent).toBe('builder');
  // Connected reads as a green check square, not a word.
  await expect(modal.locator('[data-row="context7"] .as-plug-check')).toBeVisible();

  // Yours shows what this agent holds, and nothing else.
  await modal.getByRole('button', { name:'Yours' }).click();
  await expect(modal.locator('[data-row="context7"]')).toBeVisible();
  await expect(modal.locator('[data-row="stripe"]')).toHaveCount(0);

  // A row opens its detail, with a link to where the thing comes from.
  await modal.getByRole('button', { name:'Marketplace' }).click();
  await modal.locator('[data-row="stripe"] .as-plug-copy').click();
  await expect(modal.getByRole('link', { name:/View source/ })).toBeVisible();
  await expect(modal.getByText('Connector', { exact:true })).toBeVisible();
  await expect(modal.getByText('Skills', { exact:true })).toBeVisible();
});

test('a credential typed in a plugin row reaches the backend in one call', async ({ page }) => {
  // "The Forgejo token is still not persistent, I have added it at least three
  // times." The UI used to save and grant as two calls from JavaScript, with
  // an alert() for anything that failed — so a rejected save looked like a
  // credential that simply would not stick. One backend call now writes,
  // verifies, enables and hands over.
  await openBuilder(page);
  await page.getByRole('button', { name:'Plugins', exact:true }).click();
  const modal = page.locator('.as-plug');

  // Add on a plugin that is missing something opens the fields in place.
  await modal.locator('[data-row="stripe"] [data-add]').click();
  await modal.locator('[data-row="stripe"] [data-key="url"]').fill('https://example/mcp');
  await modal.locator('[data-row="stripe"] [data-key="env:STRIPE_KEY"]').fill('rk_live_typed');
  await modal.locator('[data-row="stripe"] [data-save]').click();

  const sent = await page.evaluate(() => window.__xnautInvokes
    .filter((item) => item.cmd === 'plugin_connect').at(-1));
  expect(sent.args.id).toBe('stripe');
  expect(sent.args.values.STRIPE_KEY).toBe('rk_live_typed');
  expect(sent.args.url).toBe('https://example/mcp');
  expect(sent.args.agent).toBe('builder');
});

test('a connect that fails says so in the row instead of an alert', async ({ page }) => {
  // An alert() is dismissed and forgotten; the row keeps the reason next to
  // the field that has to change.
  await page.addInitScript(() => {
    const apply = () => {
      if (!window.__TAURI__) return setTimeout(apply, 20);
      const core = window.__TAURI__.core;
      const original = core.invoke;
      core.invoke = (cmd, args) => (cmd === 'plugin_connect'
        ? Promise.reject('Stripe needs STRIPE_KEY')
        : original(cmd, args));
    };
    apply();
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await openBuilder(page);
  await page.getByRole('button', { name:'Plugins', exact:true }).click();
  const modal = page.locator('.as-plug');
  await modal.locator('[data-row="stripe"] [data-add]').click();
  await modal.locator('[data-row="stripe"] [data-save]').click();
  await expect(modal.locator('[data-row="stripe"] .as-plug-problem')).toContainText('needs STRIPE_KEY');
  // And the fields stay open so the missing value can be typed.
  await expect(modal.locator('[data-row="stripe"] [data-key="env:STRIPE_KEY"]')).toBeVisible();
});

test('the newest message is scrolled into view, not left under the composer', async ({ page }) => {
  // .as-messages never scrolls; .as-body does. Setting scrollTop on the list
  // was a silent no-op and the last reply sat behind the composer.
  await openBuilder(page);
  for (const line of ['one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight']) {
    await page.getByLabel('Message @builder').fill(`tell me about ${line}`);
    await page.getByLabel('Message @builder').press('Enter');
  }
  await expect(page.locator('.as-message').last()).toBeVisible();
  // Polled, like the geometry below: the view settles a frame or two after
  // paint, and measuring once made this fail under a loaded full-suite run
  // while passing on its own — a flaky test is worse than no test.
  await expect.poll(async () => page.evaluate(() => {
    const box = document.querySelector('.as-body');
    return Math.ceil(box.scrollHeight - box.scrollTop - box.clientHeight);
  }), { timeout: 5000 }).toBeLessThan(120);

  // Being scrolled to the bottom is NOT the same as being able to read the
  // last line: the composer sits at the bottom of the scroller, so the real
  // assertion is geometric. Polled rather than timed — the view settles a
  // frame or two after paint, and a fixed wait made this flaky under load.
  await expect.poll(async () => page.evaluate(() => {
    const last = [...document.querySelectorAll('.as-message')].pop();
    const composer = document.querySelector('.as-composer-wrap');
    return Math.ceil(last.getBoundingClientRect().bottom - composer.getBoundingClientRect().top);
  }), { timeout: 5000 }).toBeLessThanOrEqual(0);

  // And the composer itself has to be fully on screen. Making it sticky inside
  // the scroller satisfied the assertion above while clipping the box at the
  // window edge — which is worse than the bug it replaced.
  const composer = await page.evaluate(() => {
    const wrap = document.querySelector('.as-composer-wrap').getBoundingClientRect();
    return { top: wrap.top, bottom: wrap.bottom, viewport: window.innerHeight };
  });
  expect(composer.bottom).toBeLessThanOrEqual(composer.viewport);
  expect(composer.bottom - composer.top).toBeGreaterThan(40);
});

test('the project button offers both new and existing, not one behind a modifier', async ({ page }) => {
  // It was "new folder unless you hold shift". Connecting a project that
  // already exists is the common case, not the hidden one.
  await openBuilder(page);
  await page.getByRole('button', { name: 'Project folder' }).click();
  const dialog = page.locator('.as-project-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('Yes, new project')).toBeVisible();
  await expect(dialog.getByText('No, existing project')).toBeVisible();

  // And the existing branch asks for a path rather than inventing one.
  await dialog.getByText('No, existing project').click();
  await expect(page.locator('[data-project-path]')).toBeVisible();
  await expect(page.locator('.as-project-dialog h2')).toHaveText('Connect the existing project.');
});

// XNAUT-159's last open item: the chat turn arrived all at once, so a long
// answer sat on 'Thinking…' with no sign of life. The turn now paints as it
// generates, and the returned reply still replaces what was painted.
test('a chat answer paints while it is generated, and the final reply wins', async ({ page }) => {
  await openBuilder(page);
  // The command is held open so the stream can be observed mid-flight; a stub
  // that answers immediately unsubscribes before the first chunk lands.
  await page.evaluate(() => {
    window.__xnautStub.agent_chat_turn = new Promise((resolve) => { window.__answer = resolve; });
  });
  await page.getByLabel('Message @builder').fill('what is the release order?');
  await page.getByLabel('Message @builder').press('Enter');

  const bubble = page.locator('.as-message.agent').last().locator('.as-message-text');
  await expect(bubble).toHaveText('Thinking…');

  const stream = async (delta, requestId) => page.evaluate(([delta, requestId]) => {
    const turn = window.__xnautInvokes.filter((item) => item.cmd === 'agent_chat_turn').pop();
    window.__xnautEmit('chat://chunk', { requestId: requestId || turn.args.requestId, delta });
  }, [delta, requestId]);

  await stream('Forgejo first');
  await expect(bubble).toHaveText('Forgejo first');
  await stream(', then the tag.');
  await expect(bubble).toHaveText('Forgejo first, then the tag.');

  // Another turn's chunks must not bleed into this bubble.
  await stream(' NOT THIS', 'agent-chat-someone-else');
  await expect(bubble).toHaveText('Forgejo first, then the tag.');

  await page.evaluate(() => window.__answer('Forgejo first, then the tag to GitHub.'));
  await expect(bubble).toHaveText('Forgejo first, then the tag to GitHub.');
  expect(await page.evaluate(() => window.__xnautErrors)).toEqual([]);
});


test('Compute saves an explicit destination independently from the model', async ({ page }) => {
  await openBuilder(page);
  await page.getByRole('button', { name:'Settings', exact:true }).click();
  const compute = page.locator('select[name="execution"]');
  await expect(compute.locator('option')).toHaveText(['Local', 'exe.dev', 'GitVM', 'Automatic (configured provider)']);
  await compute.selectOption('exe-dev');
  await page.getByRole('button', { name:'Save changes', exact:true }).click();
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'agent_profile_update').at(-1)?.args.profile.execution)).toBe('exe-dev');
});

test('a worker launch receipt appears in chat, opens its session, and survives in follow-up context', async ({ page }) => {
  await openBuilder(page);
  await page.evaluate(() => {
    window.__xnautStub.agent_chat_turn = new Promise((resolve) => { window.__launchAnswer = resolve; });
  });
  await page.getByLabel('Message @builder').fill('Run the JobUp security review in its own worktree.');
  await page.getByLabel('Message @builder').press('Enter');
  await expect(page.getByText('Thinking…', { exact:true })).toBeVisible();
  await page.evaluate(() => {
    const turn = window.__xnautInvokes.filter((i) => i.cmd === 'agent_chat_turn').at(-1);
    const receipt = { ok:true, execution_started:true, worktree_path:'/tmp/JobUp/.worktrees/isolated', launch:{session_id:'smoke-agent',run_id:'audit-run'} };
    window.__xnautEmit('agent-task-started', {requestId:'other-turn',agent_id:'builder',receipt});
  });
  await expect(page.getByText('Worker launched', {exact:true})).toHaveCount(0);
  await page.evaluate(() => {
    const turn = window.__xnautInvokes.filter((i) => i.cmd === 'agent_chat_turn').at(-1);
    const receipt = { ok:true, execution_started:true, worktree_path:'/tmp/JobUp/.worktrees/isolated', launch:{session_id:'smoke-agent',run_id:'audit-run'} };
    for (let i=0;i<2;i++) window.__xnautEmit('agent-task-started', {requestId:turn.args.requestId,agent_id:'builder',receipt});
    window.__launchAnswer('Worker launched. Findings are not available yet.');
  });
  await expect(page.getByText('Worker launched', {exact:true})).toHaveCount(1);
  await expect(page.locator('.as-build')).toHaveCount(0);
  await page.locator('[data-open-session="smoke-agent"]').click();
  await expect(page.locator('.terminal-output')).toBeVisible();
  await page.evaluate(() => window.xnautOpenAgentSpace('builder'));
  await page.evaluate(() => { window.__xnautStub.agent_chat_turn = 'I will check that run.'; });
  await page.getByLabel('Message @builder').fill('What did the audit find?');
  await page.getByLabel('Message @builder').press('Enter');
  await expect(page.getByText('I will check that run.',{exact:true})).toBeVisible();
  const context=await page.evaluate(() => window.__xnautInvokes.filter((i)=>i.cmd==='agent_chat_turn').at(-1).args.messages);
  expect(context.some((m)=>m.content.includes('audit-run') && m.content.includes('not proof of completion'))).toBe(true);
  expect(await page.evaluate(()=>window.__xnautErrors)).toEqual([]);
});
