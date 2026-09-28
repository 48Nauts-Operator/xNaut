// XNAUT-87: the project overview reports state, it never invents it.
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

// Opens the workspace on the overview for one project shape. Both project
// commands are stubbed: the panel's first load calls import_existing, and a
// later refresh calls list — a fixture that answers only one of them tests a
// different project than the one it set up.
async function overview(page, project) {
  const pane = await page.evaluate(async ([status, record]) => {
    window.__xnautStub.pm_module_status = status;
    window.__xnautStub.pm_project_import_existing = [record];
    window.__xnautStub.pm_project_list = [record];
    document.querySelectorAll('#pm-test-host').forEach((node) => node.remove());
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
    return true;
  }, [OK, { ...BASE, ...project }]).then(() => page.locator('#pm-test-host .pmw'));

  const card = pane.locator('[data-project]:not([data-project=""])').first();
  await expect(card, 'no project in the sidebar to open').toHaveCount(1);
  await card.click();
  await expect(pane.locator('.pmw-project-nav')).toBeVisible();
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
    const pane = await overview(page, project);

    await expect(pane.locator('.pmw-stage-badge'),
      'the hero still badges a stage the project is not in').toHaveCount(0);
    await expect(pane.locator('.pmw-stage-band'),
      'a Current stage band was rendered for a project with no stage').toHaveCount(0);
    // The primary artifact IS the current stage's document, so a project with
    // no stage must not be offered one to open.
    await expect(pane.locator('.pmw-open-overview-artifact'),
      'the page offers to open a stage document that does not exist').toHaveCount(0);
    await expect(pane.getByText('Primary artifact')).toHaveCount(0);
  });
}

test('a project genuinely in a flow still shows its stage', async ({ page }) => {
  const pane = await overview(page, { stage: 'build', owner: 'Builder' });

  await expect(pane.locator('.pmw-stage-badge')).toHaveText('build');
  const band = pane.locator('.pmw-stage-band');
  await expect(band, 'the Current stage band vanished for a project that has one').toHaveCount(1);
  // The label and the position both come from the stage table, so a wrong
  // stage cannot pass by rendering the right word.
  await expect(band).toContainText('Build');
  await expect(band).toContainText('of');
  await expect(pane.locator('.pmw-open-overview-artifact')).toHaveCount(1);
});

test('the overview shows no hardcoded quality gate or readiness bar', async ({ page }) => {
  const pane = await overview(page, { stage: 'build' });

  // Both read as measurements and neither was ever computed from anything:
  // the gate was three checkboxes that nothing ticked, the bar was the stage
  // index wearing a percentage.
  await expect(pane.getByText('Quality gate')).toHaveCount(0);
  await expect(pane.getByText('Ticket readiness')).toHaveCount(0);
  await expect(pane.locator('.pmw-gate-item, .pmw-readiness')).toHaveCount(0);
});

test('an unowned project lists no contributors', async ({ page }) => {
  const pane = await overview(page, { stage: 'build', owner: '' });

  await expect(pane.getByText('Contributors'),
    'an unowned project still got a Contributors band').toHaveCount(0);
  await expect(pane.getByText('Unassigned', { exact: true }).locator('..').locator('.pmw-contributor-avatar')).toHaveCount(0);
});

test('an owned project lists its owner as the contributor', async ({ page }) => {
  const pane = await overview(page, { stage: 'build', owner: 'Andre' });

  const row = pane.locator('.pmw-contributor-row');
  await expect(row).toHaveCount(1);
  await expect(row).toContainText('Andre');
});

test('the Artifacts tab offers no current document when there is no stage', async ({ page }) => {
  const pane = await overview(page, { stage: '' });
  await pane.locator('[data-project-section="artifacts"]').click();

  await expect(pane.locator('.pmw-open-stage-artifacts'),
    '"Open current document" is offered for a project with no current stage').toHaveCount(0);
  await expect(pane.locator('.pmw-project-empty')).toContainText('not in NAUT-Flow');
});

// Opening the NAUT-Flow tab is how you LOOK at the flow, not how you enter it.
// The editor still has to open on some stage, but the rail must not claim the
// project is standing on it — that is the same fabrication one tab across, and
// with imports no longer stamped with a stage it is now the common case.
test('the NAUT-Flow rail says "Not started" instead of putting an unstaged project on stage 1', async ({ page }) => {
  const pane = await overview(page, { stage: '' });
  await pane.locator('[data-project-section="nautflow"]').click();

  await expect(pane.locator('.pmw-nf-rail-count')).toHaveText('Not started');
  await expect(pane.locator('.pmw-vstage-current'),
    'a stage was marked current for a project that has not started the flow').toHaveCount(0);
  await expect(pane.locator('.pmw-vstage-done')).toHaveCount(0);
});

test('the NAUT-Flow rail shows the real position once the project is in a flow', async ({ page }) => {
  const pane = await overview(page, { stage: 'prd' });
  await pane.locator('[data-project-section="nautflow"]').click();

  // 'prd' is the fourth STANDARD stage, of fifteen.
  await expect(pane.locator('.pmw-nf-rail-count')).toHaveText('4 / 15');
  await expect(pane.locator('.pmw-vstage-current')).toHaveCount(1);
  await expect(pane.locator('.pmw-vstage-done')).toHaveCount(3);
});

test.beforeEach(async ({ page }) => {
  page.on('dialog', (dialog) => dialog.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
});
