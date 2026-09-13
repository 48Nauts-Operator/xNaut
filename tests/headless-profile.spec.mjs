import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';

// This spec used to evaluate FOUR hand-built agent command lines — three in
// JavaScript, one in a bash heredoc inside nautloom.rs — and run each through
// bash to check its argv. There are not four any more (XNAUT-266): every caller
// now asks `agent_headless_command`, and the argv check moved to Rust, beside
// the one builder, as `agents::tests::every_caller_gets_an_argv_the_agent_can_
// actually_parse`.
//
// What is left for this file is the half that is still a frontend property:
// that no pane has grown its own copy back, and that each one asks for the
// options it actually needs. A regrown copy would pass the Rust argv test
// perfectly and still be a fourth launch path.
const read = (file) => readFileSync(new URL('../' + file, import.meta.url), 'utf8');
const panes = {
  'project-management-panel.js': read('src/js/project-management-panel.js'),
  'designer-agent.js': read('src/js/designer-agent.js'),
  'build-sandbox.js': read('src/js/build-sandbox.js'),
};

// The words that only appear in a hand-built agent invocation. `--settings` is
// in the list because the hook rule (XNAUT-107) is the part that was quietly
// copied four ways and is the most expensive to get wrong.
const HAND_BUILT = [
  'HEADLESS_CLAUDE_SETTINGS',
  'disableAllHooks',
  'dangerously-bypass-approvals-and-sandbox',
  'dangerously-skip-permissions',
  '--strict-mcp-config',
  '--output-format stream-json',
];

for (const [name, source] of Object.entries(panes)) {
  test(`${name} does not build its own agent command line`, () => {
    // Comments may still discuss these flags; code may not contain them.
    const code = source
      .split('\n')
      .filter((line) => !/^\s*(\/\/|\*|\/\*)/.test(line))
      .join('\n');
    for (const token of HAND_BUILT) {
      expect(code, `${name} still hand-builds an agent command (${token})`).not.toContain(token);
    }
    expect(code, `${name} never asks the one launcher`).toContain('agent_headless_command');
  });
}

test('each caller asks for the options its run actually needs', () => {
  const pm = panes['project-management-panel.js'];
  const designer = panes['designer-agent.js'];
  const sandbox = panes['build-sandbox.js'];

  // Personas and the Designer resume a session and run with no user MCP
  // servers; the planner isolates MCP but never resumes; the Build stage's
  // sandbox slice does neither. Those four shapes are what the Rust argv test
  // enumerates. The fourth used to live in multiagent-pane.js; that pane is
  // gone (XNAUT-354) and its engine moved to build-sandbox.js, which is the
  // only caller it ever had.
  expect(pm).toMatch(/headlessAgentCommand\(model, '\.loom-goal\.txt', \{ resume: opts\.resume, isolateMcp: true \}\)/);
  expect(pm).toMatch(/headlessAgentCommand\('', '\.loom-goal\.txt', \{ isolateMcp: true \}\)/);
  expect(designer).toMatch(/headlessAgentCommand\(model, '\.loom-goal\.txt', \{ resume: design\.session_id, isolateMcp: true \}\)/);
  expect(sandbox).toMatch(/headlessAgentCommand\(build\.model, '\.build-goal\.txt'\)/);
});

test('the NautLoom sandbox runner is handed its command rather than building one', () => {
  const rust = read('src-tauri/src/nautloom.rs');
  const runner = rust.match(/const AGENT_RUNNER: &str = r#"([\s\S]+?)"#;/)[1];
  // The `case "$MODEL" in codex*)` block is gone; the line and the session name
  // arrive staged, built by the same code the fleet launcher uses.
  expect(runner).not.toContain('case "$MODEL" in');
  expect(runner).not.toContain('disableAllHooks');
  expect(runner).toContain('.loom-agent-cmd.txt');
  expect(runner).toContain('.loom-session.txt');
  // A missing staged command is a bug in loom_run, not something to guess
  // around: the runner must refuse rather than launch nothing.
  expect(runner).toMatch(/\[ -n "\$AGENT" \] \|\| \{[^}]*exit 1/);
  // Both control files are cleaned up and kept out of the diff.
  expect(rust).toContain('.loom-agent-cmd.txt .loom-session.txt');
  expect(rust).toContain('":(exclude).loom-agent-cmd.txt"');
});
