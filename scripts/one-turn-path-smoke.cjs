// Both chat surfaces have to reach the same tools (XNAUT-194).
//
// The bug: the pane called chat_send, which posts messages and nothing else,
// while Agent Space went through run_turn and got the whole tool list. The same
// agent could create a ticket in one pane and not the other, and reported that
// as "the tool isn't available" — which reads as a feature nobody built.

const { readFileSync } = require('node:fs');
const { join } = require('node:path');

const root = join(__dirname, '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const fail = (m) => { console.error(`FAIL ${m}`); process.exit(1); };

const pane = read('src/js/chat-panel.js');
const stripped = pane.replace(/^\s*\/\/.*$/gm, '');

if (!stripped.includes("'chat_send_tools'")) {
  fail('the chat pane does not use the tool-capable turn, so its agent can do less than the same agent in Agent Space');
}

const chat = read('src-tauri/src/chat.rs');
if (!chat.includes('agent_tools::run_turn')) fail('chat_send_tools never runs the tool loop');
// The fallback must exist AND announce itself: a silent one cost four days.
if (!chat.includes('tool_failure_notice')) fail('a tool-less fallback that says nothing is the XNAUT-195 bug again');

// Registered and allowed, or it fails at runtime with "not allowed by ACL".
const main = read('src-tauri/src/main.rs');
const acl = read('src-tauri/permissions/default.toml');
if (!main.includes('chat::chat_send_tools')) fail('chat_send_tools is not registered in main.rs');
if (!acl.includes('"chat_send_tools"')) fail('chat_send_tools is missing from permissions/default.toml');

// Both surfaces must end up at the same tool list.
const tools = read('src-tauri/src/agent_tools.rs');
for (const name of ['create_agent', 'vault_write', 'list_agents']) {
  if (!tools.includes(`"name": "${name}"`)) fail(`${name} is not in tool_specs(); the surfaces agree but on nothing`);
}

console.log('PASS one-turn-path-smoke (both surfaces run the tool loop, the fallback announces itself)');
