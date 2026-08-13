#!/bin/zsh
# Run the test suites on tron, against THIS worktree's code.
#
# The sync is step one on purpose. Tron cannot reach Forgejo (no route to
# cosmos), so nothing there self-updates: ~/xnaut-dev sat three days stale and a
# run against it silently tested old code. Pushing the tree first is what makes
# the result mean anything.
#
# Run:  ./scripts/tron-test.sh
#
# ponytail: rsync over ssh, because the pull direction does not work from tron.
# If Forgejo ever becomes reachable there, swap step one for a git fetch.

set -u
REPO=${0:a:h:h}
REMOTE=tron
DEST=/Users/zelda/xnaut-dev

print "===== sync $(git -C $REPO rev-parse --short HEAD) -> $REMOTE:$DEST ====="
git -C $REPO rev-parse --short HEAD > $REPO/.git-commit
rsync -a -e ssh --delete \
  --exclude .git --exclude target --exclude node_modules \
  $REPO/ $REMOTE:$DEST/ || exit 1
rm -f $REPO/.git-commit

ssh $REMOTE 'zsh -lc '"'"'
set -u
cd /Users/zelda/xnaut-dev || exit 1

hdr() { print "\n===== $1 ====="; }

hdr "env"
print "host   $(hostname)"
print "node   $(node -v)"
print "cargo  $(cargo --version)"
print "commit $(cat .git-commit)"
print "version $(grep -m1 "^version" src-tauri/Cargo.toml)"

hdr "phase 1: npm install"
npm install --no-audit --no-fund 2>&1 | tail -2

hdr "phase 2: node smoke scripts"
for f in scripts/*-smoke.cjs; do
  if node "$f" >/dev/null 2>&1; then print "PASS  $f"; else print "FAIL  $f"; fi
done

hdr "phase 3: playwright"
npx playwright test 2>&1 | tail -3

hdr "phase 4: cargo test --bin xnaut"
(cd src-tauri && cargo test --bin xnaut 2>&1 | tail -4)

hdr "phase 5: mutation check"
node scripts/mutation-check.cjs --all 2>&1 | tail -12

hdr "done"
'"'"''
