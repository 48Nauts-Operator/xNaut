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
    name: 'a failed HSM signing reports as success',
    check: 'node scripts/securosys-attest-smoke.cjs',
    file: 'mcp/securosys-attest.py',
    from: '"isError": True',
    to: '"isError": False',
  },
  {
    name: 'a dead zellij session reappears in the sidebar',
    check: 'node scripts/sidebar-sessions-smoke.cjs',
    file: 'src/js/sidebar.js',
    from: '        if (s.exited) return false;',
    to: '',
  },
  {
    name: 'XNAUT-187 a voice command escapes the ACL',
    check: 'node scripts/voice-dictation-smoke.cjs',
    file: 'src-tauri/permissions/default.toml',
    from: 'commands.allow = ["voice_stop"]',
    to: 'commands.allow = ["voice_stopped"]',
  },
  {
    name: 'XNAUT-187 the dictate button stops reaching the backend',
    check: 'node scripts/voice-dictation-smoke.cjs',
    file: 'src/js/voice-dictate.js',
    from: "await invoke('voice_start');",
    to: "await Promise.resolve();",
  },
  {
    // The shipped bug: a mic in the chat pane only, so the composer he types
    // into had none and the feature read as missing.
    name: 'XNAUT-187 the Agent Space composer loses its microphone',
    check: 'node scripts/voice-dictation-smoke.cjs',
    file: 'src/js/agent-space.js',
    from: '<button class="as-mic" data-dictate',
    to: '<button class="as-mic" data-nothing',
  },
  {
    name: 'XNAUT-187 recorded audio is described as 16 kHz when it is not',
    check: 'cargo test --bin xnaut voice::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/voice.rs',
    from: '    out.extend_from_slice(&sample_rate.to_le_bytes());',
    to: '    out.extend_from_slice(&44_100u32.to_le_bytes());',
  },
  {
    // The shipped bug: a fixed menu placed from a raw clientX lands at
    // click x zoom, a third of the screen below the row that was clicked.
    name: 'a context menu ignores the interface zoom again',
    check: 'npx playwright test tests/files-menu.spec.mjs',
    file: 'src/js/app.js',
    from: '  el.style.left = `${Math.max(0, Math.min(x / zoom, maxLeft))}px`;',
    to: '  el.style.left = `${Math.max(0, Math.min(x, maxLeft))}px`;',
  },
  {
    // Same root cause, other symptom: xterm keeps measuring cells unzoomed, so
    // a click selected a row further down the further down the screen it was.
    name: 'terminals stop cancelling the interface zoom',
    check: 'npx playwright test tests/terminal-selection.spec.mjs',
    file: 'src/js/app.js',
    from: "    st.textContent = zoom === 1 ? '' : `.terminal-output{zoom:${(1 / zoom).toFixed(6)};}`;",
    to: "    st.textContent = '';",
  },
  {
    // The middle tier is only real if the script acts on it. The shim matches
    // the wire shape with `case`, so a changed tag reads as "not a deny" and
    // silently allows.
    name: 'XNAUT-189 the shim stops recognising an ask',
    check: 'node scripts/veto-ask-smoke.cjs',
    file: 'src-tauri/scripts/hooks/xnaut-veto.sh',
    from: `  *'"decision":"ask"'*)`,
    to: `  *'"decision":"asked"'*)`,
  },
  {
    // Interrupting the owner to confirm something the policy already forbids
    // is the worst of both designs.
    name: 'XNAUT-189 ask is consulted before deny',
    check: 'cargo test --bin xnaut veto::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/veto.rs',
    from: '    if let Some(rule) = first_match(&policy.deny, request) {\n        return Decision::Deny { reason: rule.reason.clone() };\n    }\n',
    to: '',
  },
  {
    // Two agents on one file is the case nobody detects today, and the signal
    // only exists because the veto sees a write before it happens.
    name: 'XNAUT-190 a second agent on the same file goes unnoticed',
    check: 'cargo test --bin xnaut claims::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/claims.rs',
    from: '    if previous.agent.eq_ignore_ascii_case(agent) {\n        return None;\n    }',
    to: '    return None;',
  },
  {
    // A finished run that does not say what it changed is a run you have to
    // reopen to review.
    name: 'XNAUT-190 the completion shape stops being taught',
    check: 'cargo test --bin xnaut foundation::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/foundation.rs',
    from: '  `"files": ["path/one.rs", "path/two.js"]` alongside the summary. A finished',
    to: '  the summary. A finished',
  },
  {
    // A renderer exported and never called is a feature that exists only in
    // the source. The veto editor shipped that way for one commit.
    name: 'a settings renderer loses its only call site',
    check: 'npx playwright test tests/console-clean.spec.mjs',
    file: 'src/js/app.js',
    from: "    window.xnautRenderVetoSettings(document.getElementById('veto-settings-host'));",
    to: '',
  },
  {
    // The silent fallback cost four days twice. A reply that lost its tools
    // has to say so, and it has to say which model and what the upstream said.
    name: 'XNAUT-195 a tool-less reply stops saying so',
    check: 'cargo test --bin xnaut the_tool_failure_notice',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/agent_profiles.rs',
    from: '"\\n\\n---\\n**Answered without tools.** `{model}` could not run a tool call, so nothing was \\',
    to: '"`{model}` says: \\',
  },
  {
    name: 'XNAUT-194 the chat pane goes back to a turn with no tools',
    check: 'node scripts/one-turn-path-smoke.cjs',
    file: 'src/js/chat-panel.js',
    from: "    const chatCommand = 'chat_send_tools';",
    to: "    const chatCommand = 'chat_send';",
  },
  {
    name: 'XNAUT-196 the probe stops carrying tools',
    check: 'node scripts/tool-support-smoke.cjs',
    file: 'src-tauri/src/tool_support.rs',
    from: '        "tools": [{',
    to: '        "no_tools": [{',
  },
  {
    name: 'XNAUT-197 a profile store stops being backfilled',
    check: 'cargo test --bin xnaut agent_profiles::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/agent_profiles.rs',
    from: '            profile.accent_color = DEFAULT_ACCENT_COLOR.to_string();\n            changed = true;',
    to: '            changed = false;',
  },
  {
    // The veto read the wrong field names, so no rule could ever match and
    // nothing errored: every field is serde(default).
    name: 'XNAUT-132 the veto stops reading the harness envelope',
    check: 'cargo test --bin xnaut veto::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/veto.rs',
    from: '    #[serde(default, alias = "tool_name")]',
    to: '    #[serde(default)]',
  },
  {
    // A frontend call to a command nobody registered fails at runtime and is
    // swallowed by the nearest catch. Four were shipping.
    name: 'XNAUT-198 the frontend calls a command that does not exist',
    check: 'cargo test --bin xnaut every_command_the_frontend',
    cwd: 'src-tauri',
    slow: true,
    file: 'src/js/app.js',
    from: "      await invoke('browser_pane_destroy', { label: terminal.label })",
    to: "      await invoke('browser_pane_dispose', { label: terminal.label })",
  },
  {
    // A tool that answers with a bare payload leaves the agent inferring what
    // happened and what it can do next. That is the whole ticket.
    name: 'XNAUT-193 an MCP tool answers outside the envelope',
    check: 'cargo test --bin xnaut agent_hooks::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/agent_hooks.rs',
    from: '        Ok(data) => success_envelope(name, data),',
    to: '        Ok(data) => data,',
  },
  {
    // The shipped bug: the authenticated session was bound to _ssh_handle and
    // dropped, so the channel it should have carried never existed.
    name: 'XNAUT-200 the SSH connection is dropped the moment it is made',
    check: 'node scripts/ssh-interactive-smoke.cjs',
    file: 'src-tauri/src/ssh.rs',
    from: '        channel: Arc::clone(&channel),',
    to: '',
  },
  {
    // Nothing emitted ssh-output-<id>, so the terminal was blank and looked
    // like a server that had nothing to say.
    name: 'XNAUT-200 SSH output stops reaching the terminal',
    check: 'node scripts/ssh-interactive-smoke.cjs',
    file: 'src-tauri/src/ssh.rs',
    from: '&format!("ssh-output-{session_id}"),',
    to: '&format!("ssh-out-{session_id}"),',
  },
  {
    // The editor wrote privateKey, the backend read key_path, and serde
    // dropped it: every key profile failed as if it had no credential at all.
    name: 'XNAUT-200 a key profile loses its key on the way to the backend',
    check: 'cargo test --bin xnaut ssh::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/ssh.rs',
    from: '#[serde(rename_all = "camelCase")]\npub struct SshConfig {',
    to: 'pub struct SshConfig {',
  },
  {
    // A non-blocking channel takes what it feels like. Forgetting how far the
    // write got resends a paste from the start.
    name: 'XNAUT-200 a short SSH write loses its place',
    check: 'cargo test --bin xnaut ssh::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/ssh.rs',
    from: '        rest = &rest[written..];',
    to: '        rest = &rest[..0];',
  },
  {
    // Both ways into the SSH modal showed it without rendering the list, so
    // there was no Connect button on screen to press.
    name: 'XNAUT-200 the SSH modal opens with no profiles in it',
    check: 'npx playwright test tests/ssh-interactive.spec.mjs',
    file: 'src/js/app.js',
    from: "else if (action === 'ssh') { loadSSHProfiles(); showSSHModal(); }",
    to: "else if (action === 'ssh') { loadSSHProfiles(); showModal('ssh-modal'); }",
  },
  {
    // Raw payload.data into xterm prints the base64, which is how a working
    // channel would still look broken.
    name: 'XNAUT-200 SSH output stops being decoded',
    check: 'npx playwright test tests/ssh-interactive.spec.mjs',
    file: 'src/js/app.js',
    // Anchored on the atob line: the same decode-and-write pair exists in the
    // PTY scrollback restore, and a shorter needle mutates that one instead.
    from: "    const binary = atob(event.payload.data);",
    to: '    const binary = event.payload.data;',
  },
  {
    // The chips were stored on the profile and read by nobody while the tab
    // said "Enforced at dispatch". The prompt is the only place the list can
    // be true, so it has to actually arrive there.
    name: 'XNAUT-202 the collaborators the owner picked stop reaching the prompt',
    check: 'cargo test --bin xnaut composer::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/composer.rs',
    from: '    out.push_str(&collaborators_block(profile));\n',
    to: '',
  },
  {
    name: 'XNAUT-202 the Collaborators tab claims enforcement again',
    check: 'node scripts/surfaces-honest-smoke.cjs',
    file: 'src/js/agent-space.js',
    from: 'Advisory: nothing blocks a hand-off',
    to: 'Enforced at dispatch. Nothing blocks a hand-off',
  },
  {
    // run_turn opens every plugin the agent holds, so this sentence has been
    // false since mcp_client::open_for landed.
    name: 'XNAUT-202 the plugin library says a chat turn skips plugins',
    check: 'node scripts/surfaces-honest-smoke.cjs',
    file: 'src/js/plugins-panel.js',
    from: 'A chat turn opens them as well, so the agent can call their tools while it answers.',
    to: 'Chat turns do not use plugins.',
  },
  {
    // generate_summary is a Rust METHOD, never a serialised field, so this
    // reads undefined at runtime and the fallback one-liner always wins.
    name: 'XNAUT-202 the work-log panel renders a Rust method name again',
    check: 'node scripts/surfaces-honest-smoke.cjs',
    file: 'src/js/app.js',
    from: '          html = marked.parse(summary);',
    to: '          html = marked.parse(session.generate_summary || summary);',
  },
  {
    // worklog_stop clears the active session, so the one moment anybody wants
    // a summary is the one moment the old command could not produce one.
    name: 'XNAUT-202 a stopped work session cannot be summarised',
    check: 'cargo test --bin xnaut worklog::',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/worklog.rs',
    from: '        Some(id) => load_session(id.trim()),',
    to: '        Some(_) => Err("No active work session".to_string()),',
  },
  {
    // ledger_recent was registered and ACL-allowed with no caller at all:
    // every refusal was written to disk and shown nowhere.
    name: 'XNAUT-202 the decision ledger loses its only reader',
    check: 'node scripts/ledger-view-smoke.cjs',
    file: 'src/js/veto-settings.js',
    from: '    await loadLedger();\n',
    to: '',
  },
  {
    name: 'XNAUT-202 the footer stops asking what the last codex run cost',
    check: 'node scripts/codex-spend-footer-smoke.cjs',
    file: 'src/js/usage-footer.js',
    from: "      invoke('codex_spend', { limit: 1 }),",
    to: '      Promise.resolve(null),',
  },
  {
    // A model with no known price reading as free is the one wrong answer:
    // the estimate is missing, not zero.
    name: 'XNAUT-202 an unpriced codex model reads as a free one',
    check: 'node scripts/codex-spend-footer-smoke.cjs',
    file: 'src/js/usage-footer.js',
    from: "    const usd = spend && spend.length && typeof spend[0].cost_usd === 'number' ? spend[0].cost_usd : null;",
    to: '    const usd = spend && spend.length ? spend[0].cost_usd || 0 : null;',
  },
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
    name: 'XNAUT-19 node autodetect plans scripts the repo does not have',
    check: 'cargo test --bin xnaut sandbox_verify::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/sandbox_verify.rs',
    from: '            .is_some()',
    to: '            .is_some() || true',
  },
  {
    name: 'XNAUT-19 an explicit verify.json loses to autodetect',
    check: 'cargo test --bin xnaut sandbox_verify::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/sandbox_verify.rs',
    from: '    let config = if explicit.is_file() {',
    to: '    let config = if explicit.is_file() && !repo_dir.join("package.json").is_file() {',
  },
  {
    name: 'XNAUT-14 a vault path may climb out of its scope',
    check: 'cargo test --bin xnaut vault::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/vault.rs',
    from: "    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|c| c == \"..\") {",
    to: '    if rel.is_empty() {',
  },
  {
    name: 'XNAUT-128 a declared type stops being checked',
    check: 'cargo test --bin xnaut build_dag::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/build_dag.rs',
    from: '        "object" => v.is_object(),',
    to: '        "object" => true,',
  },
  {
    name: 'XNAUT-128 an explicit null counts as a produced value',
    check: 'cargo test --bin xnaut build_dag::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/build_dag.rs',
    from: '            None | Some(serde_json::Value::Null) => Some(PortIssue {',
    to: '            None => Some(PortIssue {',
  },
  {
    // A note on half a fenced block anchors to lines that mean nothing alone.
    name: 'XNAUT-192 a fenced block in the plan splits into pieces',
    check: 'npx playwright test tests/plan-canvas.spec.mjs',
    file: 'src/js/plan-pane.js',
    from: '        while (i < lines.length && !isFence(lines[i])) i++;',
    to: '        while (i < lines.length && lines[i].trim() !== \'\') i++;',
  },
  {
    // The verdict is the whole feature. Sending the wrong one unblocks an
    // agent to build a plan its owner just rejected.
    name: 'XNAUT-192 Request changes answers the agent with approved',
    check: 'npx playwright test tests/plan-canvas.spec.mjs',
    file: 'src/js/plan-pane.js',
    from: "    btnChanges.onclick = () => decide('denied');",
    to: "    btnChanges.onclick = () => decide('approved');",
  },
  {
    name: 'XNAUT-192 an unanswered plan reads as approved',
    check: 'cargo test --bin xnaut plan_review::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/plan_review.rs',
    from: '        _ => "pending",',
    to: '        _ => "approved",',
  },
  {
    // The shipped bug: nothing ever inserted a tap, so the phone's websocket
    // found None and closed for every session. Unit tests that build a
    // MobileTap by hand cannot see it.
    name: 'XNAUT-201 a created session gets no mobile tap',
    check: 'cargo test --bin xnaut pty::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/pty.rs',
    from: '    state\n        .mobile_taps\n        .lock()\n        .await\n        .insert(session_id.to_string(), MobileTap::new(cols, rows));\n',
    to: '',
  },
  {
    // A desktop resize that skips the tap leaves a later phone attach building
    // its terminal at the width the session was born with.
    name: 'XNAUT-201 a desktop resize stops reaching the mobile tap',
    check: 'cargo test --bin xnaut pty::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/pty.rs',
    from: '    if let Some(tap) = state.mobile_taps.lock().await.get_mut(session_id) {\n        tap.cols = cols;\n        tap.rows = rows;\n    }\n',
    to: '',
  },
  {
    // The reader is the only thing that feeds the tap, and it cannot be driven
    // from a test. Losing that one call is silent.
    name: 'XNAUT-201 the PTY reader stops teeing reads',
    check: 'cargo test --bin xnaut the_pty_reader_still_tees',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/pty.rs',
    from:
      '                    if let Some(state) = app.try_state::<AppState>() {\n' +
      '                        tauri::async_runtime::block_on(tee_output(\n' +
      '                            &state,\n' +
      '                            &session_id,\n' +
      '                            &buffer[..n],\n' +
      '                        ));\n' +
      '                    }\n',
    to: '',
  },
  {
    // The other half: a tap that exists but is never fed leaves the phone
    // holding an open socket that shows nothing.
    name: 'XNAUT-201 the PTY tee stops feeding the mobile tap',
    check: 'cargo test --bin xnaut pty::tests',
    cwd: 'src-tauri',
    slow: true,
    file: 'src-tauri/src/pty.rs',
    from: '        tap.push(chunk);',
    to: '        let _ = tap;',
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
  {
    // The shipped bug: OSC 7 puts the working directory in the same chunk as
    // the output, so a keyword trigger fired on the path, not on what ran.
    name: 'XNAUT-199 a trigger matches escape sequences instead of output',
    check: 'node scripts/triggers-smoke.cjs',
    file: 'src/js/app.js',
    from: "    .replace(/\\x1b\\][^\\x07\\x1b]*(?:\\x07|\\x1b\\\\)/g, '')",
    to: '',
  },
  {
    name: 'XNAUT-199 a trailing comma makes a keyword trigger fire on everything',
    check: 'node scripts/triggers-smoke.cjs',
    file: 'src/js/app.js',
    from: ".map(k => k.trim().toLowerCase()).filter(Boolean)",
    to: ".map(k => k.trim().toLowerCase())",
  },
  {
    name: 'XNAUT-199 a streaming agent notifies once per 16 ms flush',
    check: 'node scripts/triggers-smoke.cjs',
    file: 'src/js/app.js',
    from: '    if (last !== undefined && now - last < TRIGGER_COOLDOWN_MS) continue;',
    to: '',
  },
  {
    // The ticket itself: two implementations, one of them dead and emitting
    // three events nothing listened for.
    name: 'XNAUT-199 the second trigger path grows back in the backend',
    check: 'node scripts/triggers-smoke.cjs',
    file: 'src-tauri/src/main.rs',
    from: '            commands::terminal_output_snapshot,',
    to: '            commands::terminal_output_snapshot,\n            commands::create_trigger,',
  },
  {
    name: 'XNAUT-199 Agent Space output stops reaching the triggers',
    check: 'node scripts/triggers-smoke.cjs',
    file: 'src/js/agent-space.js',
    from: '          if (window.xnautCheckTriggers) window.xnautCheckTriggers(chunk);',
    to: '',
  },
];

const withRust = process.argv.includes('--all');
// --only <substring> narrows to one ticket's entries. The full run is long
// enough that gathering evidence for a single fix otherwise means re-proving
// everything else first.
const onlyFlag = process.argv.indexOf('--only');
const ONLY = onlyFlag === -1 ? null : process.argv[onlyFlag + 1];
const cases = MUTATIONS.filter(
  (m) => (withRust || !m.slow) && (!ONLY || m.name.includes(ONLY)),
);

// --json <path> writes the run as data, so a test report quotes the real red
// test instead of restating a verdict line.
const jsonFlag = process.argv.indexOf('--json');
const JSON_OUT = jsonFlag === -1 ? null : process.argv[jsonFlag + 1];
const record = [];

const root = mkdtempSync(join(tmpdir(), 'mutation-check-'));
execSync(
  `rsync -a --exclude .git --exclude target --exclude node_modules ${JSON.stringify(REPO)}/ ${JSON.stringify(root)}/`,
  { stdio: 'inherit' },
);
// Linked, not copied: Playwright's browsers make node_modules far too big to rsync.
execSync(`ln -s ${JSON.stringify(join(REPO, 'node_modules'))} ${JSON.stringify(join(root, 'node_modules'))}`);

// Its own target dir: sharing the worktree's starves the running `cargo tauri
// dev` watcher, and a fresh copy each run would rebuild Tauri from cold.
//
// Overridable because the default is shared across every worktree on the
// machine, and two agents running this at once then fight over one cargo lock
// and, worse, rebuild that target from different sources: a "mutated" run can
// end up executing a binary built from someone else's tree, which reads as
// SURVIVED. Set MUTATION_TARGET_DIR to get an isolated (cold) target.
const TARGET = process.env.MUTATION_TARGET_DIR || join(tmpdir(), 'mutation-check-target');

// Returns {ok, output}. The output of the MUTATED run is the evidence: it is
// the red test naming what broke. A verdict line alone asks to be trusted.
// A build that cannot build is not a red test, and calling it one sends the
// next person hunting a bug that is not there. macOS purges /var/folders and
// ~/Library/Caches under storage pressure, which on a 96%-full disk takes the
// shared cargo target and the Playwright browsers out from under a run. Both
// look exactly like "already fails unmutated" unless they are named.
const ENVIRONMENT_BREAKAGE = [
  [/couldn't read .*out\/bindgen\.rs|could not compile `libsqlite3-sys`/, 'the shared cargo target is half-deleted — rm -rf $TMPDIR/mutation-check-target'],
  [/Executable doesn't exist at .*ms-playwright/, 'the Playwright browser is gone — npx playwright install chromium'],
];

const environmentProblem = (output) => {
  for (const [pattern, advice] of ENVIRONMENT_BREAKAGE) {
    if (pattern.test(output)) return advice;
  }
  return null;
};

const run = (m) => {
  try {
    const out = execSync(m.check, {
      cwd: join(root, m.cwd || '.'),
      stdio: 'pipe',
      // Its own port: playwright's reuseExistingServer would otherwise attach to
      // a server another worktree already has on 4173 and run the check against
      // that worktree's frontend, so every mutation here would read as survived.
      env: {
        ...process.env,
        CARGO_TARGET_DIR: TARGET,
        XNAUT_TEST_PORT: process.env.XNAUT_TEST_PORT || '4174',
      },
    });
    return { ok: true, output: out.toString() };
  } catch (e) {
    return { ok: false, output: `${e.stdout || ''}${e.stderr || ''}` };
  }
};

// The line a reader needs out of a few hundred lines of test runner noise.
const failingLine = (output) =>
  output
    .split('\n')
    .find((l) => /^\w*Error: |✘|✕| panicked at |^test .*FAILED/.test(l))
    ?.trim()
    .slice(0, 160) || '(no assertion line found)';

let bad = 0;
for (const m of cases) {
  const path = join(root, m.file);
  const pristine = readFileSync(path, 'utf8');

  if (!pristine.includes(m.from)) {
    console.log(`NOT FOUND  ${m.name}\n           ${m.file} no longer contains the mutated text`);
    bad += 1;
    continue;
  }
  const before = run(m);
  if (!before.ok) {
    const broken = environmentProblem(before.output);
    console.log(broken
      ? `ENV BROKE   ${m.name}\n           not a failing test: ${broken}`
      : `BASELINE   ${m.name}\n           ${m.check} already fails unmutated`);
    bad += 1;
    continue;
  }

  writeFileSync(path, pristine.replace(m.from, m.to));
  const mutated = run(m);
  writeFileSync(path, pristine);

  console.log(`${mutated.ok ? 'SURVIVED  ' : 'caught    '} ${m.name}`);
  if (!mutated.ok) console.log(`           ${failingLine(mutated.output)}`);
  if (mutated.ok) bad += 1;

  record.push({
    name: m.name,
    check: m.check,
    file: m.file,
    from: m.from,
    to: m.to,
    caught: !mutated.ok,
    evidence: failingLine(mutated.output),
  });
}

if (JSON_OUT) writeFileSync(JSON_OUT, `${JSON.stringify(record, null, 2)}\n`);

console.log(
  `\n${cases.length - bad}/${cases.length} mutations caught${withRust ? '' : '  (cargo skipped, --all to include)'}`,
);
process.exit(bad ? 1 : 0);
