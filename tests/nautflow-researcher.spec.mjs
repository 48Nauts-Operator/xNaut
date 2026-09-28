// XNAUT-356: the Analyst and the Architect get live outside knowledge.
//
// No Council integration — NautFlow is already a council in sequence, and the
// only thing it lacked was a member who looks outside. What can go quietly
// wrong here, and is asserted on values rather than pixels:
//
//   1. the brief is fetched and then not carried into the launch. The prompt is
//      what the agent actually sees, so the assertion is on `loom_run`'s goal,
//      including the source URLs it is told to cite from;
//   2. with no Perplexity key the stage silently loses its research and looks
//      identical to a researched one. It has to run anyway AND say on the card
//      that the Researcher was unavailable;
//   3. research leaks into stages that never asked for it — the Reviewer with a
//      web search is scope, not rigour.
import { test, expect } from '@playwright/test';

const NAUTBOT = {
  handle: 'nautbot', display_name: 'NautBot', tagline: '', purpose: '', runtime_id: 'codex',
  provider: 'nautgate', model: 'gpt-5.6-sol', chat_model: '', reasoning_effort: 'high', execution: 'local',
  role: 'core-orchestrator', capabilities: [], notifications: true, accent_color: '#f5b840',
  default_project: null, created_at: '2026-09-13T00:00:00Z', updated_at: '2026-09-13T00:00:00Z',
};
const ANALYST = {
  ...NAUTBOT, handle: 'analyst', display_name: 'Analyst', runtime_id: 'claude',
  provider: 'anthropic', model: 'claude-sonnet-5', reasoning_effort: '', role: 'analysis',
};
const REVIEWER = { ...ANALYST, handle: 'reviewer', display_name: 'Reviewer', role: 'reviewer' };
// The seed's Researcher: a search provider, a model that cites.
const RESEARCHER = {
  ...NAUTBOT, handle: 'researcher', display_name: 'Researcher', runtime_id: 'claude',
  provider: 'perplexity', model: 'sonar-pro', reasoning_effort: '', role: 'researcher',
  capabilities: ['research', 'search'],
};

const AVAILABLE = { available: true, reason: '', handle: 'researcher', provider: 'perplexity', model: 'sonar-pro' };
const BRIEF = {
  ...AVAILABLE,
  answer: '- Two comparable products ship a sequential review council [1].\n- The standard everyone cites is BMAD [2].',
  sources: [
    { title: 'Sequential agent councils in the wild', url: 'https://example.com/councils' },
    { title: '', url: 'https://example.com/bmad' },
  ],
};

const STUB = {
  agent_profile_list: [NAUTBOT, ANALYST, RESEARCHER, REVIEWER],
  research_status: AVAILABLE,
  research_brief: BRIEF,
  vault_note_read: null,
  loom_run: { pid: 4242, log: '/tmp/persona.log' },
};

const errors = (page) => page.evaluate(() => window.__xnautErrors || []);
const invokes = (page, cmd) => page.evaluate((c) => window.__xnautInvokes.filter((i) => i.cmd === c).map((i) => i.args), cmd);

async function openNautFlow(page, stub) {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
  await page.evaluate((s) => {
    Object.assign(window.__xnautStub, s);
    localStorage.setItem('xnaut-nf-mode:SMOKE', 'expert');
    localStorage.setItem('xnaut-nf-valoverride:SMOKE', '1');
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, { project: 'SMOKE', section: 'nautflow' });
  }, stub);
  const pane = page.locator('#pm-test-host .pmw');
  return pane;
}

test('the Analyst is briefed by the Researcher, and told to cite it', async ({ page }) => {
  const pane = await openNautFlow(page, STUB);

  // Idea is the Analyst's stage: the card names who will research, before
  // anything is clicked.
  await pane.locator('[data-flow-stage="idea"]').first().click();
  await expect(pane.locator('.pmw-stage-research')).toHaveText('researcher · @researcher · perplexity · sonar-pro');

  await pane.locator('.pmw-ask-agent').click();
  await page.waitForTimeout(800);

  // The Researcher was actually asked, about THIS project and THIS stage.
  const asked = await invokes(page, 'research_brief');
  expect(asked.length, 'the Analyst launched without asking the Researcher').toBe(1);
  expect(asked[0].question).toContain('Idea');

  // …and what came back reached the agent, sources and citing rule included.
  const runs = await invokes(page, 'loom_run');
  expect(runs.length).toBe(1);
  const goal = runs[0].goal;
  expect(goal, 'the brief never reached the prompt').toContain('RESEARCH BRIEF');
  expect(goal).toContain('@researcher · perplexity · sonar-pro');
  expect(goal).toContain('The standard everyone cites is BMAD');
  expect(goal).toContain('[1] Sequential agent councils in the wild — https://example.com/councils');
  expect(goal).toContain('[2] https://example.com/bmad');
  expect(goal).toContain('## Sources');
  // Evidence, not instruction: the owner contract still wins.
  expect(goal).toContain('does not override the owner contract');
  // The run panel says the research happened, and with how many sources.
  await expect(page.locator('.nfr-body')).toContainText('@researcher (perplexity · sonar-pro) returned 2 sources');

  expect(await errors(page)).toEqual([]);
});

test('with no Perplexity key the stage runs anyway and the card says the Researcher was unavailable', async ({ page }) => {
  const pane = await openNautFlow(page, {
    ...STUB,
    research_status: { available: false, reason: 'no Perplexity key — add one in Settings › AI', handle: 'researcher', provider: 'perplexity', model: 'sonar-pro' },
    research_brief: { available: false, reason: 'no Perplexity key — add one in Settings › AI', handle: 'researcher', provider: 'perplexity', model: 'sonar-pro', answer: '', sources: [] },
  });

  await pane.locator('[data-flow-stage="idea"]').first().click();
  await expect(pane.locator('.pmw-stage-research')).toHaveText('Researcher unavailable — no Perplexity key — add one in Settings › AI');

  await pane.locator('.pmw-ask-agent').click();
  await page.waitForTimeout(800);

  // The stage still runs — as the Analyst, with its own document — and the
  // prompt carries no brief and no invented sources.
  const runs = await invokes(page, 'loom_run');
  expect(runs.length, 'a missing optional key stopped the stage').toBe(1);
  expect(runs[0].goal).not.toContain('RESEARCH BRIEF');
  expect(runs[0].goal).toContain('BMAD Analyst');
  // Nothing was spent on a call that cannot succeed.
  expect((await invokes(page, 'research_brief')).length).toBe(0);
  await expect(page.locator('.nfr-body')).toContainText('no research — no Perplexity key');

  expect(await errors(page)).toEqual([]);
});

test('a stage that does not research neither shows the badge nor calls out', async ({ page }) => {
  const pane = await openNautFlow(page, STUB);

  // Test and review is the Reviewer's. It verifies what is in front of it.
  await pane.locator('[data-flow-stage="test_review"]').first().click();
  await expect(pane.locator('.pmw-stage-agent')).toHaveText('Reviewer · @reviewer · anthropic · claude-sonnet-5');
  await expect(pane.locator('.pmw-stage-research')).toHaveCount(0);

  await pane.locator('.pmw-ask-agent').click();
  await page.waitForTimeout(800);
  expect((await invokes(page, 'research_brief')).length, 'the Reviewer went researching').toBe(0);
  const runs = await invokes(page, 'loom_run');
  expect(runs.length).toBe(1);
  expect(runs[0].goal).not.toContain('RESEARCH BRIEF');

  expect(await errors(page)).toEqual([]);
});
