# Control repository writes and maintenance

`read_only` pauses automatic PM writes, including receipts and implicit project
imports. A transaction already admitted completes under its lease. Explicit native
owner ticket/project commands and Synchronize still work; agent request fields
cannot grant this exception. The switch is therefore **not an exclusive maintenance
lock** and must never be used as permission for `git gc --prune=now`.

All app PM writers and bounded maintenance tasks acquire the same OS lock in the
common Git directory: `xnaut-ticket-update.lock`. Linked worktrees share it. The
name retains compatibility with the XNAUT-472 ticket-update lease. Older app
versions that did not acquire this lease, and arbitrary shell Git commands, do not
cooperate. Never run destructive prune, reflog expiry or reset alongside them.

For a cooperating shell maintenance task on macOS/Linux:

```sh
python3 scripts/control-repo-maintenance.py --repo /path/to/control --task loose-objects
```

The helper refuses a held lease and supports only Git's four bounded maintenance
tasks. It does not prune, expire, reset, change refs by hand or remove temporary
packs. On Windows use the native scheduler, which uses the portable OS lease.

Paused/busy jury receipts and Vault document events are retained beneath the common
Git directory at `xnaut-pm-state/deferred`. They create no Git objects while paused.
The normal sweep attempts at most 32 valid retained events for the current source
branch per pass after the switch is off. Corrupt files and other linked branches
cannot consume that attempt budget. A changed ticket revision holds the jury receipt for reconciliation instead
of overwriting an owner's decision. Corrupt evidence is retained and reported.
A payload-validated committed deterministic event makes replay safe if the app dies before deleting
the local outbox item. The original source job/document remains authoritative.

A failed push does not turn a successful local commit into a failed ticket write.
Its persistent fault appears in the PM board until a successful push or explicit
Synchronize clears it. Disk-pressure evidence includes the control repository's
loose-object count and bytes, plus the retained fault. Packing is independent of
push eligibility; no automatic history reset or destructive cleanup is attempted.

The local outbox and fault files are machine-local evidence, not a second ticket
store. Preserve them during recovery. A stale jury receipt needs deliberate
reconciliation from its original job; automatic replay never chooses over an
owner's newer ticket revision. Filesystem failure can also prevent retaining an
outbox record; that failure is reported through the ledger and stderr and is not
reported as a successful receipt write.
