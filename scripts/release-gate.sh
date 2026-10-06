#!/usr/bin/env bash
# Refuse to release a version nothing has clicked.
#
#   scripts/release-gate.sh 1.13.11 [commit-ish]
#
# Exit 0 only when a GUI smoke run exists for that exact version, every case in
# it passed, and its exact source/build provenance matches the release. Native
# attachment walks require a linked real launch receipt; identity is not launch.
# Optionally set BINARY_SHA256 to pin the specific candidate executable hash.
#
# WHY: v1.13.7, .8, .9 and .10 all shipped, and not one of them has a run record
# where every case passed. The order was build -> tag -> release -> test, so the
# test could only ever confirm a regression that users already had. This turns it
# around: the evidence has to exist before the tag can move.
#
# "partial" is a refusal, not a pass. A partial run is one where controls were
# pressed and nothing verified they rendered, which is exactly the state that let
# a broken Settings walk read as green for two releases.
#
# The freshness check is the other half. Without it the first green run becomes a
# permanent licence and every later release rides on evidence for code it does
# not contain.
#
# ponytail: reads the run records on THIS machine. A run that happened on tron
# has to be rsynced home first (that is already the contract -- the dashboard
# reads the same directory). No server, no database.
set -uo pipefail

VERSION="${1:-}"
COMMIT="${2:-HEAD}"
RUNS="${RUNS:-$HOME/xnaut-testing/runs}"

if [ -z "$VERSION" ]; then
  echo "usage: release-gate.sh <version> [commit-ish]" >&2
  exit 2
fi
VERSION="${VERSION#v}"

COMMIT_SHA="$(git rev-parse --verify "${COMMIT}^{commit}" 2>/dev/null)" || COMMIT_SHA=""
COMMIT_EPOCH="$(git log -1 --format=%ct "$COMMIT" 2>/dev/null)" || COMMIT_EPOCH=""
if [ -z "$COMMIT_SHA" ] || [ -z "$COMMIT_EPOCH" ]; then
  echo "release-gate: cannot resolve '$COMMIT'" >&2
  exit 2
fi

VERSION="$VERSION" COMMIT_SHA="$COMMIT_SHA" COMMIT_EPOCH="$COMMIT_EPOCH" RUNS="$RUNS" python3 - <<'PYGATE'
import datetime as dt
import glob
import json
import math
import os
from pathlib import Path
import re
import sys

version = os.environ["VERSION"]
commit = os.environ["COMMIT_SHA"]
commit_time = dt.datetime.fromtimestamp(int(os.environ["COMMIT_EPOCH"]), dt.timezone.utc)
runs = os.environ["RUNS"]
wanted = {version, f"{version}-dev"}
expected_binary = os.environ.get("BINARY_SHA256", "")
if expected_binary and not re.fullmatch(r"[0-9a-f]{64}", expected_binary):
    print("REFUSED: BINARY_SHA256 must be a full lowercase SHA-256.")
    sys.exit(2)

# Existing native_surface_smoke.py schema: attachment validates these surfaces,
# but deliberately makes no claim that it launched the application.
NATIVE_CASES = {
    "identity", "sidebar", "right-pane", "snippets", "browser", "markdown",
    "diff", "workspace", "worktrees", "more", "help", "settings", "roster",
    "refresh-usage", "restore",
    *["settings-" + key for key in ("ai", "voice", "tasksmode", "appearance",
      "shortcuts", "mobile", "nautify", "guardrails", "coreteam", "issueintake", "triggers")],
    *["view-" + key for key in ("agent", "buildrun", "nautflowrun", "nfvalidate", "nfdesign")],
}

class Refusal(ValueError):
    pass

def require(condition, why):
    if not condition:
        raise Refusal(why)

def timestamp(value):
    require(isinstance(value, str), "missing or malformed timestamp")
    try:
        return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)
    except ValueError as error:
        raise Refusal("missing or malformed UTC timestamp") from error

def interval(record):
    start, finish = timestamp(record.get("started")), timestamp(record.get("finished"))
    require(commit_time <= start <= finish <= dt.datetime.now(dt.timezone.utc),
            "evidence must start after the release commit and finish in the past")
    return start, finish

def cases(record):
    rows = record.get("cases")
    require(isinstance(rows, list) and bool(rows), "cases must be a nonempty array")
    result = {}
    for row in rows:
        require(isinstance(row, dict), "each case must be an object")
        key = row.get("id")
        require(isinstance(key, str) and bool(key.strip()) and key not in result,
                "case IDs must be nonempty and unique")
        require(row.get("status") == "passed", f"case {key} is not passed")
        result[key] = row
    require(record.get("status", "passed") == "passed", "run status is not passed")
    return result

def provenance(record):
    require(record.get("source_commit") == commit, "source_commit does not match the full release SHA")
    binary = record.get("binary_sha256")
    require(isinstance(binary, str) and re.fullmatch(r"[0-9a-f]{64}", binary),
            "missing or malformed executable binary_sha256")
    require(not expected_binary or binary == expected_binary, "binary_sha256 differs from the candidate")
    require(record.get("app_version") in wanted, "app_version differs from the candidate")

def positive_pid(value):
    return type(value) is int and value > 0

def linked_json(root, relative):
    require(isinstance(relative, str) and bool(relative.strip()), "missing evidence path")
    path = Path(relative)
    require(not path.is_absolute(), "evidence path must be relative to its run directory")
    path = (root / path).resolve()
    require(path.is_relative_to(root.resolve()) and path.is_file(), "evidence path escapes its run directory or is unavailable")
    try:
        value = json.loads(path.read_text())
    except (OSError, ValueError) as error:
        raise Refusal("linked evidence is unreadable or malformed") from error
    require(isinstance(value, dict), "linked evidence must be an object")
    return value

def validate(record, path):
    require(record.get("suite") == "gui-smoke", "record is not a GUI smoke suite")
    require(all(isinstance(record.get(key), str) and record[key].strip() for key in ("id", "host")), "missing run ID or host")
    provenance(record)
    start, finish = interval(record)
    checked = cases(record)
    if record.get("method") == "isolated-native-webview-dom":
        require(record.get("status") == "passed", "native surface run is not passed")
        require(NATIVE_CASES <= checked.keys(), "native surface run is missing mandatory checks: " + ", ".join(sorted(NATIVE_CASES - checked.keys())))
        require(positive_pid(record.get("pid")), "native surface PID is missing")
        root = path.parent
        for key, case in checked.items():
            evidence = linked_json(root, case.get("evidence_file"))
            require(evidence.get("case") == key and evidence.get("source_commit") == commit
                    and positive_pid(evidence.get("pid")) and evidence["pid"] == record["pid"], f"case {key} evidence identity differs")
            require(isinstance(evidence.get("result"), dict) and evidence["result"].get("ok") is True,
                    f"case {key} has no successful observation")
            require(start <= timestamp(evidence.get("observed_at")) <= finish,
                    f"case {key} observation is outside its run")
        launch = linked_json(root, record.get("launch_receipt"))
        provenance(launch)
        require(launch.get("status") == "passed" and positive_pid(launch.get("pid")), "linked launch status or PID is invalid")
        require(all(launch.get(key) == record.get(key) for key in ("source_commit", "binary_sha256", "app_version", "pid")),
                "launch receipt and native surface identity differ")
        launched_at, observed_at = interval(launch)
        require(observed_at <= start, "linked launch was observed after the surface walk began")
        require("launch" in cases(launch), "linked receipt has no passing launch check")
        proof = launch.get("launch")
        require(isinstance(proof, dict), "linked receipt has no owned launch proof")
        seconds = proof.get("observed_running_seconds")
        require(proof.get("owned") is True and positive_pid(proof.get("pid"))
                and proof["pid"] == record["pid"] and launch.get("owner_untouched") is True,
                "launch ownership/PID/owner preservation is unproved")
        require(isinstance(proof.get("executable"), str) and Path(proof["executable"]).is_absolute(),
                "launch executable is not identified")
        require(type(seconds) in (int, float) and math.isfinite(seconds) and seconds >= 30
                and (observed_at - launched_at).total_seconds() >= 30,
                "launch must prove at least 30 seconds of actual process survival")
    else:
        # Existing gui-smoke.sh summary schema remains recognized; old records
        # without exact build provenance cannot authorize a new release.
        require(record.get("method") in (None, ""), "unknown GUI smoke method")
        require({"launch", "surfaces"} <= checked.keys(), "GUI smoke requires both launch and surfaces checks")
    return checked

candidates = []
for name in glob.glob(os.path.join(runs, "*", "*", "run.json")):
    try:
        record = json.loads(Path(name).read_text())
    except (OSError, ValueError):
        continue
    if not isinstance(record, dict) or not isinstance(record.get("app_version"), str) or record["app_version"] not in wanted:
        continue
    try:
        finished = timestamp(record.get("finished"))
    except Refusal:
        # A version-matching incomplete record must not let an older green run
        # silently authorize the candidate.
        finished = dt.datetime.max.replace(tzinfo=dt.timezone.utc)
    candidates.append((finished, name, record))

if not candidates:
    print(f"REFUSED: no GUI smoke run for {version} in {runs}/<host>/<id>/run.json.")
    sys.exit(1)
_, name, record = max(candidates, key=lambda item: (item[0], item[1]))
try:
    checked = validate(record, Path(name))
except (Refusal, OSError) as error:
    print(f"REFUSED: {error}.")
    print(f"  {name}")
    sys.exit(1)
print(f"OK: {version} source {commit} binary {record['binary_sha256']} verified by {record['id']} on {record['host']} ({record['finished']}).")
for key, case in checked.items():
    print(f"  passed    {key}: {case.get('title', '')}")
PYGATE
