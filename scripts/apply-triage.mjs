#!/usr/bin/env node
// apply-triage: move the board once the triage and the judges have spoken.
//
// The other half of scripts/triage-review-queue.mjs. That one ranks and
// explains and deliberately changes nothing; this one applies a decision the
// owner has made, with the evidence for each move recorded in the commit.
//
// It is a separate file on purpose. A tool that both decides and applies is one
// bug away from moving 76 tickets nobody asked it to move, and this sprint
// exists because things happened quietly.
//
// SAFETY:
//   - Dry run by default. --apply is required.
//   - A ticket whose status is no longer what the triage saw is SKIPPED, not
//     overwritten. The sweep writes to this board every 180 seconds, so the
//     read-to-write window is real, not theoretical.
//   - A ticket that no longer exists is skipped. XNAUT-215 was deleted between
//     the judges running and this being written.
//   - Each move writes the ticket JSON and an event, and the whole batch is ONE
//     commit, so `git revert` undoes the entire operation.
//
// Inputs:
//   --eval-set   <path>   the JSONL from triage-review-queue.mjs
//   --judgements <path>   optional JSONL: {id, recommended_status, evidence}
//                         Overrides the eval set's verdict for the ids it names.
//
// Mapping from the eval set, for anything the judgements do not name:
//   shipped  -> complete    the work is in a release the owner is running
//   merged   -> (skipped)   landed but unreleased; not evidence enough
//   no-commit-> (skipped)   needs a judge, and that is what --judgements is
//
// Usage:
//   node scripts/apply-triage.mjs --eval-set /tmp/e.jsonl --judgements /tmp/j.jsonl
//   node scripts/apply-triage.mjs --eval-set /tmp/e.jsonl --judgements /tmp/j.jsonl --apply

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";

const argv = process.argv.slice(2);
const APPLY = argv.includes("--apply");
const flag = (n, d = null) => {
  const i = argv.indexOf(`--${n}`);
  return i === -1 ? d : argv[i + 1];
};
const CONTROL = join(homedir(), ".xnaut-control");
const readJsonl = (p) =>
  !p || !existsSync(p)
    ? []
    : readFileSync(p, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));

const evalSet = readJsonl(flag("eval-set"));
if (!evalSet.length) {
  console.error("no eval set; run triage-review-queue.mjs --eval-set <path> first");
  process.exit(1);
}
const judged = new Map(readJsonl(flag("judgements")).map((r) => [r.id, r]));

// What each ticket should become, and why. The why is not decoration: it is
// what goes in the commit, and it is the only thing that makes a 76-ticket
// move reviewable afterwards.
const VERDICT_TARGET = { shipped: "complete" };
const planned = evalSet
  .map((row) => {
    const judgement = judged.get(row.id);
    const target = judgement?.recommended_status ?? VERDICT_TARGET[row.verdict] ?? null;
    const why = judgement?.evidence ?? row.evidence;
    return { id: row.id, from: row.status, to: target, why, source: judgement ? "judge" : "triage" };
  })
  .filter((m) => m.to && m.to !== m.from);

const ticketPath = (id) =>
  join(CONTROL, "projects", id.split("-")[0], "tickets", `${id}.json`);

const results = { moved: [], skipped: [] };
const stamp = new Date().toISOString().replace(/\.\d+Z$/, "Z");
const touched = [];

for (const move of planned) {
  const path = ticketPath(move.id);
  if (!existsSync(path)) {
    results.skipped.push({ ...move, reason: "ticket no longer exists" });
    continue;
  }
  const ticket = JSON.parse(readFileSync(path, "utf8"));
  // The board is live. If the status moved since the triage read it, someone
  // (or the sweep) knows something this plan does not.
  if (ticket.status !== move.from) {
    results.skipped.push({
      ...move,
      reason: `status is now "${ticket.status}", was "${move.from}" when triaged`,
    });
    continue;
  }
  results.moved.push(move);
  if (!APPLY) continue;

  ticket.status = move.to;
  ticket.revision = (ticket.revision ?? 1) + 1;
  ticket.updated_at = stamp;
  writeFileSync(path, JSON.stringify(ticket, null, 2));
  const eventPath = join(
    CONTROL,
    "events",
    `${stamp.replace(/[-:]/g, "").replace("Z", ".000Z")}-${randomUUID()}.json`,
  );
  mkdirSync(join(CONTROL, "events"), { recursive: true });
  writeFileSync(
    eventPath,
    JSON.stringify(
      {
        version: 1,
        event: "ticket.updated",
        subject: move.id,
        timestamp: stamp,
        details: { status: move.to, revision: ticket.revision, evidence: move.why },
      },
      null,
      2,
    ),
  );
  touched.push(path, eventPath);
}

const byTarget = {};
for (const m of results.moved) (byTarget[m.to] ??= []).push(m.id);
console.log(`${results.moved.length} to move, ${results.skipped.length} skipped\n`);
for (const [target, ids] of Object.entries(byTarget)) {
  console.log(`-> ${target} (${ids.length})`);
  console.log(`   ${ids.join(", ")}\n`);
}
if (results.skipped.length) {
  console.log("SKIPPED");
  for (const s of results.skipped) console.log(`  ${s.id.padEnd(12)} ${s.reason}`);
  console.log();
}

if (!APPLY) {
  console.log("Dry run. Nothing was written. Re-run with --apply.");
  process.exit(0);
}

const lines = results.moved.map((m) => `${m.id}: ${m.from} -> ${m.to} (${m.source}) ${m.why}`);
const message =
  `feat(pm): settle the review queue against release evidence\n\n` +
  `${results.moved.length} tickets moved. Every one carries the evidence that moved it:\n` +
  `a commit that is an ancestor of the shipped lineage, and the release tag that\n` +
  `contains it, or a judge's verdict naming a path on disk.\n\n` +
  `Nothing here was decided by this script. The deterministic half is\n` +
  `scripts/triage-review-queue.mjs and it moves nothing; the judgements are a\n` +
  `separate file. One commit, so a single revert undoes all of it.\n\n` +
  lines.join("\n") +
  `\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>\n`;

execFileSync("git", ["add", "--", ...touched], { cwd: CONTROL });
execFileSync(
  "git",
  ["-c", "user.name=xNaut", "-c", "user.email=xnaut@local", "commit", "-q", "-F", "-"],
  { cwd: CONTROL, input: message },
);
console.log(`committed ${results.moved.length} moves`);
