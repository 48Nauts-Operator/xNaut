// XNAUT-87: a project surface reports state, it never invents it.
//
// The bug these cover is one line —
//   const stage = stages.some((i) => i[0] === project.stage) ? project.stage : stages[0][0]
// — which turned "this project has no stage I recognise" into "this project is
// at stage 1". It is untestable by reading: the fabricated value is a perfectly
// well-formed stage key, so the page looks right and is wrong. The only way to
// catch it is to hand the panel a project that is NOT in a flow and assert the
// page says nothing about stages.
//
// Four ways a project can fail to be in a flow are exercised, because the
// original ternary collapsed all of them to the same wrong answer: no stage
// field, an empty one, a stage key that is not in any table, and a stage that
// belongs to a DIFFERENT flow than the project's own.
//
// XNAUT-342 moved where these are asked. The Overview tab was the original
// reader and is gone: its Active work table is the Work tab, its Current stage
// band the NAUT-Flow tab, its Primary artifact band the Vault tab and its
// Contributors band the Project details entry of the three-dot menu. So the
// stage the hero badges, the Artifacts page and the NAUT-Flow rail are where
// the fabrication would now show, and the bands are asserted ABSENT rather than
// honest — a band that came back would be a second reader of the same stage.
import { test, expect } from '@playwright/test';

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

const BASE = {
  key: 'SMOKE', name: 'Smoke Test', purpose: 'exercise the panels',
  source_path: '/tmp/smoke', flow_type: 'standard', status: 'active', tickets: [],
};

// Mounts one section of the panel for one project shape, the way the workspace
// mounts it: for a named project, at a named section. Both project commands are
// stubbed because the panel's first load calls import_existing and a later
// refresh calls list — a fixture that answers only one of them tests a
// different project than the one it set up.
async function section(page, key, project) {
  await page.evaluate(async ([status, record, sectionKey]) => {
    window.__xnautStub.pm_module_status = status;
    window.__xnautStub.pm_project_import_existing = [record];
    window.__xnautStub.pm_project_list = [record];
    document.querySelectorAll('#pm-test-host').forEach((node) => node.remove());
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, { project: 'SMOKE', section: sectionKey });
  }, [OK, { ...BASE, ...project }, key]);
  const pane = page.locator('#pm-test-host .pmw');
  // Wait for the SECTION to have painted rather than for the pane element to
  // exist. Every section draws the project hero except NAUT-Flow, which fills
  // the pane with its stage rail and centre instead.
  await expect(pane.locator(key === 'nautflow' ? '.pmw-nf-rail' : '.pmw-project-hero')).toBeVisible();
  return pane;
}

const NOT_IN_A_FLOW = [
  ['no stage field at all', {}],
  ['an empty stage', { stage: '' }],
  ['a stage no flow defines', { stage: 'shipped' }],
  // 'concept' is a STANDARD stage; this project runs the feature flow, which
  // does not have it. Before the fix this rendered as the feature flow's first
  // stage, which the project had never been anywhere near.
  ['a stage belonging to another flow', { flow_type: 'feature', stage: 'concept' }],
];

for (const [label, project] of NOT_IN_A_FLOW) {
  test(`a project with ${label} shows no stage at all`, async ({ page }) => {
    const pane = await section(page, 'details', project);

    await expect(pane.locator('.pmw-stage-badge'),
      'the hero still badges a stage the project is not in').toHaveCount(0);
    // The bands the Overview tab used to draw. They are gone with that tab, and
    // a project with no stage is exactly the case where one coming back would
    // fabricate a position again.
    await expect(pane.locator('.pmw-stage-band'),
      'a Current stage band was rendered for a project with no stage').toHaveCount(0);
    await expect(pane.locator('.pmw-open-overview-artifact'),
      'the page offers to open a stage document that does not exist').toHaveCount(0);
    await expect(pane.getByText('Primary artifact')).toHaveCount(0);
  });
}

test('a project genuinely in a flow still shows its stage', async ({ page }) => {
  const pane = await section(page, 'details', { stage: 'build', owner: 'Builder' });

  // The badge is the stage, read from the project rather than defaulted.
  await expect(pane.locator('.pmw-stage-badge')).toHaveText('build');
  // And Project details names the stage in words as well, so a wrong stage
  // cannot pass by rendering the right key.
  await expect(pane.locator('.pmw-project-page')).toContainText('build');
});

test('the NAUT-Flow tab names the stage and its position, both read from the project', async ({ page }) => {
  const pane = await section(page, 'nautflow', { stage: 'build' });

  // The label and the position both come from the stage table.
  await expect(pane.locator('.pmw-nf-rail-count')).toHaveText('12 / 15');
  await expect(pane.locator('.pmw-nf-center')).toContainText('Build');
});

test('no project surface shows a hardcoded quality gate or readiness bar', async ({ page }) => {
  // Both read as measurements and neither was ever computed from anything:
  // the gate was three checkboxes that nothing ticked, the bar was the stage
  // index wearing a percentage. Asked of every section that draws a page.
  for (const key of ['details', 'artifacts', 'settings', 'nautflow']) {
    const pane = await section(page, key, { stage: 'build' });
    await expect(pane.getByText('Quality gate'), `${key} shows a quality gate`).toHaveCount(0);
    await expect(pane.getByText('Ticket readiness'), `${key} shows a readiness bar`).toHaveCount(0);
    await expect(pane.locator('.pmw-gate-item, .pmw-readiness')).toHaveCount(0);
  }
});

// The Contributors band was the Overview's, and the checklist of 2026-09-22 puts
// it in Project details (XNAUT-342). "Unassigned" there is the honest answer for
// a project with no owner, where the band's answer was a contributor row for a
// project that had no contributors.
test('an unowned project says Unassigned rather than inventing a contributor', async ({ page }) => {
  const pane = await section(page, 'details', { stage: 'build', owner: '' });

  await expect(pane.locator('.pmw-contributor-row'),
    'the deleted Contributors band came back').toHaveCount(0);
  await expect(pane.locator('.pmw-project-page')).toContainText('Unassigned');
});

test('an owned project names its owner in Project details', async ({ page }) => {
  const pane = await section(page, 'details', { stage: 'build', owner: 'Andre' });

  await expect(pane.locator('.pmw-project-page')).toContainText('Andre');
  await expect(pane.locator('.pmw-project-page')).not.toContainText('Unassigned');
});

// The health card's Started had no home once the Overview tab went, so the
// checklist puts it here beside budget, rate and flow (XNAUT-342). It is
// created_at, a stored field: a project without one says so instead of showing
// today's date or a dash that could mean anything.
test('Project details carries Started, and says when it was never recorded', async ({ page }) => {
  let pane = await section(page, 'details', { stage: 'build', created_at: '2026-04-17T09:30:00Z' });
  await expect(pane.locator('[data-project-started]')).toHaveText('2026-04-17');

  pane = await section(page, 'details', { stage: 'build' });
  await expect(pane.locator('[data-project-started]')).toHaveText('Not recorded');
});

test('Project details carries the budget, rate and flow the health card showed', async ({ page }) => {
  const pane = await section(page, 'details', {
    stage: 'build', budget_chf: 24000, hourly_rate_chf: 180, flow_type: 'feature',
  });

  const page_ = pane.locator('.pmw-project-page');
  await expect(page_).toContainText('24,000');
  await expect(page_).toContainText('180');
  await expect(page_).toContainText('Feature');
});

test('the Artifacts tab offers no current document when there is no stage', async ({ page }) => {
  const pane = await section(page, 'artifacts', { stage: '' });

  await expect(pane.locator('.pmw-open-stage-artifacts'),
    '"Open current document" is offered for a project with no current stage').toHaveCount(0);
  await expect(pane.locator('.pmw-project-empty')).toContainText('not in NAUT-Flow');
});

// Opening the NAUT-Flow tab is how you LOOK at the flow, not how you enter it.
// The editor still has to open on some stage, but the rail must not claim the
// project is standing on it — that is the same fabrication one tab across, and
// with imports no longer stamped with a stage it is now the common case.
test('the NAUT-Flow rail says "Not started" instead of putting an unstaged project on stage 1', async ({ page }) => {
  const pane = await section(page, 'nautflow', { stage: '' });

  await expect(pane.locator('.pmw-nf-rail-count')).toHaveText('Not started');
  await expect(pane.locator('.pmw-vstage-current'),
    'a stage was marked current for a project that has not started the flow').toHaveCount(0);
  await expect(pane.locator('.pmw-vstage-done')).toHaveCount(0);
});

test('the NAUT-Flow rail shows the real position once the project is in a flow', async ({ page }) => {
  const pane = await section(page, 'nautflow', { stage: 'prd' });

  // 'prd' is the fourth STANDARD stage, of fifteen.
  await expect(pane.locator('.pmw-nf-rail-count')).toHaveText('4 / 15');
  await expect(pane.locator('.pmw-vstage-current')).toHaveCount(1);
  await expect(pane.locator('.pmw-vstage-done')).toHaveCount(3);
});

// The removal itself, asserted where it would be undone: the Overview, Docs and
// Delivery sections do not exist, so asking for one is a stale call site and the
// panel says so instead of falling through to the page it used to draw.
test('a section this surface no longer has names itself instead of rendering Overview', async ({ page }) => {
  for (const gone of ['overview', 'docs', 'delivery']) {
    const pane = await section(page, gone, { stage: 'build' });
    await expect(pane.locator('.pmw-empty.pmw-error'),
      `the ${gone} section still rendered something`).toContainText(`There is no “${gone}” section`);
    await expect(pane.locator('.pmw-overview-layout')).toHaveCount(0);
    await expect(pane.locator('.pmw-active-work-table')).toHaveCount(0);
    await expect(pane.locator('.pmw-project-docs')).toHaveCount(0);
  }
});

test.beforeEach(async ({ page }) => {
  page.on('dialog', (dialog) => dialog.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
});
