// tests/features/designer.feature, the two @smoke scenarios.
//
// These were reported UNTESTED by every run so far, on both machines, and the
// feature file says why that matters: the Designer was reported fixed three
// times in one day while the canvas showed a holding page over a finished site.
// The surface most in need of a check had none.
//
// The reason it stayed untested is that the panel mounts from
// project-management-panel.js:1403, so reaching it needs PM configured with a
// project. That is a route problem, not a Designer problem, so these tests mount
// the panel directly through its own entry point:
//
//     window.xnautDesigner.mount(host, project)
//
// What that covers: the Designer panel's own behaviour. What it does not cover:
// navigating to it. Those are different failures and the second one belongs to
// the PM tests and the native AX walk.
//
// The backend is faked, so the fake has to be faithful or this is theatre. Every
// rule below is mirrored from src-tauri/src/designer.rs with the line noted, and
// if that file changes this file is wrong rather than merely stale:
//
//   designer.rs:210  a new design is created with runtime "local"
//   designer.rs:353  set_runtime refuses while the design is live, with
//                    "stop this design before changing where it runs"
//   designer.rs:341  live means: local -> a port is open; sandbox -> a lease
//                    that has not expired. The JS mirror at
//                    designer-panel.js:32 reads public_url for local.
import { test, expect } from '@playwright/test';

const PROJECT = { name: 'SMOKE', key: 'SMOKE' };

function fakeDesigner(seed) {
  return (initial) => {
    window.__designerCalls = [];
    let designs = initial.slice();
    const live = (d) => (d.runtime === 'local'
      ? !!d.public_url
      : !!d.sandbox_id && d.sandbox_expires_ms > Date.now());

    const install = setInterval(() => {
      const core = window.__TAURI__ && window.__TAURI__.core;
      if (!core) return;
      clearInterval(install);
      const real = core.invoke;
      core.invoke = (cmd, args) => {
        if (cmd.startsWith('designer_')) window.__designerCalls.push({ cmd, args });
        switch (cmd) {
          case 'designer_list':
            return Promise.resolve(designs);
          case 'designer_get':
            return Promise.resolve(designs.find((d) => d.slug === args.slug) || null);
          case 'designer_create': {
            // designer.rs:210 — created local, not sandbox.
            const d = {
              slug: `design-${designs.length + 1}`, name: args.name, kind: args.kind,
              created_at_ms: Date.now(), updated_at_ms: Date.now(), archived: false,
              sandbox_id: '', public_url: '', sandbox_expires_ms: 0, messages: [],
              session_id: '', runtime: 'local', local_port: 0,
            };
            designs.push(d);
            return Promise.resolve(d);
          }
          case 'designer_set_runtime': {
            const d = designs.find((x) => x.slug === args.slug);
            if (!d) return Promise.reject('no such design');
            if (args.runtime !== 'local' && args.runtime !== 'sandbox') {
              return Promise.reject(`unknown runtime: ${args.runtime}`);
            }
            if (d.runtime === args.runtime) return Promise.resolve(d);
            // designer.rs:353 — refused while running, and the reason is the
            // point of the scenario.
            if (live(d)) return Promise.reject('stop this design before changing where it runs');
            d.runtime = args.runtime;
            return Promise.resolve({ ...d });
          }
          case 'vault_init':
            return Promise.resolve('/tmp/vault');
          default:
            return real(cmd, args);
        }
      };
    }, 5);
  };
}

async function openDesigner(page, designs = []) {
  await page.addInitScript(fakeDesigner(designs), designs);
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((project) => {
    const host = document.createElement('div');
    host.id = 'designer-test-host';
    document.body.appendChild(host);
    window.xnautDesigner.mount(host, project);
  }, PROJECT);
  await expect(page.locator('#designer-test-host .dsg')).toBeVisible();
}

// The runtime the header is actually showing, read the way a user reads it.
const activeRuntime = (page) => page.locator('.dsg-rt.active').first().innerText();

test('a new design defaults to local and says where it runs', async ({ page }) => {
  await openDesigner(page);

  await page.locator('#designer-test-host [data-new]').first().click();
  await page.locator('#designer-test-host [data-name]').fill('Marketing site');
  await page.locator('#designer-test-host [data-create]').click();

  // The canvas opens on create, and the header carries the runtime.
  await expect(page.locator('.dsg-rtgroup')).toBeVisible();
  expect(await activeRuntime(page), 'a new design must default to Local').toBe('Local');

  const created = await page.evaluate(() =>
    window.__designerCalls.find((c) => c.cmd === 'designer_create'));
  expect(created, 'nothing was created').toBeTruthy();
  expect(created.args.name).toBe('Marketing site');
});

test('the runtime switch changes the selection when the design is not running', async ({ page }) => {
  await openDesigner(page, [{
    slug: 'idle-one', name: 'Idle', kind: 'website',
    created_at_ms: Date.now(), updated_at_ms: Date.now(), archived: false,
    sandbox_id: '', public_url: '', sandbox_expires_ms: 0, messages: [],
    session_id: '', runtime: 'local', local_port: 0,
  }]);

  await page.locator('#designer-test-host .dsg-card[data-open]').first().click();
  expect(await activeRuntime(page)).toBe('Local');

  await page.locator('.dsg-rt[data-rt="sandbox"]').click();
  await expect(page.locator('.dsg-rt.active')).toHaveText('Sandbox');

  const sent = await page.evaluate(() =>
    window.__designerCalls.find((c) => c.cmd === 'designer_set_runtime'));
  expect(sent.args.runtime).toBe('sandbox');
});

test('the switch survives reopening the design', async ({ page }) => {
  await openDesigner(page, [{
    slug: 'idle-one', name: 'Idle', kind: 'website',
    created_at_ms: Date.now(), updated_at_ms: Date.now(), archived: false,
    sandbox_id: '', public_url: '', sandbox_expires_ms: 0, messages: [],
    session_id: '', runtime: 'local', local_port: 0,
  }]);

  await page.locator('#designer-test-host .dsg-card[data-open]').first().click();
  await page.locator('.dsg-rt[data-rt="sandbox"]').click();
  await expect(page.locator('.dsg-rt.active')).toHaveText('Sandbox');

  // Back out and in again. The choice lives in design.json rather than
  // localStorage precisely so that spin-up, publish and stop can all read it,
  // so a selection that does not survive reopening is a real defect.
  await page.locator('.dsgc-back').click();
  await page.locator('#designer-test-host .dsg-card[data-open]').first().click();
  expect(await activeRuntime(page), 'the runtime was not persisted').toBe('Sandbox');
});

test('a running design refuses the switch with a reason, rather than ignoring it', async ({ page }) => {
  // Live by the rule in designer-panel.js:32: a local design with a public_url.
  await openDesigner(page, [{
    slug: 'running-one', name: 'Running', kind: 'website',
    created_at_ms: Date.now(), updated_at_ms: Date.now(), archived: false,
    sandbox_id: '', public_url: 'http://127.0.0.1:5310', sandbox_expires_ms: 0,
    messages: [], session_id: '', runtime: 'local', local_port: 5310,
  }]);

  await page.locator('#designer-test-host .dsg-card[data-open]').first().click();
  expect(await activeRuntime(page)).toBe('Local');

  const sandboxBtn = page.locator('.dsg-rt[data-rt="sandbox"]');
  await expect(sandboxBtn, 'a live design must not accept a runtime change').toBeDisabled();

  // Disabled is not enough on its own. Silently doing nothing is the failure
  // this scenario names, so the reason has to be readable.
  await expect(sandboxBtn).toHaveAttribute(
    'title', /Stop the sandbox first to change where this design runs/);

  expect(await activeRuntime(page), 'the runtime changed anyway').toBe('Local');
});
