// XNAUT-342, the removal half. André, testing 1.27.1: "why is the old project
// view still there, the workspace was supposed to replace that."
//
// The fold had landed the tabs and left the standalone Projects panel reachable
// beside them — its own project list, its own dropdown, its own nine-tab nav.
// Two projects' worth of navigation for one project. These cover the two halves
// of removing it, and the second half is the one worth testing:
//
//   1. Nothing opens the old panel any more. Asserted as an absence, and an
//      absence is only worth asserting if the thing that replaced it is
//      asserted too — otherwise "nothing opened" passes on an app where
//      nothing works.
//   2. Every route that USED to open it lands in the project workspace
//      instead: the top-bar button, the Observatory's last-project card, and
//      the New project form. Each one is a real click here, because each was
//      a call to a global that no longer exists, and an undefined global in JS
//      is a silent no-op rather than a crash (CLAUDE.md).
import { test, expect } from '@playwright/test';

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

const PROJECTS = [
  { key: 'SMOKE', name: 'Smoke Test', purpose: 'exercise the panels', source_path: '/tmp/smoke', stage: 'build', flow_type: 'standard', revision: 1 },
];

const STUB = {
  pm_module_status: OK,
  pm_project_list: PROJECTS,
  pm_project_import_existing: PROJECTS,
  list_directory: [{ name: 'README.md', path: '/tmp/smoke/README.md', is_directory: false }],
  read_file: '# Smoke\n',
  project_facts: { is_repo: true, branch: 'main', changes: 0, worktrees: 1, last_commit_ms: 1 },
  memory_find_cmd: [],
  memory_list: [],
};

async function boot(page, extra) {
  page.on('dialog', (dialog) => dialog.dismiss().catch(() => {}));
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((s) => { Object.assign(window.__xnautStub, s); }, { ...STUB, ...(extra || {}) });
  // The app finishes waking for a couple of seconds; clicking into that makes
  // every assertion below race the boot rather than the change.
  await page.waitForTimeout(2500);
}

test('the top-bar button opens the project workspace, not the Projects panel', async ({ page }) => {
  await boot(page);

  // The control is named for what it does now. "Open Projects (tasks & plan)"
  // named a panel that no longer exists.
  const button = page.getByRole('button', { name: 'Open project workspace', exact: true });
  await expect(button).toHaveCount(1);
  await button.click();

  await expect(page.locator('.wsp')).toBeVisible();
  await expect(page.locator('.wsp-name')).toHaveText('Smoke Test');
  // And it is the workspace, not the panel wearing a new label: the panel's own
  // chrome is absent because the panel's standalone mode is gone.
  await expect(page.locator('.pmw-project-select')).toHaveCount(0);
  await expect(page.locator('.pmw-rail')).toHaveCount(0);
  await expect(page.locator('.pmw-project-nav')).toHaveCount(0);
});

test('with no project registered the button says where one is made', async ({ page }) => {
  await boot(page, { pm_project_list: [], pm_project_import_existing: [] });

  await page.getByRole('button', { name: 'Open project workspace', exact: true }).click();

  // An empty workspace for a project that does not exist would be the dishonest
  // answer: it looks like a project with nothing in it.
  await expect(page.locator('.wsp')).toHaveCount(0);
  await expect(page.locator('body')).toContainText('No projects yet');
});

test('no route opens the old Projects panel', async ({ page }) => {
  await boot(page);

  // 'pm' was the nav key, and xnautAttachProjectManagementTab the global it
  // called. Both are gone; the key warns instead of opening something.
  const dead = await page.evaluate(() => ({
    attach: typeof window.xnautAttachProjectManagementTab,
    show: typeof window.xnautShowProject,
  }));
  expect(dead.attach, 'xnautAttachProjectManagementTab is still assigned').toBe('undefined');
  expect(dead.show, 'xnautShowProject is still assigned').toBe('undefined');

  const warned = await page.evaluate(() => {
    const seen = [];
    const real = console.warn;
    console.warn = (...args) => { seen.push(args.join(' ')); real(...args); };
    window.xnautSidebarNavigate('pm');
    console.warn = real;
    return seen.join('\n');
  });
  expect(warned, 'the pm nav key was handled instead of reported unknown').toContain('unknown sidebar nav key');
  // Nothing opened. A tab called Projects is exactly what was removed.
  await expect(page.locator('.pmw-rail')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Open Projects (tasks & plan)' })).toHaveCount(0);
});

test("the Observatory's last-project card opens NAUT-Flow at the stage it remembered", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    localStorage.setItem('xnaut-nf-last', JSON.stringify({
      key: 'SMOKE', name: 'Smoke Test', stage: 'PRD', stageKey: 'prd', at: Date.now(),
    }));
  });
  await page.evaluate(() => window.xnautAttachObservatoryTab());
  await expect(page.locator('.obs')).toBeVisible();

  const card = page.locator('.obs-lastproj');
  await expect(card, 'the last-project card never rendered').toHaveCount(1);
  await card.click();

  // The workspace, on its NAUT-Flow tab, standing on the stage the card named —
  // not on stage 1 and not on the project's own current stage. That distinction
  // is the whole point of carrying flowStage: a remembered stage the workspace
  // ignored would look right most of the time. 'prd' is the fourth standard
  // stage, titled "Product requirements"; the project's own stage is 'build',
  // the twelfth, and its centre says "Build".
  await expect(page.locator('.wsp')).toBeVisible();
  await expect(page.locator('.wsp-tabs button[data-wsp-tab="nautflow"]')).toHaveClass(/active/);
  const centre = page.locator('.wsp-surface .pmw-nf-center');
  await expect(centre).toContainText('Product requirements');
  await expect(page.locator('.wsp-surface .pmw-nf-rail-count'), 'the rail lost the project position').toHaveText('12 / 15');
});

test('creating a project lands in its workspace on the Work tab', async ({ page }) => {
  // The form calls project_create then pm_project_create; both answer, and the
  // project list has to know the new key or the workspace has nothing to open.
  const created = [...PROJECTS, { key: 'FRESH', name: 'Fresh', purpose: '', source_path: '/tmp/fresh', stage: '', flow_type: 'standard', revision: 1 }];
  await boot(page, {
    project_create: { ok: true },
    pm_project_create: { key: 'FRESH', name: 'Fresh' },
    pm_project_list: created,
    pm_project_import_existing: created,
  });

  await page.evaluate(() => window.xnautSidebarNavigate('new-project'));
  await expect(page.locator('.rpnp')).toBeVisible();
  // Name and path both gate the button (right-pane-newproject.js `sync`).
  await page.locator('.rpnp-name').fill('Fresh');
  await page.locator('.rpnp-path').fill('/tmp/fresh');
  await expect(page.locator('.rpnp-create')).toBeEnabled();
  await page.locator('.rpnp-create').click();

  await expect(page.locator('.wsp')).toBeVisible();
  await expect(page.locator('.wsp-name')).toHaveText('Fresh');
  await expect(page.locator('.wsp-tabs button[data-wsp-tab="work"]')).toHaveClass(/active/);
});
