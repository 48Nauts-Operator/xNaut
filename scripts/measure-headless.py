#!/usr/bin/env python3
"""XNAUT-107: bounded, opt-in real CLI proof. Writes only inside the worktree.

Run: python3 scripts/measure-headless.py
At most three model calls, each one turn, $0.50 budget, 60-second timeout.
Only timing/status fields are saved; prompts, credentials and CLI logs are not.
"""
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / '.xnaut/measurements/XNAUT-107'
OUT.mkdir(parents=True, exist_ok=True)
marker = OUT / 'hook-fired'
source = (ROOT / 'src/js/project-management-panel.js').read_text()
shell_settings = re.search(r'const HEADLESS_CLAUDE_SETTINGS = `([^`]+)`;', source)[1]
env = {k: v for k, v in os.environ.items() if not k.startswith('XNAUT_')}
# Resolve the exact production shell expression, with no managed session context.
profile = json.loads(subprocess.check_output(
    ['bash', '-c', 'set -- ' + shell_settings + '; printf "%s" "$2"'], env=env, text=True))
assert profile == {'disableAllHooks': True}, profile
hook = {'SessionStart': [{'matcher': 'startup', 'hooks': [{
    'type': 'command', 'command': 'sleep 2; printf fired > ' + shlex.quote(str(marker)),
}]}]}
results = []
for name, settings, isolated in [
    ('controlled_baseline', {'hooks': hook, 'disableAllHooks': False}, True),
    ('controlled_minimal', {'hooks': hook, **profile}, True),
    ('inherited_minimal', profile, False),
]:
    marker.unlink(missing_ok=True)
    args = ['claude', '-p', 'Reply with only OK. Do not use tools.', '--output-format', 'json',
            '--tools', '', '--strict-mcp-config', '--mcp-config', '{"mcpServers":{}}',
            '--no-session-persistence', '--max-turns', '1', '--max-budget-usd', '0.50',
            '--settings', json.dumps(settings)]
    if isolated:
        args += ['--setting-sources', '']
    start = time.monotonic()
    proc = subprocess.Popen(args, cwd=OUT, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        stdout, _ = proc.communicate(timeout=60)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.communicate()
        raise SystemExit(f'{name}: timed out; stopped without retry')
    row = {'name': name, 'wall_ms': round((time.monotonic() - start) * 1000),
           'exit_code': proc.returncode, 'hook_fired': marker.exists()}
    try:
        data = json.loads(stdout)
    except ValueError:
        raise SystemExit(f'{name}: no JSON result; output withheld, stopped without retry')
    for key in ['duration_ms', 'duration_api_ms', 'is_error', 'subtype', 'num_turns', 'total_cost_usd']:
        row[key] = data.get(key)
    if row['duration_ms'] is not None and row['duration_api_ms']:
        row['reported_overhead_ms'] = row['duration_ms'] - row['duration_api_ms']
        row['reported_ratio'] = round(row['duration_ms'] / row['duration_api_ms'], 3)
    results.append(row)
    (OUT / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(row), flush=True)
    if proc.returncode or data.get('is_error') or data.get('subtype') != 'success':
        raise SystemExit('Unsuccessful model run; stopped without retry')
    if name.startswith('controlled') and marker.exists() != (name == 'controlled_baseline'):
        raise SystemExit('Hook marker contradicts expected profile behavior')
print('Hook marker proof passed. Compare wall timings; API noise can obscure the two-second delay.')
