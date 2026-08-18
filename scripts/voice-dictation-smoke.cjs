// The dictate button must actually reach the backend.
//
// Its predecessor called window.SpeechRecognition, which does not exist in
// WKWebView: no error, no crash, a button that did nothing for months. This
// checks the class of failure directly — every command the button invokes has
// to exist in main.rs AND in permissions/default.toml, or it fails at runtime
// with "not allowed by ACL".

const { readFileSync } = require('node:fs');
const { join } = require('node:path');

const root = join(__dirname, '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const fail = (m) => { console.error(`FAIL ${m}`); process.exit(1); };

const js = read('src/js/chat-panel.js');
// Comments stripped first: the file documents the old bug on purpose, and a
// check that cannot tell a warning from the bug it warns about is noise.
const code = js.replace(/^\s*\/\/.*$/gm, '');

if (/SpeechRecognition/.test(code)) fail('chat-panel still calls window.SpeechRecognition, which WKWebView does not have');

const invoked = [...code.matchAll(/invoke\('(voice_[a-z_]+)'/g)].map((m) => m[1]);
if (!invoked.length) fail('the dictate button invokes no voice command');
for (const want of ['voice_start', 'voice_stop']) {
  if (!invoked.includes(want)) fail(`dictation never calls ${want}`);
}

const main = read('src-tauri/src/main.rs');
const acl = read('src-tauri/permissions/default.toml');
for (const cmd of new Set(invoked)) {
  if (!main.includes(`voice::${cmd}`)) fail(`${cmd} is not registered in main.rs invoke_handler`);
  if (!acl.includes(`"${cmd}"`)) fail(`${cmd} is missing from permissions/default.toml — it will fail at runtime with "not allowed by ACL"`);
}

// macOS denies the microphone with no prompt and no error when the usage
// string is absent, which looks exactly like a broken feature.
if (!read('src-tauri/Info.plist').includes('NSMicrophoneUsageDescription')) {
  fail('NSMicrophoneUsageDescription missing — macOS will deny the mic silently');
}

console.log(`PASS voice-dictation-smoke (${[...new Set(invoked)].join(', ')})`);
