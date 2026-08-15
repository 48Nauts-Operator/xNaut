import { test, expect } from '@playwright/test';

// The Librarian moved from a right-pane view to an agent. Its conversations
// had to move with it: a feature that relocates and strands the history has
// taken something away.
test('old Librarian conversations become @librarian threads', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-vault:last', 'work');
    localStorage.setItem('xnaut-vault-conversations:work', JSON.stringify([
      { title: 'create a new document Research/project-101.md', at: '2026-07-06T09:09:00Z',
        messages: [
          { role: 'user', content: 'create a new document Research/project-101.md' },
          { role: 'assistant', content: 'The templates are now ready for use.' },
        ] },
    ]));
    localStorage.setItem('xnaut-chat-history:vault:work', JSON.stringify([
      { role: 'user', content: 'please copy this document also to the vault' },
      { role: 'assistant', content: 'The Concept.md note has been created in the vault.' },
    ]));
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);

  const threads = await page.evaluate(() => JSON.parse(localStorage.getItem('xnaut-agent-threads:v1') || '{}').librarian || []);
  expect(threads.length).toBeGreaterThanOrEqual(2);
  const titles = threads.map((thread) => thread.title);
  expect(titles.some((title) => title.includes('project-101'))).toBe(true);
  expect(threads.every((thread) => thread.messages.length > 0)).toBe(true);

  // Runs once, not on every load.
  const again = await page.evaluate(() => {
    const before = JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).librarian.length;
    return { before, flag: localStorage.getItem('xnaut-librarian-threads-migrated') };
  });
  expect(again.flag).toBe('1');

  // And the icon is gone from the right pane.
  expect(await page.locator('[data-rpane-view="librarian"]').count()).toBe(0);
});
