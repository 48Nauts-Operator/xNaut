import { test, expect } from '@playwright/test';

// Exercise the real workspace, right pane and Wiki together. A dropdown whose
// label changes while its checkout and Wiki remain on the old project is broken.
async function openWorkspace(page) {
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (cmd, args) => {
      const handler = window.__xnautStub[cmd];
      if (typeof handler !== 'function') return invoke(cmd, args);
      window.__xnautInvokes.push({ cmd, args });
      return Promise.resolve().then(() => handler(args));
    };
    const projects = [
      { key: 'CHESS', name: 'ChessTrainer', source_path: '/projects/ChessTrainer' },
      { key: 'CODE', name: 'CodeLib', source_path: '/projects/CodeLib' },
      { key: 'EMPTY', name: 'No checkout', source_path: '' },
    ];
    Object.assign(window.__xnautStub, {
      pm_project_list: projects, pm_project_import_existing: projects,
      list_directory: ({ path }) => [{ name: 'README.md', path: `${path}/README.md`, is_directory: false }],
      project_facts: { is_repo: true, branch: 'main', changes: 0, worktrees: 1 },
      project_wiki_overview: ({ project }) => {
        const p = projects.find(p => project === p.key || (p.source_path && String(project || '').startsWith(p.source_path + '/')))
          || projects.find(p => p.source_path && p.source_path === project);
        if (!p) throw new Error('Select a registered project to open its Wiki');
        return { project: { ...p, root: p.source_path }, documents: [], runs: [], activity: [], warnings: [],
          observed_at: new Date().toISOString(), stats: {} };
      },
    });
    window.xnautRightPaneShow('workspace');
    window.xnautRightPaneSetRoot('/projects/ChessTrainer/feature');
  });
  await page.locator('.rpws-nav [data-sub="wiki"]').click();
  await page.evaluate(() => window.xnautOpenWorkspace({ project: 'CHESS', worktree: '/projects/ChessTrainer/feature' }));
  await expect(page.locator('.wsp-name')).toHaveText('ChessTrainer');
}

test('changing project replaces its worktree, file tree and already-open Wiki together', async ({ page }) => {
  await openWorkspace(page);
  await page.locator('.wsp .xps-select').selectOption('CODE');
  await expect(page.locator('.wsp-name')).toHaveText('CodeLib');
  await expect(page.locator('.wsp-root')).toHaveText('/projects/CodeLib');
  await expect(page.locator('.wsp-row[data-file="/projects/CodeLib/README.md"]')).toBeVisible();
  await expect(page.locator('.rpane-title')).toHaveAttribute('title', '/projects/CodeLib');
  await expect(page.locator('[data-project-wiki-host] [data-project-name]')).toHaveText('CodeLib');
  expect(await page.evaluate(() => window.xnautActiveProjectPath())).toBe('/projects/CodeLib');
  expect(await page.evaluate(() => window.xnautActiveProjectKey())).toBe('CODE');
});

test('a project without a checkout clears the previous file and Wiki context', async ({ page }) => {
  await openWorkspace(page);
  await page.locator('.wsp .xps-select').selectOption('EMPTY');
  await expect(page.locator('.wsp-root')).toHaveText('no source path');
  await expect(page.locator('.wsp-tree')).toContainText('no source path');
  await expect(page.locator('.wsp-row')).toHaveCount(0);
  await expect(page.locator('.rpane-title')).toHaveAttribute('title', '');
  await expect(page.locator('[data-project-wiki-host] [data-project-name]')).not.toHaveText('ChessTrainer');
  expect(await page.evaluate(() => window.xnautActiveProjectPath())).toBeNull();
});

test('returning to Workspace restores its selected project in the right pane', async ({ page }) => {
  await openWorkspace(page);
  await page.locator('.wsp .xps-select').selectOption('CODE');
  await expect(page.locator('.wsp-name')).toHaveText('CodeLib');
  const workspace = await page.locator('.tab.active').getAttribute('data-session-id');
  await page.evaluate(() => {
    window.xnautAttachMarkdownTab({ filename: 'Other document' });
    window.xnautRightPaneSetRoot('/projects/ChessTrainer');
  });
  await expect(page.locator('.rpane-title')).toHaveAttribute('title', '/projects/ChessTrainer');
  await page.locator(`.tab[data-session-id="${workspace}"]`).click();
  await expect(page.locator('.rpane-title')).toHaveAttribute('title', '/projects/CodeLib');
  await expect(page.locator('[data-project-wiki-host] [data-project-name]')).toHaveText('CodeLib');
});
