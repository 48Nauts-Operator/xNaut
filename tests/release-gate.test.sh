#!/usr/bin/env bash
# Exercise the actual release gate against isolated on-disk run/launch receipts.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
REPO="$REPO" python3 - <<'PY'
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

REPO = Path(os.environ['REPO'])
cutoff = (dt.datetime.now(dt.timezone.utc) - dt.timedelta(seconds=60)).isoformat()
SOURCE = subprocess.check_output(['git', 'rev-list', '-1', '--before='+cutoff, 'HEAD'], cwd=REPO, text=True).strip()
if not SOURCE:
    raise SystemExit('No historical commit available for the isolated receipt fixtures')
EPOCH = int(subprocess.check_output(['git', 'show', '-s', '--format=%ct', SOURCE], cwd=REPO, text=True))
# A real historical commit keeps synthetic receipt times post-commit and in
# the past even when these tests run immediately after a new source commit.
START = dt.datetime.fromtimestamp(EPOCH, dt.timezone.utc)
def stamp(seconds):
    return (START + dt.timedelta(seconds=seconds)).strftime('%Y-%m-%dT%H:%M:%SZ')
BINARY = 'a' * 64
NATIVE_IDS = ['identity', 'sidebar', 'right-pane', 'snippets', 'browser', 'markdown', 'diff',
              'workspace', 'worktrees', 'more', 'help', 'settings', 'roster', 'refresh-usage', 'restore']
NATIVE_IDS += ['settings-' + key for key in ['ai','voice','tasksmode','appearance','shortcuts','mobile',
                                           'nautify','guardrails','coreteam','issueintake','triggers']]
NATIVE_IDS += ['view-' + key for key in ['agent','buildrun','nautflowrun','nfvalidate','nfdesign']]

def summary():
    return {'id':'fixture', 'host':'fixture-host', 'suite':'gui-smoke', 'app_version':'1.30.0-dev',
            'source_commit':SOURCE, 'binary_sha256':BINARY, 'started':stamp(32), 'finished':stamp(34),
            'cases':[{'id':key, 'status':'passed'} for key in ['launch','surfaces']]}

class Gate(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.runs = Path(self.temp.name)
        self.root = self.runs / 'fixture-host' / 'fixture'
        self.root.mkdir(parents=True)
        self.record = summary()

    def call(self, expected=False):
        (self.root / 'run.json').write_text(json.dumps(self.record))
        result = subprocess.run(['bash', str(REPO/'scripts/release-gate.sh'), '1.30.0', SOURCE], cwd=REPO,
            env={**os.environ, 'RUNS':str(self.runs), 'BINARY_SHA256':BINARY}, text=True, capture_output=True)
        self.assertEqual(result.returncode == 0, expected, result.stdout + result.stderr)
        if not expected:
            self.assertIn('REFUSED:', result.stdout)
        return result

    def native(self):
        self.record.update(method='isolated-native-webview-dom', status='passed', pid=456,
                           launch_receipt='launch.json', cases=[])
        for key in NATIVE_IDS:
            self.record['cases'].append({'id':key, 'status':'passed', 'evidence_file':key+'.json'})
            (self.root/(key+'.json')).write_text(json.dumps({'case':key, 'source_commit':SOURCE,
                'pid':456, 'observed_at':stamp(33), 'result':{'ok':True}}))
        self.launch = {'source_commit':SOURCE, 'binary_sha256':BINARY, 'app_version':'1.30.0-dev',
            'pid':456, 'started':stamp(0), 'finished':stamp(31), 'status':'passed', 'owner_untouched':True,
            'cases':[{'id':'launch','status':'passed'}],
            'launch':{'pid':456, 'owned':True, 'executable':'/fixture/xNAUT.app/Contents/MacOS/xnaut',
                      'observed_running_seconds':31}}
        self.save_launch()

    def save_launch(self):
        (self.root/'launch.json').write_text(json.dumps(self.launch))

    def test_provenanced_legacy_summary_passes(self):
        self.call(True)

    def test_empty_missing_malformed_duplicate_or_incomplete_cases_refuse(self):
        for cases in [None, [], {}, 'passed', [None], [{}], [{'id':'launch','status':'passed'}],
                      [{'id':'launch','status':'passed'}, {'id':'launch','status':'passed'}]]:
            with self.subTest(cases=cases):
                self.record = summary(); self.record['cases'] = cases; self.call()
        self.record = summary(); del self.record['cases']; self.call()

    def test_failed_partial_unknown_case_and_run_status_refuse(self):
        for status in ['failed','partial','unknown',None]:
            self.record = summary(); self.record['cases'][1]['status'] = status; self.call()
        self.record = summary(); self.record['status'] = 'failed'; self.call()

    def test_wrong_suite_missing_or_wrong_build_provenance_refuse(self):
        for key, value in [('suite','unit-tests'), ('source_commit',None), ('source_commit','b'*40),
                           ('source_commit',SOURCE[:12]), ('binary_sha256',None),
                           ('binary_sha256','not-a-hash'), ('binary_sha256','b'*64)]:
            self.record = summary(); self.record[key] = value; self.call()

    def test_stale_future_or_malformed_times_refuse(self):
        for key, value in [('started',stamp(-1)), ('finished',stamp(31)), ('finished',None),
                           ('finished','2999-01-01T00:00:00Z'), ('finished',{})]:
            self.record = summary(); self.record[key] = value; self.call()

    def test_newer_failure_never_falls_back_to_older_green(self):
        older = self.runs/'fixture-host'/'older'; older.mkdir()
        green = summary(); green['finished'] = stamp(33)
        (older/'run.json').write_text(json.dumps(green))
        self.record['cases'] = []; self.call()
        self.record['finished'] = None; self.call()

    def test_native_walk_and_explicit_same_build_launch_pass(self):
        self.native(); self.call(True)

    def test_native_attachment_identity_does_not_count_as_launch(self):
        self.native(); del self.record['launch_receipt']; self.call()
        self.native(); (self.root/'launch.json').unlink(); self.call()
        self.native(); (self.root/'launch.json').write_text('{'); self.call()

    def test_missing_native_check_and_missing_or_failed_observation_refuse(self):
        self.native(); self.record['cases'] = [c for c in self.record['cases'] if c['id'] != 'help']; self.call()
        self.native(); (self.root/'browser.json').unlink(); self.call()
        self.native(); evidence = json.loads((self.root/'help.json').read_text()); evidence['result']['ok'] = False
        (self.root/'help.json').write_text(json.dumps(evidence)); self.call()
        self.native(); evidence['result']['ok'] = True; evidence['pid'] = 789
        (self.root/'help.json').write_text(json.dumps(evidence)); self.call()

    def test_launch_must_match_source_hash_version_pid_and_real_survival(self):
        for key, value in [('source_commit','b'*40), ('binary_sha256','b'*64), ('app_version','1.30.0'),
                           ('pid',789), ('owner_untouched',False), ('finished',stamp(35)), ('started',stamp(-1))]:
            self.native(); self.launch[key] = value; self.save_launch(); self.call()
        for key, value in [('owned',False), ('pid',789), ('executable','relative/path'),
                           ('observed_running_seconds',1), ('observed_running_seconds',True)]:
            self.native(); self.launch['launch'][key] = value; self.save_launch(); self.call()
        self.native(); self.launch['finished'] = stamp(1); self.save_launch(); self.call()
        self.native(); self.launch['cases'] = []; self.save_launch(); self.call()

    def test_launch_or_case_path_cannot_escape_even_by_symlink(self):
        self.native(); (self.runs/'foreign.json').write_text(json.dumps(self.launch))
        self.record['launch_receipt'] = '../../foreign.json'; self.call()
        self.record['launch_receipt'] = str(self.runs/'foreign.json'); self.call()
        (self.root/'escaped.json').symlink_to(self.runs/'foreign.json')
        self.record['launch_receipt'] = 'escaped.json'; self.call()
        self.native(); self.record['cases'][0]['evidence_file'] = '../../foreign.json'; self.call()

unittest.main()
PY
