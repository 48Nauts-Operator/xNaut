#!/usr/bin/env node
// housekeeper — reclaim the disk that agent worktrees quietly eat.
//
// Every subagent gets its own git worktree, and the first `cargo test` in it
// builds a fresh Rust target directory of roughly 6 GB. The worktree stays
// after the agent finishes. Twenty-six agents over two days took 136 GB and
// the disk hit 98% before anyone noticed, twice in four days.
//
// So this is not a tidying script, it is a brake. It answers one question per
// worktree, "is anything here that does not exist somewhere safer", and only
// then offers to remove it.
//
// SAFETY, in order of how much they matter:
//   1. Uncommitted changes are never touched. Not counted, not cleaned.
//   2. A branch whose HEAD is not already in the mainline is never touched.
//      The branch is the deliverable; the worktree is scaffolding.
//   3. `target/` is only ever cleared, never the source beside it, and only
//      where a build can regenerate it.
//   4. Nothing runs without --apply. The default is a report.
//
// Removing a worktree does NOT delete its branch, which is why this is safe
// at all: git keeps the ref, so the work survives even when the directory
// does not.
//
// Usage:
//   node scripts/housekeeper.mjs              report only, touches nothing
//   node scripts/housekeeper.mjs --apply      do the safe removals
//   node scripts/housekeeper.mjs --apply --targets-only
//                                             keep every worktree, clear the
//                                             build caches inside stale ones
//   --mainline <ref>   what counts as "already merged"   (default: HEAD here)
//   --stale-days <n>   a target dir is stale after this  (default: 3)

import { execFileSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { join } from "node:path";

const argv = process.argv.slice(2);
const APPLY = argv.includes("--apply");
const TARGETS_ONLY = argv.includes("--targets-only");
const flag = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i === -1 ? fallback : argv[i + 1];
};
const STALE_DAYS = Number(flag("stale-days", 3));

const git = (args, cwd) => {
  try {
    return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
  } catch {
    return "";
  }
};
const bytes = (path) => {
  try {
    return Number(execFileSync("du", ["-sk", path], { encoding: "utf8" }).split(/\s+/)[0]) * 1024;
  } catch {
    return 0;
  }
};
const gb = (n) => `${(n / 1024 ** 3).toFixed(1)}G`;

const root = git(["rev-parse", "--show-toplevel"], process.cwd());
if (!root) {
  console.error("not inside a git repository");
  process.exit(1);
}
const MAINLINE = flag("mainline", git(["rev-parse", "HEAD"], root));

// `git worktree list --porcelain` is the only listing that includes entries
// whose directory is already gone, which is exactly the state a half-finished
// cleanup leaves behind.
const worktrees = [];
let current = null;
for (const line of git(["worktree", "list", "--porcelain"], root).split("\n")) {
  if (line.startsWith("worktree ")) {
    current = { path: line.slice(9), branch: "", head: "" };
    worktrees.push(current);
  } else if (line.startsWith("branch ") && current) {
    current.branch = line.slice(7).replace("refs/heads/", "");
  } else if (line.startsWith("HEAD ") && current) {
    current.head = line.slice(5);
  }
}

const rows = worktrees.map((w) => {
  const missing = !existsSync(w.path);
  const isMain = w.path === root;
  const isAgent = w.path.includes("/.claude/worktrees/agent-");
  const dirty = missing ? 0 : git(["status", "--porcelain"], w.path).split("\n").filter(Boolean).length;
  let inMainline = false;
  try {
    execFileSync("git", ["merge-base", "--is-ancestor", w.head, MAINLINE], { cwd: root, stdio: "ignore" });
    inMainline = true;
  } catch {
    inMainline = false;
  }
  const target = join(w.path, "src-tauri", "target");
  const hasTarget = !missing && existsSync(target);
  const targetAgeDays = hasTarget
    ? (Date.now() - statSync(target).mtimeMs) / 86_400_000
    : 0;
  return {
    ...w,
    missing,
    isMain,
    isAgent,
    dirty,
    inMainline,
    size: missing ? 0 : bytes(w.path),
    target: hasTarget ? target : null,
    targetSize: hasTarget ? bytes(target) : 0,
    targetAgeDays,
  };
});

// A worktree is removable only when nothing in it is unique: no uncommitted
// changes, and a HEAD the mainline already contains. `missing` entries are
// registrations whose directory is gone, which prune alone sometimes leaves.
const removable = rows.filter(
  (r) => !r.isMain && !r.dirty && (r.missing || (r.inMainline && r.isAgent)),
);
// Clearing a build cache is always safe; it is only worth doing when the
// worktree has been idle, so an active one is left alone.
// `!r.dirty` is not paranoia, it is rule 1 above. The first version of this
// listed one worktree as both CLEARABLE and KEPT: 27G of build cache beside an
// uncommitted file. Clearing target/ next to unsaved work is probably safe and
// definitely contradicts what the report says it is doing, and a housekeeper
// nobody trusts does not get run.
const clearable = rows.filter(
  (r) => r.target && !r.isMain && !r.dirty && r.targetAgeDays > STALE_DAYS && !removable.includes(r),
);
const kept = rows.filter((r) => !removable.includes(r) && !r.isMain);

console.log(`mainline: ${MAINLINE.slice(0, 8)}   worktrees: ${rows.length}\n`);

if (removable.length) {
  console.log(`REMOVABLE  (merged agent worktrees and dead registrations)  ${gb(removable.reduce((a, r) => a + r.size, 0))}`);
  for (const r of removable) {
    const why = r.missing ? "directory already gone" : "branch is in the mainline";
    console.log(`  ${gb(r.size).padStart(6)}  ${r.branch || "(detached)"}  ${why}`);
  }
  console.log();
}
if (clearable.length) {
  console.log(`CLEARABLE  (build caches idle > ${STALE_DAYS}d, source untouched)  ${gb(clearable.reduce((a, r) => a + r.targetSize, 0))}`);
  for (const r of clearable) {
    console.log(`  ${gb(r.targetSize).padStart(6)}  ${r.branch || "(detached)"}  idle ${r.targetAgeDays.toFixed(0)}d`);
  }
  console.log();
}
const risky = kept.filter((r) => r.dirty || (!r.inMainline && !r.missing));
if (risky.length) {
  console.log("KEPT  (something here exists nowhere else)");
  for (const r of risky) {
    const why = r.dirty ? `${r.dirty} uncommitted file(s)` : "HEAD is not in the mainline";
    console.log(`  ${gb(r.size).padStart(6)}  ${r.branch || "(detached)"}  ${why}`);
  }
  console.log();
}

if (!APPLY) {
  const would = removable.reduce((a, r) => a + r.size, 0) + clearable.reduce((a, r) => a + r.targetSize, 0);
  console.log(`would reclaim ${gb(would)}. Nothing was touched; re-run with --apply.`);
  process.exit(0);
}

let freed = 0;
if (!TARGETS_ONLY) {
  for (const r of removable) {
    freed += r.size;
    try {
      execFileSync("git", ["worktree", "remove", "--force", r.path], { cwd: root, stdio: "ignore" });
    } catch {
      execFileSync("rm", ["-rf", r.path], { stdio: "ignore" });
    }
    console.log(`removed  ${r.branch || "(detached)"}`);
  }
  execFileSync("git", ["worktree", "prune"], { cwd: root, stdio: "ignore" });
}
for (const r of clearable) {
  freed += r.targetSize;
  execFileSync("rm", ["-rf", r.target], { stdio: "ignore" });
  console.log(`cleared  ${r.branch || "(detached)"}/src-tauri/target`);
}
console.log(`\nreclaimed ${gb(freed)}`);
