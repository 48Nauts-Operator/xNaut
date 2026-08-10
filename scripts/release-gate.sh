#!/usr/bin/env bash
# Refuse to release a version nothing has clicked.
#
#   scripts/release-gate.sh 1.13.11 [commit-ish]
#
# Exit 0 only when a GUI smoke run exists for that exact version, every case in
# it passed, and it finished AFTER the commit being released. Anything else is a
# refusal with the reason named.
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

COMMIT_EPOCH="$(git log -1 --format=%ct "$COMMIT" 2>/dev/null)" || COMMIT_EPOCH=""
if [ -z "$COMMIT_EPOCH" ]; then
  echo "release-gate: cannot resolve '$COMMIT'" >&2
  exit 2
fi

VERSION="$VERSION" COMMIT_EPOCH="$COMMIT_EPOCH" RUNS="$RUNS" python3 - <<'PY'
import calendar, glob, json, os, sys, time

version = os.environ["VERSION"]
commit_epoch = int(os.environ["COMMIT_EPOCH"])
runs = os.environ["RUNS"]

# A dev build of the version under test is the right evidence: you build it,
# click it, and only then tag. The bare "dev" that one run recorded identifies no
# build at all and is never a match.
wanted = {version, f"{version}-dev"}

candidates = []
for path in glob.glob(os.path.join(runs, "*", "*", "run.json")):
    try:
        with open(path) as fh:
            rec = json.load(fh)
    except (OSError, ValueError):
        continue          # a half-written record is not evidence; ignore it
    if rec.get("app_version") not in wanted:
        continue
    try:
        finished = calendar.timegm(time.strptime(rec["finished"], "%Y-%m-%dT%H:%M:%SZ"))
    except (KeyError, ValueError):
        continue
    candidates.append((finished, path, rec))

if not candidates:
    print(f"REFUSED: no GUI smoke run for {version}.")
    print(f"  Looked in {runs}/<host>/<id>/run.json for app_version in {sorted(wanted)}.")
    print(f"  Build it, click it, then tag:")
    print(f"    cd src-tauri && cargo tauri build")
    print(f"    APP=src-tauri/target/release/bundle/macos/xNAUT.app scripts/gui-smoke.sh")
    sys.exit(1)

candidates.sort()
finished, path, rec = candidates[-1]

if finished < commit_epoch:
    print(f"REFUSED: the newest run for {version} predates the commit being released.")
    print(f"  run {rec['id']} on {rec.get('host')} finished {rec['finished']}")
    print(f"  commit is newer by {(commit_epoch - finished) // 60} min — it has never been clicked.")
    sys.exit(1)

bad = [c for c in rec.get("cases", []) if c.get("status") != "passed"]
if bad:
    print(f"REFUSED: run {rec['id']} on {rec.get('host')} did not pass.")
    for c in bad:
        print(f"  {c['status']:<9} {c['id']}: {c.get('note', c.get('title', ''))}")
    print(f"  {path}")
    sys.exit(1)

print(f"OK: {version} verified by run {rec['id']} on {rec.get('host')} ({rec['finished']}).")
for c in rec.get("cases", []):
    print(f"  passed    {c['id']}: {c.get('title', '')}")
PY
