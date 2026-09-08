import { test, expect } from '@playwright/test';
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import vm from 'node:vm';

// Evaluate the production command expressions, then let bash parse their argv.
// This catches broken JSON quoting, including the second NautLoom shell pass.
const read = (file) => readFileSync(new URL('../' + file, import.meta.url), 'utf8');
const pm = read('src/js/project-management-panel.js');
const designer = read('src/js/designer-agent.js');
const multi = read('src/js/multiagent-pane.js');
const rust = read('src-tauri/src/nautloom.rs');
function settings(source) {
  return source.match(/const HEADLESS_CLAUDE_SETTINGS = (`[^`]+`);/)[1];
}
function expression(source, name) {
  return source.match(new RegExp('const ' + name + ' = ([\\s\\S]+?);'))[1];
}
function command(source, expr, model = 'test-model') {
  return vm.runInNewContext(`const HEADLESS_CLAUDE_SETTINGS = ${settings(source)}; (${expr})`, {
    model, mf: ' --model test-model', resumeFlag: ' --resume test-session',
    resume: ' --resume test-session', PATHX: '',
    mcpFlags: ` --strict-mcp-config --mcp-config '{"mcpServers":{}}'`,
    mcp: ` --strict-mcp-config --mcp-config '{"mcpServers":{}}'`,
  });
}
const capture = 'claude() { printf "%s\\0" "$@"; }\n';
function argv(script, context = {}) {
  const dir = mkdtempSync(join(process.cwd(), '.xnaut-headless-test-'));
  try {
    for (const file of ['.loom-goal.txt', '.build-goal.txt']) {
      writeFileSync(join(dir, file), 'A goal with "quotes" and $literal text');
    }
    const env = { ...process.env, XNAUT_VETO_URL: '', XNAUT_HOOK_TOKEN: '', ...context };
    return execFileSync('bash', ['-c', capture + script], { cwd: dir, env })
      .toString().split('\0').filter(Boolean);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
const cases = [
  ['persona and Validator', () => command(pm, expression(pm, 'agentLine')), true],
  ['Designer', () => command(designer, expression(designer, 'agentLine')), true],
  ['planner', () => command(pm, pm.match(/script: (PATHX \+ 'claude -p'[^\n]+),/)[1]), true],
  ['multiagent worker', () => command(multi, expression(multi, 'agent')), false],
];
for (const [name, build, isolatedMcp] of cases) {
  test(`${name} isolates unattended hooks and preserves managed hooks`, () => {
    for (const context of [{}, { XNAUT_VETO_URL: 'http://fixture.invalid/veto' }, { XNAUT_HOOK_TOKEN: 'fixture-token' }]) {
      const args = argv(build(), context);
      expect(args).toContain('-p');
      expect(args.at(-1)).toBe('A goal with "quotes" and $literal text');
      const index = args.indexOf('--settings');
      expect(index).toBeGreaterThan(-1);
      expect(JSON.parse(args[index + 1])).toEqual(Object.keys(context).length ? {} : { disableAllHooks: true });
      expect(args.includes('--strict-mcp-config')).toBe(isolatedMcp);
      if (isolatedMcp) expect(JSON.parse(args[args.indexOf('--mcp-config') + 1])).toEqual({ mcpServers: {} });
      if (name === 'Designer' || name === 'persona and Validator') {
        expect(args[args.indexOf('--model') + 1]).toBe('test-model');
        expect(args[args.indexOf('--resume') + 1]).toBe('test-session');
      }
    }
  });
}

test('NautLoom settings survive the nested AGENT shell', () => {
  const runner = rust.match(/const AGENT_RUNNER: &str = r#"([\s\S]+?)"#;/)[1];
  // Execute the production case statement, then its nested command through bash.
  const branch = runner.slice(runner.indexOf('case "$MODEL" in'), runner.indexOf('\nesac') + 6);
  for (const context of [{}, { XNAUT_VETO_URL: 'http://fixture.invalid/veto' }, { XNAUT_HOOK_TOKEN: 'fixture-token' }]) {
    const args = argv(`MODEL=test-model\n${branch}\nexport -f claude\nbash -c "\${AGENT% 2>&1*}"`, context);
    expect(JSON.parse(args[args.indexOf('--settings') + 1])).toEqual(Object.keys(context).length ? {} : { disableAllHooks: true });
    expect(args[args.indexOf('--model') + 1]).toBe('test-model');
    expect(args).not.toContain('--strict-mcp-config');
    expect(args.at(-1)).toBe('A goal with "quotes" and $literal text');
  }
});

test('conversational Agent Space and interactive terminals retain hooks', () => {
  const agents = read('src-tauri/src/agents.rs');
  expect(agents).not.toContain('disableAllHooks');
  expect(agents).toContain('codex_veto_flags()');
  expect(agents).toContain('XNAUT_VETO_URL');
});
