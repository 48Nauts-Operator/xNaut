#!/usr/bin/env bash
# What gui-smoke.sh writes, checked without a GUI.
#
# The run record is the only part of the smoke test that anyone reads later, and
# it is the part that has failed silently twice. Once when it was written to a
# directory the dashboard does not read, so eleven runs on tron were invisible.
# Once when two consecutive releases reported the identical "1 of 12 did not
# open: Settings" and shipped anyway, because nothing compared a run to the one
# before it.
#
# Both are now assertions here rather than hopes in a comment. This needs no
# display, no Accessibility grant and no app: it drives the emit block directly.
#
#   ./tests/gui-smoke-emit.test.sh

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FAILED=0

ok()   { printf '  ok   %s\n' "$*"; }
fail() { printf '  FAIL %s\n' "$*"; FAILED=1; }

# The emit block is a heredoc inside the script; run the same bytes the script
# runs rather than a copy that can drift.
sed -n "/^import json, os, pathlib/,/^PY$/p" "$REPO/scripts/gui-smoke.sh" | sed '$d' > "$TMP/emit.py"
[ -s "$TMP/emit.py" ] || { echo "could not extract the emit block from gui-smoke.sh"; exit 1; }

emit() { # emit <outdir> <VERIF> <UNVERIF> <BAD> <EMPTY>
  mkdir -p "$1"
  OUT="$1" STARTED=2026-01-01T00:00:00Z FINISHED=2026-01-01T00:05:00Z \
  APP_VER="${VER:-1.0.0}" APP_PID=123 TASK="" \
  SURF_OK="$2
$3" SURF_BAD="$4" SURF_VERIF="$2" SURF_UNVERIF="$3" SURF_EMPTY="$5" \
  python3 "$TMP/emit.py" >/dev/null
}
note() { python3 -c "
import json,sys
d=json.load(open('$1/run.json'))
print([c for c in d['cases'] if c['id']=='surfaces'][0][sys.argv[1]])" "$2"; }

echo "== the run directory is the one the dashboard reads"
# The bug: gui-smoke wrote ~/xnaut-gui-smoke/<ts>/ while testing-report.mjs read
# ~/xnaut-testing/runs/<host>/<id>/. Nothing bridged them and nothing errored.
smoke_out=$(grep -m1 '^OUT="\${OUT:-' "$REPO/scripts/gui-smoke.sh")
report_root=$(grep -m1 "xnaut-testing" "$REPO/scripts/testing-report.mjs")
case "$smoke_out" in
  *xnaut-testing/runs*) ok "gui-smoke writes under xnaut-testing/runs" ;;
  *) fail "gui-smoke OUT default is not under xnaut-testing/runs: $smoke_out" ;;
esac
case "$report_root" in
  *xnaut-testing/runs*) ok "testing-report reads the same root" ;;
  *) fail "testing-report ROOT moved away from xnaut-testing/runs: $report_root" ;;
esac

echo "== a surface that opens blank is a failure, not a pass"
# tests/features/smoke.feature says so; the script recorded a press as a pass
# until 2026-08-10, which made the whole walk a reachability check.
emit "$TMP/blank" "Help" "" "" "Browser"
[ "$(note "$TMP/blank" status)" = failed ] \
  && ok "status failed" || fail "a blank surface did not fail the run"
case "$(note "$TMP/blank" note)" in
  *"opened blank: Browser"*) ok "the blank surface is named" ;;
  *) fail "note does not name the blank surface: $(note "$TMP/blank" note)" ;;
esac

echo "== pressed-but-unverifiable is its own state, never passed"
emit "$TMP/part" "Help" "Refresh usage" "" ""
[ "$(note "$TMP/part" status)" = partial ] \
  && ok "status partial" || fail "unverifiable surfaces reported as $(note "$TMP/part" status)"

echo "== the same failure twice is escalated, not repeated quietly"
# 1.13.8 and 1.13.9 both reported "1 of 12 did not open: Settings" and both
# shipped. A reader comparing two runs by hand is the thing that did not happen.
mkdir -p "$TMP/hist"
VER=1.13.9 emit "$TMP/hist/20260810-100000" "Help" "" "Settings" ""
VER=1.13.10 emit "$TMP/hist/20260810-110000" "Help" "" "Settings" ""
case "$(note "$TMP/hist/20260810-110000" note)" in
  *UNCHANGED*Settings*) ok "repeat named with the run and version it repeats" ;;
  *) fail "repeat not escalated: $(note "$TMP/hist/20260810-110000" note)" ;;
esac

echo "== a different failure is not a repeat"
mkdir -p "$TMP/hist2"
VER=1.13.9 emit "$TMP/hist2/20260810-100000" "Help" "" "Settings" ""
VER=1.13.10 emit "$TMP/hist2/20260810-110000" "Help" "" "Worktrees" ""
case "$(note "$TMP/hist2/20260810-110000" note)" in
  *UNCHANGED*) fail "a new failure was reported as unchanged" ;;
  *) ok "new failure not mislabelled" ;;
esac

echo "== nothing pressed is untested, not passed"
# The distinction the release-test skill calls the specific mistake that let the
# Designer ship broken three times in one day.
emit "$TMP/none" "" "" "" ""
[ "$(note "$TMP/none" status)" = untested ] \
  && ok "status untested" || fail "an empty run reported $(note "$TMP/none" status)"

[ $FAILED -eq 0 ] && echo "PASS" || echo "FAIL"
exit $FAILED
