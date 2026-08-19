// The interactive half of SSH: a channel, an output event, and a write that
// reaches it.
//
// The bug this exists to catch (XNAUT-200) was silent by construction. The
// authenticated session was bound to `_ssh_handle` and dropped, `write_to_ssh`
// was a TODO returning Ok, and nothing emitted ssh-output-<id>. Connecting
// "worked", a terminal opened, and every keystroke went nowhere with no error
// anywhere. Only the two ends together prove anything, so this checks both.

const { readFileSync } = require('node:fs');
const { join } = require('node:path');

const root = join(__dirname, '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const fail = (m) => { console.error(`FAIL ${m}`); process.exit(1); };

const ssh = read('src-tauri/src/ssh.rs');
const commands = read('src-tauri/src/commands.rs');
const app = read('src/js/app.js');

// 1. The connection has to be kept. A handle that is dropped at the end of the
// statement leaves a session nobody can write to.
if (/let\s+_ssh_handle\s*=/.test(ssh)) fail('the authenticated session is bound to _ssh_handle and dropped');
if (!/channel:\s*Arc::clone\(&channel\)/.test(ssh)) fail('the shell channel is not stored on the session');
if (!/\.shell\(\)/.test(ssh)) fail('no interactive shell is ever started on the channel');

// 2. Output has to be emitted under the name the frontend listens for.
if (!ssh.includes('ssh-output-{session_id}')) fail('nothing emits ssh-output-<id>, so the terminal stays blank');
if (!/ssh-output-\$\{sshSessionId\}/.test(app)) fail('the terminal does not listen for ssh-output');
// Base64 on the wire, same contract as terminal-output: raw JSON strings lose
// escape bytes and mangle multi-byte UTF-8.
if (!ssh.includes('STANDARD.encode(&pending)')) fail('output is not base64 encoded');
if (!/atob\(event\.payload\.data\)/.test(app)) fail('the terminal never decodes the base64 output');

// 3. Exactly one listener per session. Two of them wrote every chunk twice.
const listeners = (app.match(/listen\(`ssh-output-/g) || []).length;
if (listeners !== 1) fail(`ssh-output has ${listeners} listeners, expected exactly 1`);

// 4. Keystrokes have to reach the channel, not a println.
if (/TODO: Implement actual SSH write/.test(commands)) fail('write_to_ssh is still a TODO that swallows input');
if (!commands.includes('crate::ssh::write_to_ssh(')) fail('the write command never calls into the ssh module');

// 5. The key field. The editor sent privateKey, the backend read key_path, and
// serde dropped it, so every key profile failed as "no authentication method".
if (!/#\[serde\(rename_all = "camelCase"\)\]\s*\npub struct SshConfig/.test(ssh)) fail('SshConfig is not camelCase, so keyPath from the editor is dropped');
if (app.includes('ssh-private-key')) fail('the editor still reads the hidden private-key textarea');
if (!/keyPath: authMethod === 'privateKey'/.test(app)) fail('the saved profile does not carry keyPath');
if (!/keyPath: profile\.authMethod === 'privateKey' \? profile\.keyPath/.test(app)) fail('connect does not send keyPath');

// 6. Both new commands must exist and be allowed, or they fail at runtime with
// "Command not found" into the nearest catch.
const main = read('src-tauri/src/main.rs');
const acl = read('src-tauri/permissions/default.toml');
for (const cmd of ['write_to_ssh', 'resize_ssh']) {
  if (!main.includes(`commands::${cmd}`)) fail(`${cmd} is not registered in main.rs`);
  if (!acl.includes(`"${cmd}"`)) fail(`${cmd} is missing from permissions/default.toml`);
}

console.log('PASS ssh-interactive-smoke (channel held, output emitted and decoded, writes wired, keyPath on both sides)');
