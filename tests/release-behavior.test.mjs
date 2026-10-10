import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { checkNative, checkBrowser, checkReceipt, nativeSuites, requiredNative, browserFiles } from '../scripts/release-behavior.mjs';

test('native evidence requires actual passing executions of every required behavior', () => {
  const valid = 'test queue::restart ... ok\ntest queue::approval ... ok\n';
  assert.equal(checkNative(valid, ['queue::'], ['queue::restart']).length, 2);
  for (const output of ['', 'running 0 tests\ntest result: ok.', 'test queue::restart ... ignored',
    'test queue::restart ... FAILED', 'test queue::approval ... ok']) {
    assert.throws(() => checkNative(output, ['queue::'], ['queue::restart']));
  }
  assert.throws(() => checkNative(valid, ['missing::'], []));
});

const browser = () => ({ suites: [{ file: 'workflow.spec.mjs', specs: [{ title: 'approve and run',
  tests: [{ status: 'expected', expectedStatus: 'passed', results: [{ status: 'passed' }] }],
}] }] });

test('browser evidence refuses missing suites, empty runs and runner errors', () => {
  assert.equal(checkBrowser(browser(), ['workflow.spec.mjs']).length, 1);
  assert.throws(() => checkBrowser(browser(), ['missing.spec.mjs']));
  assert.throws(() => checkBrowser({ suites: [] }, []));
  assert.throws(() => checkBrowser({ ...browser(), errors: [{ message: 'unavailable' }] }, []));
});

test('skipped, expected-failure and flaky browser tests never authorize release', () => {
  for (const status of ['skipped', 'failed', 'timedOut', 'interrupted']) {
    const report = browser();
    report.suites[0].specs[0].tests[0].results[0].status = status;
    assert.throws(() => checkBrowser(report, []));
  }
  for (const mutate of [
    t => { t.results = []; },
    t => { t.expectedStatus = 'failed'; },
    t => { t.status = 'flaky'; },
    t => { t.results.unshift({ status: 'failed' }); },
  ]) {
    const report = browser(); mutate(report.suites[0].specs[0].tests[0]);
    assert.throws(() => checkBrowser(report, []));
  }
});

test('manual release cannot reuse partial, dirty, missing or different-commit evidence', () => {
  const receipt = {
    status: 'passed', scope: 'native-and-browser', source_commit: 'candidate', working_tree_changes: '',
    started_at: '2026-01-01T00:00:00Z', finished_at: '2026-01-01T00:01:00Z',
    native: [...nativeSuites.map(suite => `${suite}fixture`), ...requiredNative],
    browser: browserFiles.map(file => ({ file, title: 'fixture' })),
  };
  assert.doesNotThrow(() => checkReceipt(receipt, 'candidate'));
  for (const changes of [
    { status: 'partial' }, { scope: 'native-only' }, { working_tree_changes: ' M source.rs' },
    { source_commit: 'yesterday' }, { finished_at: null }, { native: [] }, { browser: [] },
  ]) assert.throws(() => checkReceipt({ ...receipt, ...changes }, 'candidate'));
});

// cfg(unix) Rust tests have no Windows execution to attest. Keep the platform
// requirement aligned while retaining this live worker protocol gate on macOS.
test('publisher protocol evidence is required only where its Rust test is compiled', () => {
  const suite = 'repository_transfer::tests::repository_publisher_protocol_suite';
  for (const platform of ['darwin', 'win32']) {
    const script = `Object.defineProperty(process, 'platform', {value:${JSON.stringify(platform)}});
      const m = await import('./scripts/release-behavior.mjs');
      console.log(JSON.stringify([m.nativeSuites.includes(${JSON.stringify(suite)}),m.requiredNative.includes(${JSON.stringify(suite)})]));`;
    const result = spawnSync(process.execPath, ['--input-type=module', '-e', script], {encoding:'utf8'});
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), [platform !== 'win32', platform !== 'win32']);
  }
});
