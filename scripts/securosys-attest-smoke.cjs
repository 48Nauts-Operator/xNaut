// Smoke for mcp/securosys-attest.py: the MCP handshake works, and a signature
// only becomes a stored receipt when the TSB actually returned one.
//
// The TSB here is a local stub — the real demo TSB is borrowed access and no
// test suite may call it (that rule lives in NautGate's sb-attest too). The
// stub signs request 1 and refuses request 2, which is exactly the pair of
// behaviours the server must not confuse.

const http = require('node:http');
const { spawn } = require('node:child_process');
const { mkdtempSync } = require('node:fs');
const { readFileSync, existsSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join } = require('node:path');

const assert = (ok, what) => { if (!ok) { console.error(`FAIL ${what}`); process.exit(1); } };

let calls = 0;
const stub = http.createServer((req, res) => {
  let body = '';
  req.on('data', (c) => { body += c; });
  req.on('end', () => {
    calls += 1;
    const sent = JSON.parse(body);
    assert(req.url === '/v1/synchronousSign', `stub got ${req.url}`);
    assert(sent.signRequest && sent.signRequest.signKeyName === 'SMOKE_KEY', 'signRequest shape');
    if (calls === 1) {
      res.end(JSON.stringify({ signature: 'c21va2Utc2lnbmF0dXJl' }));
    } else {
      res.statusCode = 500;
      res.end(JSON.stringify({ errorCode: 701, reason: 'res.error.in.hsm', message: 'KEY_FUNCTION_NOT_PERMITTED' }));
    }
  });
});

stub.listen(0, '127.0.0.1', () => {
  const port = stub.address().port;
  const home = mkdtempSync(join(tmpdir(), 'sec-attest-smoke-'));
  // publish target: a real (throwaway) git repo, no remotes — push is skipped,
  // the commit is the assertion
  const pub = mkdtempSync(join(tmpdir(), 'sec-attest-pub-'));
  const { execSync } = require('node:child_process');
  execSync(`git init -q ${pub} && mkdir -p ${pub}/attest && echo '{"receipts": []}' > ${pub}/attest/receipts.json && git -C ${pub} add -A && git -C ${pub} -c user.email=s@s -c user.name=smoke commit -qm init`);
  const child = spawn('python3', ['mcp/securosys-attest.py'], {
    env: {
      ...process.env,
      HOME: home, // receipts land under the temp HOME, wiped with it
      GIT_AUTHOR_EMAIL: 's@s', GIT_AUTHOR_NAME: 'smoke',
      GIT_COMMITTER_EMAIL: 's@s', GIT_COMMITTER_NAME: 'smoke',
      SECUROSYS_TSB_URL: `http://127.0.0.1:${port}`,
      SECUROSYS_KEY_NAME: 'SMOKE_KEY',
      SECUROSYS_PUBLISH_DIR: pub,
    },
  });
  const replies = [];
  let buf = '';
  child.stdout.on('data', (chunk) => {
    buf += chunk;
    let i;
    while ((i = buf.indexOf('\n')) !== -1) {
      replies.push(JSON.parse(buf.slice(0, i)));
      buf = buf.slice(i + 1);
    }
  });

  const send = (msg) => child.stdin.write(JSON.stringify(msg) + '\n');
  send({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05' } });
  send({ jsonrpc: '2.0', method: 'notifications/initialized' });
  send({ jsonrpc: '2.0', id: 2, method: 'tools/list' });
  send({ jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'attest', arguments: { subject: 'smoke.test', text: 'hello proof' } } });
  send({ jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'attest', arguments: { subject: 'smoke.test', text: 'second' } } });
  send({ jsonrpc: '2.0', id: 5, method: 'tools/call', params: { name: 'receipts', arguments: {} } });
  child.stdin.end();

  child.on('close', () => {
    stub.close();
    assert(replies.length === 5, `expected 5 replies, got ${replies.length}`);
    assert(replies[0].result.serverInfo.name === 'securosys-attest', 'initialize');
    assert(replies[1].result.tools.length === 2, 'tools/list');
    const ok = JSON.parse(replies[2].result.content[0].text);
    assert(ok.signature === 'c21va2Utc2lnbmF0dXJl', 'signature stored from TSB response');
    assert(ok.digest.length === 64, 'text was sha256-hashed');
    assert(replies[3].result.isError === true, 'TSB failure is an error, not a receipt');
    assert(/KEY_FUNCTION_NOT_PERMITTED/.test(replies[3].result.content[0].text), 'TSB reason survives');
    const list = JSON.parse(replies[4].result.content[0].text);
    assert(list.receipts.length === 1, `exactly the signed one is stored, got ${list.receipts.length}`);
    const file = join(home, 'Library', 'Application Support', 'xnaut', 'attestations.jsonl');
    const onLinux = !existsSync(file);
    const path = onLinux ? join(home, '.local', 'share', 'xnaut', 'attestations.jsonl') : file;
    assert(readFileSync(path, 'utf8').trim().split('\n').length === 1, 'receipt file has one line');
    assert(ok.published === true, 'attest reports the receipt as published');
    const pubJson = JSON.parse(readFileSync(join(pub, 'attest', 'receipts.json'), 'utf8'));
    assert(pubJson.receipts.length === 1, 'publish dir mirrors exactly the signed receipt');
    const commits = execSync(`git -C ${pub} log --oneline`).toString().trim().split('\n');
    assert(commits.length === 2 && /publish 1 receipt/.test(commits[0]), 'publish commit landed');
    console.log('PASS securosys-attest-smoke');
  });
});
