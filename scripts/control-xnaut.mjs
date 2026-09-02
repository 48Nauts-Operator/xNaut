#!/usr/bin/env node
// control-xnaut — the lever an agent drives xNAUT with (XNAUT-265).
//
// Every verification round on the rig used to re-invent its own harness out of
// cliclick, screencapture and throwaway shell scripts. That costs tokens, and
// nothing about the round was reproducible afterwards. This is the alternative
// the pstack post calls "build the lever": one small CLI, machine-readable
// output, so a round is a sequence of commands instead of a scripting problem.
//
// xNAUT cannot be driven over the Chrome DevTools Protocol — Tauri renders in
// WKWebView on macOS, which speaks Safari's inspector protocol — so the control
// surface is the app's own HTTP bridge plus macOS accessibility:
//
//   bridge  → doctor, eval, wake, sessions, verify records   (in-process truth)
//   AX tree → snapshot, click                                 (what is on screen)
//   screen  → screenshot                                      (evidence)
//
// Every command prints JSON on stdout and exits non-zero on failure, so an
// agent can branch on the result without parsing prose.
//
// Usage:
//   control-xnaut doctor
//   control-xnaut eval "window.xnautActiveProjectPath()"
//   control-xnaut snapshot [--grep Repository]
//   control-xnaut click "Open worktree manager"
//   control-xnaut screenshot /tmp/proof.png
//   control-xnaut wake claude "Check your tickets"
//   control-xnaut ledger --kind sweep --tail 20
//   control-xnaut wait-settle [--ms 2000]
//   control-xnaut watch [--every 10]        live one-line status, until you ctrl-c
//
//   --host tron.local     drive a remote rig instead of this machine
//   --ssh tron            its ssh name, when that differs from the http host
//   --json                already the default; kept so scripts can be explicit
//   --dry-run             print what would be done, touch nothing

import { execFileSync, execSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const argv = process.argv.slice(2);
const cmd = argv[0];
const flag = (name, fallback = null) => {
  const i = argv.indexOf(`--${name}`);
  return i === -1 ? fallback : (argv[i + 1] ?? true);
};
const positional = argv.slice(1).filter((a, i, all) => !a.startsWith("--") && !(all[i - 1] || "").startsWith("--"));
const HOST = flag("host", "127.0.0.1");
// The ssh name and the http name are not always the same: the rig answers HTTP
// on tron.local and ssh on the alias `tron` (different user). Assuming one name
// for both is how the first run of this CLI failed.
const SSH_HOST = flag("ssh", HOST);
const REMOTE = HOST !== "127.0.0.1" && HOST !== "localhost";
const DRY = argv.includes("--dry-run");

// Paths must resolve on the machine that reads them: the rig's home is not
// this machine's home, and hardcoding the local one silently looked in a
// directory that does not exist there.
const appFile = (name) => REMOTE
  ? `"$HOME/Library/Application Support/xnaut/${name}"`
  : JSON.stringify(join(homedir(), "Library/Application Support/xnaut", name));

const die = (msg, hint) => {
  console.error(JSON.stringify({ ok: false, error: msg, ...(hint ? { hint } : {}) }, null, 2));
  process.exit(1);
};
const out = (value) => console.log(JSON.stringify(value, null, 2));

// The bridge token lives beside the app's settings. On a remote rig it is read
// over ssh, which is also the honest test that the rig is reachable at all.
function bridgeToken() {
  if (REMOTE) {
    try {
      return execFileSync("ssh", ["-o", "BatchMode=yes", SSH_HOST,
        `jq -r .token ${appFile("mobile.json")}`], { encoding: "utf8" }).trim();
    } catch (e) {
      die(`cannot read the bridge token on ${HOST}`, "is the host reachable over ssh, and has xNAUT run there?");
    }
  }
  const local = join(homedir(), "Library/Application Support/xnaut/mobile.json");
  if (!existsSync(local)) die("no mobile.json", "start xNAUT once so the bridge writes its config");
  return JSON.parse(readFileSync(local, "utf8")).token;
}

async function bridge(path, { method = "GET", body } = {}) {
  const token = bridgeToken();
  const url = `http://${HOST}:8931${path}${path.includes("?") ? "&" : "?"}token=${token}`;
  if (DRY) return { dryRun: true, method, url: url.replace(token, "TOKEN") };
  let response;
  try {
    response = await fetch(url, { method, body, signal: AbortSignal.timeout(30000) });
  } catch (e) {
    die(`the bridge did not answer on ${HOST}:8931`, "is xNAUT running there? `control-xnaut doctor` checks");
  }
  const text = await response.text();
  if (!response.ok) die(`bridge ${response.status}: ${text.slice(0, 200)}`);
  try { return JSON.parse(text); } catch { return { raw: text }; }
}

// The accessibility tree is xNAUT's DOM as far as a tester is concerned: it is
// what the app actually renders, names included, and it survives theming.
function axDump() {
  const script = `
    tell application "System Events"
      tell process "xnaut"
        set out to ""
        repeat with e in (entire contents of window 1)
          try
            set r to (role of e as text) & " | " & (name of e as text) & " | " & (value of e as text)
            set out to out & r & linefeed
          end try
        end repeat
        return out
      end tell
    end tell`;
  const run = (s) => REMOTE
    ? execFileSync("ssh", ["-o", "BatchMode=yes", SSH_HOST, `osascript -e ${JSON.stringify(s)}`], { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 })
    : execFileSync("osascript", ["-e", s], { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 });
  try {
    return run(script);
  } catch (e) {
    die("could not read the accessibility tree",
      "grant Accessibility permission to the terminal running this, and check xNAUT is frontmost");
  }
}

// Doctor minus its clocks. A clock that stops moving is a dead app, not a
// settled one, so the fields that tick on their own are not evidence either way
// and must not keep `wait-settle` spinning until its budget runs out.
const CLOCK_FIELDS = /^(last_sweep_|sweep_ticks|last_status_tick_)/;
function settleShape(health) {
  return Object.fromEntries(
    Object.entries(health || {}).filter(([k]) => !CLOCK_FIELDS.test(k)),
  );
}

const COMMANDS = {
  // Health, plus the clocks. Every other field here stays green with the app's
  // periodic work dead behind it, which on 2026-09-02 cost an hour of ssh and a
  // timezone mistake to find out. `sweep` reads "never" when the loop has not
  // ticked at all, which is a different fact from a tick that found nothing.
  async doctor() {
    const health = await bridge("/api/control/doctor");
    if (!DRY && !("last_sweep_at" in health)) {
      health.sweep = "not reported by this build; it predates the doctor clock";
    } else if (!DRY) {
      const clock = (at, age, extra = "") =>
        at ? `${at} (${age}s ago${extra})` : "never; the loop has not ticked in this app's life";
      health.sweep = clock(health.last_sweep_at, health.last_sweep_age_secs,
        `, ${health.sweep_ticks} tick${health.sweep_ticks === 1 ? "" : "s"}`);
      health.status_clock = clock(health.last_status_tick_at, health.last_status_tick_age_secs);
      // 180s is the sweep interval; two missed ticks is a loop that is gone.
      if (health.last_sweep_age_secs === null || health.last_sweep_age_secs > 400) {
        health.warning = "the sweep clock is stale or has never ticked; the board is not being worked";
      }
    }
    out(health);
  },

  async eval() {
    const expression = positional[0];
    if (!expression) die("an expression is required", 'control-xnaut eval "window.xnautActiveProjectPath()"');
    const started = await bridge("/api/control/eval", { method: "POST", body: expression });
    if (DRY) return out(started);
    // The answer arrives in the app's debug log, tagged with the marker.
    const read = () => REMOTE
      ? execFileSync("ssh", ["-o", "BatchMode=yes", SSH_HOST, `grep -F ${started.marker} ${appFile("debug.log")} | tail -1`], { encoding: "utf8" })
      : execSync(`grep -F ${started.marker} ${appFile("debug.log")} | tail -1`, { encoding: "utf8" });
    for (let i = 0; i < 20; i += 1) {
      await new Promise((r) => setTimeout(r, 150));
      let line = "";
      try { line = read().trim(); } catch { /* not written yet */ }
      if (line) {
        const value = line.slice(line.indexOf(started.marker) + started.marker.length).trim();
        return out({ ok: !value.startsWith("ERR"), expression, value });
      }
    }
    die("the expression never answered", "the webview may be busy; try again or check the app is running");
  },

  snapshot() {
    const tree = axDump();
    const needle = flag("grep");
    const lines = tree.split("\n").filter(Boolean)
      .filter((l) => !needle || l.toLowerCase().includes(String(needle).toLowerCase()));
    out({ ok: true, lines: lines.length, tree: lines });
  },

  click() {
    const name = positional[0];
    if (!name) die("a name is required", 'control-xnaut click "Open worktree manager"');
    if (DRY) return out({ dryRun: true, wouldClick: name });
    const script = `
      tell application "System Events"
        tell process "xnaut"
          set hits to (every UI element of window 1 whose name is ${JSON.stringify(name)})
          if (count of hits) is 0 then error "no element named ${name}"
          click item 1 of hits
        end tell
      end tell`;
    try {
      REMOTE
        ? execFileSync("ssh", ["-o", "BatchMode=yes", SSH_HOST, `osascript -e ${JSON.stringify(script)}`], { encoding: "utf8" })
        : execFileSync("osascript", ["-e", script], { encoding: "utf8" });
      out({ ok: true, clicked: name });
    } catch (e) {
      die(`nothing named ${JSON.stringify(name)} could be clicked`,
        "run `snapshot --grep <part of the name>` to see what the app is actually rendering");
    }
  },

  screenshot() {
    const path = positional[0] || "/tmp/xnaut-proof.png";
    if (DRY) return out({ dryRun: true, wouldWriteTo: path });
    try {
      REMOTE
        ? execFileSync("ssh", ["-o", "BatchMode=yes", SSH_HOST, `screencapture -x ${JSON.stringify(path)}`])
        : execFileSync("screencapture", ["-x", path]);
      out({ ok: true, path, host: HOST });
    } catch (e) {
      die("screencapture failed", "grant Screen Recording permission to the terminal running this");
    }
  },

  async wake() {
    const handle = positional[0];
    const message = positional[1] || "Check your tickets.";
    if (!handle) die("an agent handle is required", 'control-xnaut wake claude "Check your tickets"');
    out(await bridge(`/api/agents/${encodeURIComponent(handle)}/wake`, { method: "POST", body: message }));
  },

  ledger() {
    const kind = flag("kind");
    const tail = Number(flag("tail", 20));
    const raw = REMOTE
      ? execFileSync("ssh", ["-o", "BatchMode=yes", SSH_HOST, `tail -400 ${appFile("agent-ledger.jsonl")}`], { encoding: "utf8" })
      : execSync(`tail -400 ${appFile("agent-ledger.jsonl")}`, { encoding: "utf8" });
    const rows = raw.split("\n").filter(Boolean)
      .map((l) => { try { return JSON.parse(l); } catch { return null; } })
      .filter(Boolean)
      .filter((r) => !kind || String(r.kind).startsWith(String(kind)));
    out({ ok: true, count: rows.length, entries: rows.slice(-tail) });
  },

  // Visibility you do not have to ask a model for.
  //
  // Written 2026-09-01, after an hour in which an agent was working the whole
  // time and the only way to know it was to ask me. One line per tick, small
  // enough to leave running in a corner: what the fleet is doing, whether the
  // report is growing, and the last thing the agent actually printed.
  async watch() {
    const every = Math.max(2, Number(flag("every", 10))) * 1000;
    const reportPath = flag("report", "/tmp/xnaut-test-report.md");
    const sh = (script) => {
      try {
        return REMOTE
          ? execFileSync("ssh", ["-o", "BatchMode=yes", SSH_HOST, script], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 })
          : execSync(script, { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
      } catch { return ""; }
    };
    let lastReport = -1;
    for (;;) {
      const health = await bridge("/api/control/doctor").catch(() => null);
      const sessions = await bridge("/api/sessions").catch(() => null);
      const rows = Array.isArray(sessions) ? sessions : (sessions?.sessions ?? []);
      const agents = rows
        .filter((r) => (r.label || "") !== "shell")
        .map((r) => `${r.label || r.agent_id || "agent"}:${r.status || "?"}`)
        .join(" ") || "none";
      const lines = Number(sh(`wc -l < ${JSON.stringify(reportPath)} 2>/dev/null || echo 0`).trim()) || 0;
      const growth = lastReport < 0 || lines === lastReport ? "" : ` +${lines - lastReport}`;
      lastReport = lines;
      // The last thing the agent actually put on screen: the difference
      // between "a process exists" and "it is doing something".
      const live = sh(`/opt/homebrew/bin/zellij list-sessions -n 2>/dev/null | grep -v EXITED | awk '{print $1}' | grep '^xnaut-' | tail -1`).trim();
      const screen = live
        // Claude Code marks its own activity: ⏺ a tool call, ⎿ its result, ✳/✽ the
        // spinner. Everything else on screen is chrome, and the permissions banner
        // is always the last raw line, so `tail -1` reports furniture, not work.
        ? sh(`/opt/homebrew/bin/zellij --session ${live} action dump-screen 2>/dev/null | sed 's/\\x1b\\[[0-9;]*m//g' | grep -E '[⏺✳✽]' | grep -v 'Tip:' | tail -1`).trim().slice(0, 88)
        : "";
      const stamp = new Date().toTimeString().slice(0, 8);
      console.log(
        `${stamp}  ${agents}  │ zellij ${health?.zellij_sessions ?? "?"}` +
        `  │ report ${lines}${growth}` +
        (screen ? `  │ ${screen}` : "  │ (no agent screen)"),
      );
      await new Promise((r) => setTimeout(r, every));
    }
  },

  // Not a sleep with a nice name: it waits for the app to stop producing
  // output, which is what "settled" actually means for an agent-driven UI.
  //
  // The clocks are excluded from that judgement. A heartbeat is SUPPOSED to
  // move every tick, so comparing raw doctor payloads would mean the app never
  // settles again the moment those fields exist.
  async waitSettle() {
    const budget = Number(flag("ms", 4000));
    const step = 300;
    let last = null;
    let stable = 0;
    for (let waited = 0; waited < budget; waited += step) {
      await new Promise((r) => setTimeout(r, step));
      const health = await bridge("/api/control/doctor");
      const shape = JSON.stringify(settleShape(health));
      if (shape === last) { stable += 1; } else { stable = 0; last = shape; }
      if (stable >= 2) return out({ ok: true, settled: true, waitedMs: waited });
    }
    out({ ok: true, settled: false, waitedMs: budget, note: "still changing when the budget ran out" });
  },
};

const ALIASES = { "wait-settle": "waitSettle" };
const chosen = COMMANDS[ALIASES[cmd] || cmd];

if (!cmd || cmd === "help" || cmd === "--help") {
  console.log(readFileSync(new URL(import.meta.url)).toString().split("\n")
    .filter((l) => l.startsWith("//")).map((l) => l.replace(/^\/\/ ?/, "")).join("\n"));
  process.exit(cmd ? 0 : 1);
}
if (!chosen) die(`unknown command ${JSON.stringify(cmd)}`, `known: ${Object.keys(COMMANDS).concat(Object.keys(ALIASES)).join(", ")}`);
await chosen();
