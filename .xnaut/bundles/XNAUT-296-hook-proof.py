"""Exercise the production hook against an isolated local HTTP listener."""
import http.server
import json
import os
from pathlib import Path
import subprocess
import threading

root = Path(__file__).resolve().parents[2]
seen = []


class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        seen.append((self.path, json.loads(self.rfile.read(int(self.headers['Content-Length'])))))
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'{}')

    def log_message(self, *_args):
        pass


server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    env = dict(os.environ, XNAUT_HOOK_URL=f'http://127.0.0.1:{server.server_port}',
               XNAUT_HOOK_TOKEN='isolated-fixture', XNAUT_RUN_ID='fixture-run',
               XNAUT_MODEL='requested-model')
    for data in ['{"model":"actual-model"}', '{}']:
        subprocess.run(['sh', str(root / 'src-tauri/scripts/hooks/xnaut-hook.sh'), 'working'],
                       input=data, text=True, env=env, check=True, timeout=5)
finally:
    server.shutdown()
    server.server_close()
    thread.join()

assert len(seen) == 2
assert all(path == '/v1/hook' and body['run_id'] == 'fixture-run' for path, body in seen)
assert seen[0][1]['model'] == 'actual-model'
assert seen[1][1]['model'] == 'requested-model'
print('2 requests passed: reported model overrides launch model; fallback and run id retained.')
