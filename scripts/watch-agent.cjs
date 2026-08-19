#!/usr/bin/env node
// Make a headless agent watchable.
//
// `claude -p --output-format stream-json` writes one JSON object per line, and
// a 400 KB log of those is unreadable by a person. This renders the same stream
// as lines you can follow: what it said, what it ran, what came back, and what
// broke. Pipe a log into it, or point it at a file to follow.
//
//   node scripts/watch-agent.cjs <log>       follow a running agent
//   tail -f agent.log | node scripts/watch-agent.cjs
//
// Written 2026-08-19, when six agents were running overnight with no way to see
// any of them. An agent you cannot watch is an agent you cannot trust.

const { createReadStream, statSync, watchFile } = require('node:fs');
const readline = require('node:readline');

const DIM = '\x1b[2m';
const BOLD = '\x1b[1m';
const RED = '\x1b[31m';
const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const BLUE = '\x1b[36m';
const OFF = '\x1b[0m';

const clip = (value, max = 140) => {
  const text = String(value ?? '').replace(/\s+/g, ' ').trim();
  return text.length > max ? `${text.slice(0, max)}…` : text;
};

const stamp = () => new Date().toTimeString().slice(0, 8);

// The one-line summary of a tool call is the argument a person would have
// wanted to see: the command, the path, the pattern. Not the whole payload.
function toolSummary(name, input) {
  const it = input || {};
  const first = it.command || it.file_path || it.path || it.pattern || it.query
    || it.notebook_path || it.url || it.prompt || '';
  return first ? `${name} ${DIM}${clip(first, 110)}${OFF}` : name;
}

function render(line) {
  let event;
  try {
    event = JSON.parse(line);
  } catch {
    return;
  }
  const at = `${DIM}${stamp()}${OFF}`;

  if (event.type === 'assistant' && event.message?.content) {
    for (const part of event.message.content) {
      if (part.type === 'text' && part.text.trim()) {
        console.log(`${at} ${BOLD}say${OFF}  ${clip(part.text, 400)}`);
      }
      if (part.type === 'tool_use') {
        console.log(`${at} ${BLUE}run${OFF}  ${toolSummary(part.name, part.input)}`);
      }
    }
    return;
  }

  if (event.type === 'user' && event.message?.content) {
    for (const part of event.message.content) {
      if (part.type !== 'tool_result') continue;
      const body = Array.isArray(part.content)
        ? part.content.map((c) => c.text || '').join(' ')
        : part.content;
      // Errors are the reason anyone opens this window.
      const colour = part.is_error ? RED : DIM;
      const label = part.is_error ? 'ERR ' : 'out ';
      console.log(`${at} ${colour}${label}${OFF} ${colour}${clip(body, 160)}${OFF}`);
    }
    return;
  }

  if (event.type === 'result') {
    const ok = event.subtype === 'success';
    console.log(`${at} ${ok ? GREEN : RED}${BOLD}${ok ? 'DONE' : 'ENDED'}${OFF} ${clip(event.result, 400)}`);
    if (event.total_cost_usd) {
      console.log(`${at} ${DIM}cost $${event.total_cost_usd.toFixed(2)}, ${event.num_turns} turns${OFF}`);
    }
    return;
  }

  if (event.type === 'system' && event.subtype === 'init') {
    console.log(`${at} ${YELLOW}start${OFF} ${event.cwd || ''}`);
  }
}

const path = process.argv[2];
if (!path) {
  readline.createInterface({ input: process.stdin }).on('line', render);
} else {
  // Follow: print what is there, then everything appended. watchFile rather
  // than fs.watch because a log written by another process on macOS does not
  // reliably raise rename/change events.
  let offset = 0;
  const pump = () => {
    let size;
    try {
      size = statSync(path).size;
    } catch {
      return;
    }
    if (size <= offset) return;
    const stream = createReadStream(path, { start: offset, end: size - 1, encoding: 'utf8' });
    offset = size;
    readline.createInterface({ input: stream }).on('line', render);
  };
  pump();
  watchFile(path, { interval: 500 }, pump);
}
