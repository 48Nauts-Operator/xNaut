// Mutation check: prove each smoke script would FAIL if the code it covers broke.
//
// A green check proves nothing on its own. This breaks the covered code on
// purpose and asserts the check goes red. A mutation that stays green means the
// check is decoration and buys confidence it did not earn.
//
// Everything runs against an rsync'd COPY of the tree. Never mutate the worktree:
// André's dev app is served from one, and a JS or Rust edit restarts his session.
//
// Run:  node scripts/mutation-check.cjs           (node checks, seconds)
//       node scripts/mutation-check.cjs --all     (adds cargo, cold build first time)
//
// Adding a case is one entry: what to break, and which check must notice.

const { execSync } = require('node:child_process');
const { mkdtempSync, readFileSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join } = require('node:path');

const REPO = join(__dirname, '..');

const MUTATIONS = [
  {
    name: 'XNAUT-17 feature track falls back to standard',
    check: 'node scripts/flow-tracks-smoke.cjs',
    file: 'src/js/project-management-panel.js',
    from: "if (project.flow_type === 'feature') return FEATURE_STAGES;",
    to: "if (project.flow_type === 'feature') return STANDARD_STAGES;",
  },
  {
    name: 'XNAUT-24 several MAX accounts collapse to one',
    check: 'node scripts/usage-accounts-smoke.cjs',
    file: 'src/js/usage-footer.js',
    from: 'const wanted = accounts.length > 1 ? accounts : [null];',
    to: 'const wanted = [null];',
  },
  {
    name: 'build slices stop requiring a port id',
    check: 'node scripts/slice-ports-smoke.cjs',
    file: 'src/js/project-management-panel.js',
    from: '.filter((p) => p.id)',
    to: '.filter(() => true)',
  },
  {
    name: 'XNAUT-93 a launch counts as a start again',
    check: 'node scripts/integrator-launch-smoke.cjs',
    file: 'src/js/project-management-panel.js',
    from: 'if (sessionUp && (await probes.agentAlive(cwd))) return;',
    to: 'if (sessionUp) return;',
  },
  {
    name: 'decisions read the worktree branch as the project',
    check: 'node scripts/decisions-view-smoke.cjs',
    file: 'src/js/right-pane-decisions.js',
    from: "const i = parts.indexOf('.worktrees');",
    to: 'const i = -1;',
  },
  {
    name: 'agent loop stops deduplicating node ids',
    check: 'node scripts/loop-compiler-smoke.cjs',
    file: 'src/js/chat-panel.js',
    from: 'while (ids.has(id)) id = `${id}-${index + 1}`;',
    to: '',
  },
  {
    name: 'a right-pane view loses its icon',
    check: 'npx playwright test console-clean',
    file: 'src/js/right-pane.js',
    from: '\n    files:',
    to: '\n    filez:',
  },
  {
    name: 'XNAUT-38 verify verdict uses the record vocabulary, not the port',
    check: 'cargo test --bin xnaut loops::tests::sandbox_bridge',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/sandbox_verify.rs',
    from: '        "success"\n',
    to: '        "passed"\n',
  },
];

const withRust = process.argv.includes('--all');
const cases = MUTATIONS.filter((m) => withRust || !m.slow);

const root = mkdtempSync(join(tmpdir(), 'mutation-check-'));
execSync(
  `rsync -a --exclude .git --exclude target --exclude node_modules ${JSON.stringify(REPO)}/ ${JSON.stringify(root)}/`,
  { stdio: 'inherit' },
);
// Linked, not copied: Playwright's browsers make node_modules far too big to rsync.
execSync(`ln -s ${JSON.stringify(join(REPO, 'node_modules'))} ${JSON.stringify(join(root, 'node_modules'))}`);

// Its own target dir: sharing the worktree's starves the running `cargo tauri
// dev` watcher, and a fresh copy each run would rebuild Tauri from cold.
const TARGET = join(tmpdir(), 'mutation-check-target');

const run = (m) => {
  try {
    execSync(m.check, {
      cwd: join(root, m.cwd || '.'),
      stdio: 'pipe',
      env: { ...process.env, CARGO_TARGET_DIR: TARGET },
    });
    return true;
  } catch {
    return false;
  }
};

let bad = 0;
for (const m of cases) {
  const path = join(root, m.file);
  const pristine = readFileSync(path, 'utf8');

  if (!pristine.includes(m.from)) {
    console.log(`NOT FOUND  ${m.name}\n           ${m.file} no longer contains the mutated text`);
    bad += 1;
    continue;
  }
  if (!run(m)) {
    console.log(`BASELINE   ${m.name}\n           ${m.check} already fails unmutated`);
    bad += 1;
    continue;
  }

  writeFileSync(path, pristine.replace(m.from, m.to));
  const survived = run(m);
  writeFileSync(path, pristine);

  console.log(`${survived ? 'SURVIVED  ' : 'caught    '} ${m.name}`);
  if (survived) bad += 1;
}

console.log(
  `\n${cases.length - bad}/${cases.length} mutations caught${withRust ? '' : '  (cargo skipped, --all to include)'}`,
);
process.exit(bad ? 1 : 0);
