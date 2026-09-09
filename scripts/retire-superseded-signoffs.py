#!/usr/bin/env python3
"""One-off cleanup for the backlog left behind by the sign-off drift bug.

Until a8ebfe4 the sign-off gate refused to review whenever the worktree it
read was not parked on the ticket's verified commit. One worktree serves many
tickets, so at most one ticket could ever pass and every other one escalated
to the owner. Nothing retired the previous escalation, so each sweep pass
added another: 293 sign-off jobs across 17 tickets by 2026-09-10, every one
carrying an inbox ask that had already been archived. The owner saw a queue of
decisions with nowhere left to write.

The code fix stops new ones. This retires the ones already on disk, applying
exactly the rule the fixed code now applies.

  - Only `signoff` jobs in state `owner_required` are touched.
  - Plan gates waiting on the owner are left alone; a plan is a different
    question and some of those are real.
  - Anything already decided (integrated, reverted, settled) is history.

Run with --dry-run first. It prints what it would change and writes nothing.

    python3 scripts/retire-superseded-signoffs.py --dry-run
    python3 scripts/retire-superseded-signoffs.py

Back up the jury store before the real run:

    tar czf ~/jury-backup.tgz -C ~/.config/xnaut/registry jury

The control repo backs itself up: it is git, so `git -C ~/.xnaut-control
reset --hard <sha>` undoes the ticket half. This script does not commit;
review `git -C ~/.xnaut-control diff` and commit yourself.
"""

import argparse
import collections
import datetime
import glob
import json
import os
import sys

NOTE = (
    "Superseded by the sign-off fix of 2026-09-10: this escalation came from a "
    "shared worktree reporting drift it had no standing to report, not from "
    "anything about this ticket.\n"
)

CONTROL = os.path.expanduser("~/.xnaut-control")
STORE = os.path.expanduser("~/.config/xnaut/registry/jury")


def stale(job):
    return job.get("gate") == "signoff" and job.get("state") == "owner_required"


def retire(job):
    job["state"] = "superseded"
    job["reason"] = NOTE + (job.get("reason") or "")
    return job


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    write = not args.dry_run

    if not os.path.isdir(CONTROL):
        sys.exit("control repo not found at %s" % CONTROL)

    tickets = collections.Counter()
    for path in glob.glob(os.path.join(CONTROL, "projects/*/tickets/*.json")):
        with open(path) as fh:
            record = json.load(fh)
        # `jury_reviews` is flattened to the top level on disk, NOT nested
        # under `approval` (which serialises as null). Reading the nested key
        # reports zero reviews on every ticket in the repo.
        hits = [j for j in (record.get("jury_reviews") or []) if stale(j)]
        if not hits:
            continue
        for job in hits:
            retire(job)
        record["revision"] = int(record.get("revision", 1)) + 1
        record["updated_at"] = now
        tickets[record["id"]] = len(hits)
        if write:
            with open(path, "w") as fh:
                json.dump(record, fh, indent=2)
            event = {
                "type": "ticket.updated",
                "ticket_id": record["id"],
                "project": record["project"],
                "at": now,
                "revision": record["revision"],
                "summary": "retired %d superseded sign-off escalations" % len(hits),
            }
            name = "ticket-updated-%s-signoff-cleanup.json" % record["id"]
            with open(os.path.join(CONTROL, "events", name), "w") as fh:
                json.dump(event, fh)

    jobs = 0
    for path in glob.glob(os.path.join(STORE, "*.json")):
        try:
            with open(path) as fh:
                job = json.load(fh)
        except (ValueError, OSError):
            continue
        if not isinstance(job, dict) or not stale(job):
            continue
        jobs += 1
        if write:
            with open(path, "w") as fh:
                json.dump(retire(job), fh, indent=2)

    print("%s%d tickets, %d escalations on them, %d jobs in the jury store" % (
        "would retire: " if args.dry_run else "retired: ",
        len(tickets), sum(tickets.values()), jobs))
    for key, count in tickets.most_common():
        print("   %-16s %d" % (key, count))
    if args.dry_run:
        print("\nnothing written. drop --dry-run to apply.")
    else:
        print("\nreview and commit:  git -C %s diff --stat" % CONTROL)


if __name__ == "__main__":
    main()
