// The veto shim is the thing that actually decides, and it is shell, so no
// Rust test covers it (XNAUT-189).
//
// It runs against a real HTTP server here rather than a stubbed curl, because
// every past failure in this family was in the seam: a reply shape the `case`
// did not match, a URL built by appending to a route. The rule under test is
// blunt — exit 2 is the ONLY outcome that stops an agent, and every other path,
// including the owner never answering, must exit 0.

const { spawn } = require('node:child_process');
const http = require('node:http');
const { join } = require('node:path');

const SHIM = join(__dirname, '..', 'src-tauri', 'scripts', 'hooks', 'xnaut-veto.sh');
const failures = [];

function check(name, actual, expected) {
  if (actual === expected) return;
  failures.push(`${name}: expected exit ${expected}, got ${actual}`);
}

// One server answers both routes: /v1/veto and /v1/inbox/wait/:id.
function serve(handler) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      let body = '';
      req.on('data', (chunk) => { body += chunk; });
      req.on('end', () => handler(req, res, body));
    });
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

// Async on purpose: the server under test lives in THIS process, and a
// synchronous spawn blocks the event loop, so curl talks to a server that
// cannot answer and every case looks like a fail-open. Cost an entire debug
// cycle; the shim was never wrong.
function runShim(port, extraEnv = {}) {
  const base = `http://127.0.0.1:${port}`;
  return new Promise((resolve) => {
    const child = spawn('sh', [SHIM], {
      env: {
        ...process.env,
        XNAUT_VETO_URL: `${base}/v1/veto`,
        XNAUT_HOOK_URL: base,
        XNAUT_HOOK_TOKEN: 'test-token',
        ...extraEnv,
      },
    });
    let stderr = '';
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.stdin.end(JSON.stringify({ tool_name: 'Bash', tool_input: { command: 'git push origin main' } }));
    child.on('close', (status) => resolve({ status, stderr }));
  });
}

(async () => {
  // 1. allow
  let server = await serve((_req, res, _body) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ decision: 'allow' }));
  });
  check('allow', (await runShim(server.address().port)).status, 0);
  server.close();

  // 2. deny, and the reason has to reach the model on stderr
  server = await serve((_req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ decision: 'deny', reason: 'Never from an agent.' }));
  });
  let result = await runShim(server.address().port);
  check('deny', result.status, 2);
  if (!String(result.stderr).includes('Never from an agent.')) {
    failures.push('deny: the reason never reached stderr, so the model is refused with no way to adapt');
  }
  server.close();

  // 3. ask -> the owner denies. This is the whole middle tier.
  let waited = null;
  server = await serve((req, res, _body) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    if (req.url.startsWith('/v1/veto')) {
      res.end(JSON.stringify({ decision: 'ask', reason: 'Pushing to main. Fine by you?', id: 'in-42' }));
      return;
    }
    waited = req.url;
    res.end(JSON.stringify({ id: 'in-42', status: 'denied' }));
  });
  result = await runShim(server.address().port);
  check('ask -> denied', result.status, 2);
  if (!String(result.stderr).includes('Fine by you?')) {
    failures.push('ask -> denied: the reason never reached stderr');
  }
  if (!waited || !waited.startsWith('/v1/inbox/wait/in-42')) {
    failures.push(`ask: waited on the wrong URL: ${waited}. The hook URL is a base, not a route (XNAUT-183).`);
  }
  server.close();

  // 4. ask -> the owner approves
  server = await serve((req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(req.url.startsWith('/v1/veto')
      ? JSON.stringify({ decision: 'ask', reason: 'ok?', id: 'in-43' })
      : JSON.stringify({ id: 'in-43', status: 'approved' }));
  });
  check('ask -> approved', (await runShim(server.address().port)).status, 0);
  server.close();

  // 5. ask, and the wait never resolves. The owner being away from the desk
  //    must not wedge the agent: an unanswered question is an allow.
  server = await serve((req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(req.url.startsWith('/v1/veto')
      ? JSON.stringify({ decision: 'ask', reason: 'ok?', id: 'in-44' })
      : JSON.stringify({ id: 'in-44', status: 'open' }));
  });
  check('ask -> still open', (await runShim(server.address().port)).status, 0);
  server.close();

  // 6. ask with no id to wait on: nothing to answer, so allow.
  server = await serve((_req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ decision: 'ask', reason: 'ok?' }));
  });
  check('ask with no id', (await runShim(server.address().port)).status, 0);
  server.close();

  // 7. the app is not there at all
  server = await serve((_req, res) => { res.destroy(); });
  const port = server.address().port;
  server.close();
  check('app unreachable', (await runShim(port)).status, 0);

  // 8. no policy wired at all
  check('no veto url', (await runShim(1, { XNAUT_VETO_URL: '' })).status, 0);

  if (failures.length) {
    for (const line of failures) console.error(`FAIL ${line}`);
    process.exit(1);
  }
  console.log('PASS veto-ask-smoke (allow, deny, ask->denied, ask->approved, ask->open, no id, unreachable, unwired)');
})();
