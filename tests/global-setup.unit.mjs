import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { ensureBrowser, installedPlaywrightCli } from './global-setup.mjs';

test('installer resolves the installed package CLI rather than an unexported subpath', () => {
  const result = spawnSync(process.execPath, [installedPlaywrightCli(), '--version'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^Version \d+\./);
});

test('usable browser needs no install', async () => {
  let installs = 0;
  await ensureBrowser(async () => {}, () => { installs++; });
  assert.equal(installs, 0);
});

test('missing browser installs once and requires a successful real probe', async () => {
  let probes = 0, installs = 0;
  await ensureBrowser(async () => {
    if (++probes === 1) throw new Error("Executable doesn't exist at /persistent/chromium");
  }, () => { installs++; });
  assert.equal(probes, 2);
  assert.equal(installs, 1);
});

test('installer success without a browser stays unavailable without looping', async () => {
  let probes = 0, installs = 0;
  await assert.rejects(ensureBrowser(async () => {
    probes++; throw new Error("Executable doesn't exist at /persistent/chromium");
  }, () => { installs++; }), /Verifier unavailable: Playwright browser setup failed/);
  assert.equal(probes, 2);
  assert.equal(installs, 1);
});

test('disk-full installation and browser startup faults cannot become passes', async () => {
  await assert.rejects(ensureBrowser(async () => {
    throw new Error('No Playwright browser is installed');
  }, () => { throw new Error('ENOSPC'); }), /Verifier unavailable: Playwright.*ENOSPC/);
  let installs = 0;
  await assert.rejects(ensureBrowser(async () => {
    throw new Error('Required system library missing');
  }, () => { installs++; }), /Verifier unavailable: Playwright could not start/);
  assert.equal(installs, 0);
});
