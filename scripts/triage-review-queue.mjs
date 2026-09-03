#!/usr/bin/env node
// triage-review-queue: the deterministic half of the fleet run.
//
// Point 7 of the sprint: "heuristic scripts filter first, LLM judges fan out
// on the rest". This is the heuristic. It answers one question per ticket in
// `review`, using git rather than a model:
//
//   Did this ticket's work actually land, and did it reach anyone?
//
// The answer is in the commit messages, which carry the ticket id by
// convention, and in the tags, which say which release contains it. Run
// against the XNAUT board on 2026-09-04 it took 83 tickets down to 18 that
// need a human or a judge. The other 65 are all in v1.26.0, which is to say
// they are already on the owner's machine.
//
// WHAT IT DOES NOT DO: move anything. `complete` means tested and approved,
// and shipping is not testing. Whether "merged into a shipped release" is
// evidence enough to close a ticket is a standing decision the owner owes,
// not one a script gets to make quietly, and the whole sprint exists because
// things happened quietly. So this ranks and explains; the board is changed
// by someone who read it.
//
// Mainline note: `main` is a month stale on this repo. Releases ship from the
// live feature lineage, so the default ref is HEAD, not main. Pointing this at
// main reports 0 of 83 merged, which is how the ref got checked in the first
// place.
//
// WHAT `no-commit` DOES NOT MEAN: that the work was never done. It means no
// commit names the ticket, and several tickets were never going to have one:
// XNAUT-215 is a reply to an engineer, XNAUT-228 is a runbook, XNAUT-254 is
// the loop audit itself. The heuristic separates code that landed from
// everything else; deciding what "everything else" is worth is precisely the
// judgement it is handing on, which is why it hands it on rather than
// guessing.
//
// Usage:
//   node scripts/triage-review-queue.mjs                    report
//   node scripts/triage-review-queue.mjs --project XNAUT
//   node scripts/triage-review-queue.mjs --ref feat/x       lineage to search
//   node scripts/triage-review-queue.mjs --status ready     triage another queue
//   node scripts/triage-review-queue.mjs --eval-set <path>  write JSONL for the judges

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const argv = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i === -1 ? fallback : argv[i + 1];
};
const PROJECT = flag("project", "XNAUT");
const REF = flag("ref", "HEAD");
const STATUS = flag("status", "review");
const EVAL_SET = flag("eval-set", null);
const CONTROL = join(homedir(), ".xnaut-control", "projects", PROJECT, "tickets");

const git = (args) => execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });

const tickets = readdirSync(CONTROL)
  .filter((f) => f.endsWith(".json"))
  .map((f) => JSON.parse(readFileSync(join(CONTROL, f), "utf8")))
  .filter((t) => t.status === STATUS)
  // Numeric, so XNAUT-9 sorts before XNAUT-10 rather than after it.
  .sort((a, b) => Number(a.id.split("-").pop()) - Number(b.id.split("-").pop()));

if (!tickets.length) {
  console.log(`no ${PROJECT} tickets in ${STATUS}`);
  process.exit(0);
}

// One pass over the log rather than one `git log --grep` per ticket: 83
// invocations of git took longer than reading the whole history once.
// %H keeps the commit so a finding can be checked, not just believed.
const commits = git(["log", "--format=%H%x00%s%x00%b%x01", REF])
  .split("\x01")
  .map((entry) => entry.trim())
  .filter(Boolean)
  .map((entry) => {
    const [hash, subject, body = ""] = entry.split("\x00");
    return { hash, subject, text: `${subject}\n${body}` };
  });

// Newest first, so the first tag that contains a commit is the release it
// first reached. `git tag --contains` per commit would be correct and slow;
// one log per tag is bounded by the number of releases.
const tags = git(["tag", "--list", "v*", "--sort=v:refname"]).split("\n").filter(Boolean);
const tagContents = tags.map((tag) => ({
  tag,
  text: git(["log", "--format=%s%n%b", tag]),
}));

// The id must appear as a whole token: XNAUT-25 must not match XNAUT-250.
const mentions = (text, id) => new RegExp(`\\b${id}\\b`).test(text);

const rows = tickets.map((ticket) => {
  const landed = commits.filter((c) => mentions(c.text, ticket.id));
  if (!landed.length) {
    return { ticket, verdict: "no-commit", detail: `no commit on ${REF} names ${ticket.id}` };
  }
  const firstRelease = tagContents.find((t) => mentions(t.text, ticket.id));
  const proof = `${landed.length} commit(s), newest ${landed[0].hash.slice(0, 8)}`;
  return firstRelease
    ? { ticket, verdict: "shipped", detail: `in ${firstRelease.tag}; ${proof}`, release: firstRelease.tag }
    : { ticket, verdict: "merged", detail: `on ${REF} but in no release yet; ${proof}` };
});

// Ranked by how much human attention the row needs, which is the inverse of
// how much evidence there is. A shipped ticket is the least interesting thing
// on the board and belongs at the bottom.
const ORDER = { "no-commit": 0, merged: 1, shipped: 2 };
rows.sort((a, b) => ORDER[a.verdict] - ORDER[b.verdict]);

const count = (v) => rows.filter((r) => r.verdict === v).length;
console.log(
  `${PROJECT}: ${rows.length} in ${STATUS}, searched ${REF}\n` +
    `  no-commit ${count("no-commit")}   needs a judge or a human\n` +
    `  merged    ${count("merged")}   landed, not yet released\n` +
    `  shipped   ${count("shipped")}   already in a release\n`,
);

for (const verdict of ["no-commit", "merged", "shipped"]) {
  const group = rows.filter((r) => r.verdict === verdict);
  if (!group.length) continue;
  console.log(`${verdict.toUpperCase()}`);
  for (const r of group) {
    console.log(`  ${r.ticket.id.padEnd(12)} ${r.ticket.title.slice(0, 62)}`);
    console.log(`  ${"".padEnd(12)} ${r.detail}`);
  }
  console.log();
}

if (EVAL_SET) {
  // The eval set point 7 asks for: one line per ticket, the deterministic
  // verdict and the evidence behind it, for the judges to fan out over and
  // for later runs to score themselves against. JSONL for the same reason
  // audit.rs is: one bad line costs only itself.
  const lines = rows.map((r) =>
    JSON.stringify({
      id: r.ticket.id,
      project: PROJECT,
      title: r.ticket.title,
      status: r.ticket.status,
      updated_at: r.ticket.updated_at,
      verdict: r.verdict,
      evidence: r.detail,
      release: r.release ?? null,
      needs_judgement: r.verdict === "no-commit",
      searched_ref: REF,
      generated_at: new Date().toISOString(),
    }),
  );
  writeFileSync(EVAL_SET, lines.join("\n") + "\n");
  console.log(`eval set: ${lines.length} rows -> ${EVAL_SET}`);
}

console.log(
  "Nothing was moved. `complete` means tested and approved, and shipping is not\n" +
    "testing; whether a shipped release is evidence enough to close a ticket is the\n" +
    "owner's standing decision, not this script's.",
);
