// Run the Wiki tab's extraction against a REAL docs page, outside the app.
//
// Why it exists: the extractor is the part of XNAUT-438 that can only be
// trusted by pointing it at sites nobody wrote a fixture for. The Playwright
// suite proves the rule on a saved page; this proves the rule on adk.dev, or
// on whatever docs site is about to be read for the first time. It is also
// how you debug "this site renders badly" without opening the app.
//
// It is the same two steps the app takes, in the same order and with the same
// code:
//   1. FETCH through a non-browser client (Node here, reqwest in wiki.rs),
//      because CORS is what stops the webview doing it. Same User-Agent, so a
//      CDN that refuses one refuses both.
//   2. EXTRACT with src/js/wiki-pane.js and the bundled Readability, loaded
//      into a blank page. Not a copy of the logic: the files the app ships.
//
// Usage:
//   node scripts/wiki-capture.mjs <url> [out.json]
//
// Writes { url, title, nav, markdown, html } as JSON (default:
// .xnaut/wiki-capture.json) and prints a summary. Feed the JSON to
// `wiki::tests::record_the_real_page_from_a_capture` to write it into the
// vault with the app's own writer.
import { chromium } from '@playwright/test';
import { writeFile, mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../', import.meta.url));

// Identical to wiki.rs's UA. A different one here would mean this script and
// the app get different answers from the same CDN, which is the one thing a
// debugging tool must never do.
const UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 '
  + '(KHTML, like Gecko) Version/17.0 Safari/605.1.15 xNAUT-Wiki';

const url = process.argv[2];
const out = resolve(ROOT, process.argv[3] || '.xnaut/wiki-capture.json');
if (!url || !/^https?:\/\//i.test(url)) {
  console.error('usage: node scripts/wiki-capture.mjs <http(s) url> [out.json]');
  process.exit(2);
}

const response = await fetch(url, { headers: { 'User-Agent': UA, Accept: 'text/html,application/xhtml+xml' } });
if (!response.ok) {
  console.error(`${url} answered ${response.status}`);
  process.exit(1);
}
const finalUrl = response.url || url;
const html = await response.text();

const browser = await chromium.launch();
const page = await browser.newPage();
await page.setContent('<!doctype html><html><head></head><body></body></html>');
await page.addScriptTag({ path: resolve(ROOT, 'src/js/vendor/readability.bundle.js') });
await page.addScriptTag({ path: resolve(ROOT, 'src/js/wiki-pane.js') });

const capture = await page.evaluate(({ raw, at }) => {
  const article = window.xnautWiki.extract(raw, at);
  if (!article) return null;
  const host = document.createElement('div');
  host.innerHTML = article.html;
  return {
    title: article.title,
    nav: article.nav,
    html: article.html,
    // storableMarkdown, not htmlToMarkdown: what the pane actually records is
    // the body without its title, because the collection file writes the title
    // as the entry's own heading.
    markdown: window.xnautWiki.storableMarkdown(host, article.title),
  };
}, { raw: html, at: finalUrl });
await browser.close();

if (!capture) {
  console.error(`${finalUrl} has no article the Wiki can extract; in the app this falls back to the browser pane.`);
  process.exit(1);
}

await mkdir(dirname(out), { recursive: true });
await writeFile(out, JSON.stringify({ url: finalUrl, ...capture }, null, 2));

console.log(`url      ${finalUrl}`);
console.log(`title    ${capture.title}`);
console.log(`nav      ${capture.nav.filter((n) => n.href).length} links, ${capture.nav.filter((n) => !n.href).length} groups`);
console.log(`article  ${capture.markdown.length} chars of markdown`);
console.log(`written  ${out}`);
