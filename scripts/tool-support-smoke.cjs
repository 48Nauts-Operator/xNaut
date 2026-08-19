// The tool-support probe has to reach a provider and report what it said.
//
// The bug it exists to catch (XNAUT-195) was invisible for four days because
// nothing asked the question. A check that cannot tell "this route refuses tool
// calls" from "this route is fine" would put the same silence behind a button.

const { readFileSync } = require('node:fs');
const { join } = require('node:path');

const root = join(__dirname, '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const fail = (m) => { console.error(`FAIL ${m}`); process.exit(1); };

const rust = read('src-tauri/src/tool_support.rs');
// The probe is worthless without tools in the payload: every route answers 200
// to a plain completion, including the ones this exists to catch.
if (!/"tools"\s*:\s*\[/.test(rust)) fail('the probe payload carries no tools, so it proves nothing');

// Both commands must exist in main.rs AND the ACL, or the button 
// fails at runtime with "not allowed by ACL".
const main = read('src-tauri/src/main.rs');
const acl = read('src-tauri/permissions/default.toml');
for (const cmd of ['model_tool_support', 'model_tool_support_reset']) {
  if (!main.includes(`tool_support::${cmd}`)) fail(`${cmd} is not registered in main.rs`);
  if (!acl.includes(`"${cmd}"`)) fail(`${cmd} is missing from permissions/default.toml`);
}

// The UI has to render the button, find it, and call the command. A button
// wired to nothing is the failure mode this whole ticket is about.
const ui = read('src/js/agent-space.js');
if (!ui.includes('data-toolcheck')) fail('agent-space renders no tool-call check');
if (!ui.includes("querySelector('[data-toolcheck]')")) fail('the check button is never looked up');
if (!ui.includes("invoke('model_tool_support'")) fail('the check button never calls the probe');
// And it has to show the upstream's words, not just a tick or a cross.
if (!ui.includes('support.reason')) fail('the failure reason is discarded, leaving a red cross with no cause');

// chat_model must reach the backend, or picking it changes nothing.
if (!ui.includes('chat_model:')) fail('agent-space never saves chat_model');
const profiles = read('src-tauri/src/agent_profiles.rs');
if (!profiles.includes('pub chat_model: String')) fail('AgentProfile has no chat_model');
if (!profiles.includes('chat_model_or_model')) fail('nothing resolves chat_model back to model');

console.log('PASS tool-support-smoke (payload carries tools, commands in ACL, button wired, chat_model persisted)');
