import { test, expect } from '@playwright/test';

// A VM an agent spins up has to be visible where the agent is, not only in the
// transcript that mentions it (XNAUT: "I want to see it on the right pane").
const VM = {
  vm_name: 'nautgate-nga', status: 'running', emoji: '⚓',
  ssh_command: 'ssh nautgate-nga.exe.xyz',
  https_url: 'https://nautgate-nga.exe.xyz',
  terminal_url: 'https://nautgate-nga.xterm.exe.xyz',
};

test('an exe.dev VM shows up as a computer in the agent pane', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.getByRole('button', { name: 'More surfaces' }).click();
  await page.getByText('Agent Space', { exact:true }).first().click();

  // No machines: the section is absent rather than empty.
  await expect(page.getByText('Computer · exe.dev')).toHaveCount(0);

  await page.evaluate((vm) => { window.__xnautStub.exe_machines = [vm]; }, VM);
  // Re-opening an agent re-asks exe.dev, which is what a person clicking here means.
  await page.locator('.asl-agent', { hasText:'Builder' }).first().click();

  await expect(page.getByText('Computer · exe.dev')).toBeVisible();
  await expect(page.getByText('⚓ nautgate-nga')).toBeVisible();
  await expect(page.getByText('ssh nautgate-nga.exe.xyz · running')).toBeVisible();

  // Clicking terminal opens an SSH terminal tab to the VM (e12e2dd). The VM's
  // xterm web page is private and its OAuth hop escapes an embedded webview,
  // so nothing is mounted in the pane's preview any more; the spec asserted
  // that old behaviour for a week after it was removed.
  await page.getByRole('button', { name:'terminal', exact:true }).click();
  await page.waitForTimeout(300);
  const shell = await page.evaluate(() => (window.__xnautInvokes || []).find((item) => item.cmd === 'create_command_session'));
  expect(shell, 'the terminal button opens an SSH session').toBeTruthy();
  expect(shell.args.config.args.join(' ')).toContain('ssh -t -o StrictHostKeyChecking=accept-new nautgate-nga.exe.xyz');
  await expect(page.locator('[data-artifact]')).toHaveCount(0);
});
