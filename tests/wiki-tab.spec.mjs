// The Wiki tab (XNAUT-438): "only the docu, not the entire website".
//
// Everything here runs against tests/fixtures/adk-docs-page.html, a saved docs
// page carrying a header, a search box, a cookie banner, a nested sidebar, an
// article and a marketing footer. The assertions that matter are the negative
// ones: an extractor that simply dumped the page would satisfy "the article is
// on screen" and fail every one of them.
//
// The backend is faked by wrapping invoke rather than by the usual
// `__xnautStub[cmd] = value`, because two of these commands are stateful:
// recording a page must change what the history dropdown then shows, and a
// fetch has to be able to fail for one URL and succeed for another. The fake
// mirrors wiki.rs: one entry per URL, pinned first, then newest first.
import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const PAGE_URL = 'https://adk.dev/docs/agents/llm-agents';
const OTHER_URL = 'https://adk.dev/docs/tools/function-tools';

const DOCS_HTML = await readFile(new URL('./fixtures/adk-docs-page.html', import.meta.url), 'utf8');
const SHELL_HTML = await readFile(new URL('./fixtures/adk-docs-shell.html', import.meta.url), 'utf8');

/**
 * Mount the Wiki pane on its own, with a fake wiki backend installed.
 * Returns the pane locator.
 */
async function mountWiki(page, { html = DOCS_HTML, url = PAGE_URL } = {}) {
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => window.xnautWiki && window.XnautReadability);

  await page.evaluate(({ html: body, url: at }) => {
    // The fake store. `pages` answers wiki_fetch per URL; `down` makes every
    // fetch fail, which is how the offline path is reached.
    const state = { pages: { [at]: body }, down: false, entries: [], browser: [] };
    window.__wiki = state;

    window.xnautAttachBrowserTab = (target) => { state.browser.push(target); return Promise.resolve(); };

    const slugOf = (u) => {
      const host = new URL(u).hostname.replace(/^www\./, '');
      const labels = host.split('.');
      return (labels.length > 1 ? labels[labels.length - 2] : labels[0]).toLowerCase();
    };
    // Keyed by slug, like the real one: a collection for a site nobody has
    // opened is empty, and a fake that ignored the slug would make
    // "@nothing-docu says it is empty" pass by accident.
    const view = (slug) => ({
      name: `@${slug}-docu`,
      reference: `work:smoke/Development/docu/${slug}-docu.md`,
      path: `/tmp/vault/work/smoke/Development/docu/${slug}-docu.md`,
      entries: state.entries.filter((e) => slugOf(e.url) === slug)
        .sort((a, b) => (b.pinned - a.pinned)
          || String(b.opened_at).localeCompare(String(a.opened_at))),
    });

    const real = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (cmd, args) => {
      if (String(cmd).indexOf('wiki_') !== 0) return real(cmd, args);
      window.__xnautInvokes.push({ cmd, args });
      if (cmd === 'wiki_slug') return Promise.resolve(slugOf(args.url));
      if (cmd === 'wiki_fetch') {
        if (state.down) return Promise.reject('could not reach ' + args.url + ': offline');
        const found = state.pages[args.url];
        if (!found) return Promise.reject('could not reach ' + args.url + ': no such page');
        return Promise.resolve({ url: args.url, status: 200, content_type: 'text/html', html: found });
      }
      if (cmd === 'wiki_collection_read') return Promise.resolve(view(args.slug));
      if (cmd === 'wiki_collection_record') {
        const was = state.entries.find((e) => e.url === args.url);
        state.entries = state.entries.filter((e) => e.url !== args.url);
        state.entries.push({
          url: args.url, title: args.title, markdown: args.markdown,
          pinned: was ? was.pinned : false,
          opened_at: new Date(Date.now() + state.entries.length * 1000).toISOString(),
        });
        return Promise.resolve(view(args.slug));
      }
      if (cmd === 'wiki_collection_pin') {
        state.entries.forEach((e) => { if (e.url === args.url) e.pinned = args.pinned; });
        return Promise.resolve(view(args.slug));
      }
      if (cmd === 'wiki_collection_forget') {
        state.entries = state.entries.filter((e) => e.url !== args.url);
        return Promise.resolve(view(args.slug));
      }
      return Promise.reject('unknown wiki command ' + cmd);
    };

    document.querySelectorAll('#wiki-test-host').forEach((n) => n.remove());
    const host = document.createElement('div');
    host.id = 'wiki-test-host';
    host.style.cssText = 'position:fixed;inset:0;z-index:9999;background:#111';
    document.body.appendChild(host);
    const wiki = window.xnautWiki.create();
    host.innerHTML = wiki.page();
    window.__wikiView = wiki;
    return wiki.attach(host, '/tmp/smoke');
  }, { html, url });

  return page.locator('#wiki-test-host .rpwk');
}

async function open(page, url) {
  await page.fill('#wiki-test-host [data-wiki-url]', url);
  await page.click('#wiki-test-host [data-wiki-go]');
}

// ─────────────────────────────────────────────────────────────────────────────

test('the Build run pane carries a Wiki tab between Output and History', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.locator('[data-rpane-view="workspace"]').click();
  await page.evaluate(() => window.xnautRightPaneSetRoot('/tmp/smoke'));

  const nav = page.locator('.rpws-nav');
  await expect(nav).toBeVisible();
  // The order is the ask, not merely the presence: "a sub-menu before History
  // called Wiki".
  await expect(nav.locator('button')).toHaveText(['Plan', 'Looms', 'Output', 'Wiki', 'History']);

  // And it is a tab that renders its own surface, not a nav entry wired to
  // nothing, which is the failure this whole file exists to catch.
  await nav.locator('button[data-sub="wiki"]').click();
  await expect(page.locator('.rpws-page[data-page="wiki"] .rpwk-url')).toBeVisible();
  await expect(page.locator('.rpws-page[data-page="wiki"]')).toContainText('The docs, and nothing else');
});

test('a pasted docs link renders the article, and only the article', async ({ page }) => {
  const pane = await mountWiki(page);
  await open(page, PAGE_URL);

  const article = pane.locator('.rpwk-article');
  await expect(article).toContainText('An LLM agent is a loop around a model');

  // Exactly one h1, carrying the page's own heading. Readability deletes the
  // heading or demotes it under a title taken from <title> ("LLM agents, ADK"
  // here), and both leave the reader without the name of the thing.
  await expect(article.locator('h1')).toHaveCount(1);
  await expect(article.locator('h1')).toHaveText('LLM agents');
  await expect(article.locator('h2').first()).toHaveText('Defining one');
  // No permalink pilcrows anywhere: they are a hover affordance on the site.
  expect(await article.innerText(), 'a permalink pilcrow survived').not.toContain('¶');
  await expect(article.locator('a.headerlink')).toHaveCount(0);

  // The code block survives as a code block, with the language the page
  // declared. Docs are mostly code, and a flattened <pre> is not reading them.
  const code = article.locator('pre code.language-python');
  await expect(code).toBeVisible();
  await expect(code).toContainText('agent = Agent(model="gemini-2.0-flash"');

  // The table and the nested list survive too.
  await expect(article.locator('table td code').first()).toHaveText('max_steps');
  await expect(article.locator('ul ul li').first()).toContainText('validated against the tool signature');

  // "not the entire website": every piece of chrome the fixture carries.
  const body = (await article.innerText()).toLowerCase();
  for (const gone of ['pricing', 'accept all', 'was this page helpful', 'terms', 'privacy',
    'follow us on x', 'free trial']) {
    expect(body, `the ${gone} chrome survived into the article`).not.toContain(gone);
  }
  await expect(article.locator('input, form, button'), 'the search box survived').toHaveCount(0);
});

test("the left column is the docs site's own navigation, nested, with this page marked", async ({ page }) => {
  const pane = await mountWiki(page);
  await open(page, PAGE_URL);

  const nav = pane.locator('.rpwk-nav');
  await expect(nav.locator('a')).toHaveCount(9);
  await expect(nav.locator('a', { hasText: 'Function tools' })).toHaveCount(1);

  // The site's groups are kept, so the column reads as the tree the site
  // publishes rather than a flat pile of leaves. "Tools" is NOT among them:
  // the fixture names that section twice, as a link to its own page and as
  // a heading over its children, and the heading is the copy with nothing in
  // it. adk.dev does this for every section, which is how it was found.
  await expect(nav.locator('.rpwk-group')).toHaveText(['Agents']);
  await expect(nav.locator('a', { hasText: 'Tools' }).first()).toHaveAttribute(
    'data-wiki-navlink', 'https://adk.dev/docs/tools');

  // Nesting: a child sits further in than its parent.
  const indent = (name) => nav.locator(`a`, { hasText: name }).first()
    .evaluate((el) => parseFloat(getComputedStyle(el).paddingLeft));
  expect(await indent('Lifecycle')).toBeGreaterThan(await indent('Custom agents'));
  expect(await indent('Custom agents')).toBeGreaterThan(await indent('Deploy'));

  // The page being read is the marked one, and only it.
  await expect(nav.locator('a.current')).toHaveCount(1);
  await expect(nav.locator('a.current')).toHaveText('LLM agents');

  // Nothing off the docs root got in. The header's Pricing/Blog/GitHub links
  // are navigation too, and they are not these docs.
  const hrefs = await nav.locator('a').evaluateAll((els) => els.map((e) => e.getAttribute('data-wiki-navlink')));
  expect(hrefs.every((h) => h.startsWith('https://adk.dev/docs/')), `off-root links: ${hrefs}`).toBe(true);
});

test('a link inside the docs navigates in the tab; another host opens the browser pane', async ({ page }) => {
  const pane = await mountWiki(page);
  await open(page, PAGE_URL);
  await page.evaluate((u) => { window.__wiki.pages[u] = window.__wiki.pages[Object.keys(window.__wiki.pages)[0]]; }, OTHER_URL);

  // A sidebar link.
  await pane.locator('.rpwk-nav a', { hasText: 'Function tools' }).click();
  await expect.poll(() => page.evaluate((u) => window.__xnautInvokes
    .some((i) => i.cmd === 'wiki_fetch' && i.args.url === u), OTHER_URL)).toBe(true);

  // A link in the prose, to another page of the same docs.
  await open(page, PAGE_URL);
  await pane.locator('.rpwk-article a', { hasText: 'workflow agents' }).click();
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes
    .some((i) => i.cmd === 'wiki_fetch' && i.args.url === 'https://adk.dev/docs/agents/workflow-agents'))).toBe(true);

  // A link off the docs site goes to the browser pane, and is NOT fetched here.
  await open(page, PAGE_URL);
  await pane.locator('.rpwk-article a', { hasText: 'agent protocol specification' }).click();
  await expect.poll(() => page.evaluate(() => window.__wiki.browser))
    .toContain('https://example.org/spec/agent-protocol');
  const fetched = await page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'wiki_fetch').map((i) => i.args.url));
  expect(fetched, 'an off-site link was fetched into the Wiki tab').not.toContain('https://example.org/spec/agent-protocol');
});

test('a page with no article falls back to the browser pane and says so', async ({ page }) => {
  const pane = await mountWiki(page);
  await page.evaluate((html) => { window.__wiki.pages['https://adk.dev/docs/app'] = html; }, SHELL_HTML);
  await open(page, 'https://adk.dev/docs/app');

  await expect(pane.locator('.rpwk-note')).toContainText('no article the Wiki can extract');
  await expect(pane.locator('.rpwk-note')).toContainText('browser pane');
  await expect.poll(() => page.evaluate(() => window.__wiki.browser)).toContain('https://adk.dev/docs/app');
  // And it did not half-render: nothing of the shell leaked into the article.
  await expect(pane.locator('.rpwk-article')).not.toContainText('requires JavaScript');
});

test('every page opened is recorded in the project collection, and reopens from the dropdown', async ({ page }) => {
  const pane = await mountWiki(page);
  await open(page, PAGE_URL);

  // Recorded with the article as markdown, which is what makes an offline
  // reopen possible, so an empty string here is the feature not working.
  const recorded = await expect.poll(() => page.evaluate(() => window.__wiki.entries)).not.toHaveLength(0)
    .then(() => page.evaluate(() => window.__wiki.entries));
  expect(recorded).toHaveLength(1);
  expect(recorded[0].url).toBe(PAGE_URL);
  expect(recorded[0].title).toBe('LLM agents');
  expect(recorded[0].markdown, 'the code block was not stored as a fenced block')
    .toContain('```python');
  // Markdown, not stripped text: the emphasis the page carried survived.
  expect(recorded[0].markdown).toContain('An **LLM agent** is a loop around a model');
  // The body carries no title of its own: the collection file gives every page
  // a `## Title` fence, and storing the heading too printed it twice in a row
  // in the vault. The reader puts it back; asserted in the offline test.
  expect(recorded[0].markdown.startsWith('# LLM agents'),
    'the title was stored on top of the body as well as in the fence').toBe(false);

  // The collection is named on screen, with the vault path it is written to.
  await expect(pane.locator('.rpwk-scope')).toContainText('@adk-docu');
  await expect(pane.locator('.rpwk-scope')).toContainText('work:smoke/Development/docu/adk-docu.md');

  // The dropdown is that collection, and one click reopens a page.
  await pane.locator('[data-wiki-hist]').click();
  const menu = pane.locator('.rpwk-menu');
  await expect(menu.locator('.rpwk-row')).toHaveCount(1);
  await expect(menu.locator('.rpwk-open b')).toHaveText('LLM agents');

  // Pinning is remembered on the entry, and a pinned page sorts first.
  await page.evaluate((u) => { window.__wiki.pages[u] = window.__wiki.pages[Object.keys(window.__wiki.pages)[0]]; }, OTHER_URL);
  await menu.locator('.rpwk-pin').click();
  await expect.poll(() => page.evaluate(() => window.__wiki.entries[0].pinned)).toBe(true);
  await open(page, OTHER_URL);
  await pane.locator('[data-wiki-hist]').click();
  await expect(menu.locator('.rpwk-row')).toHaveCount(2);
  await expect(menu.locator('.rpwk-open b').first()).toHaveText('LLM agents');
});

test('when the site is down the stored copy renders, and says that is what it is', async ({ page }) => {
  const pane = await mountWiki(page);
  await open(page, PAGE_URL);
  await expect(pane.locator('.rpwk-article h1')).toHaveText('LLM agents');

  await page.evaluate(() => { window.__wiki.down = true; });
  await open(page, PAGE_URL);

  await expect(pane.locator('.rpwk-note')).toContainText('offline');
  await expect(pane.locator('.rpwk-note')).toContainText('@adk-docu');
  // Rendered from the stored article, not left blank, and still headed by the
  // page's name, which the stored body does not carry.
  await expect(pane.locator('.rpwk-article h1')).toHaveText('LLM agents');
  await expect(pane.locator('.rpwk-article')).toContainText('An LLM agent is a loop around a model');
  await expect(pane.locator('.rpwk-article pre code')).toContainText('gemini-2.0-flash');

  // A URL that was never opened has nothing stored, so it says so instead of
  // showing the last page as if it were the one asked for.
  await open(page, 'https://adk.dev/docs/never-seen');
  await expect(pane.locator('.rpwk-note')).toContainText('Could not open');
});

test('@adk-docu in the bar reopens the collection where it left off', async ({ page }) => {
  const pane = await mountWiki(page);
  await open(page, PAGE_URL);
  await page.evaluate((u) => { window.__wiki.pages[u] = window.__wiki.pages[Object.keys(window.__wiki.pages)[0]]; }, OTHER_URL);
  await open(page, OTHER_URL);

  await page.evaluate(() => { window.__xnautInvokes.length = 0; });
  await open(page, '@adk-docu');

  // The LAST page of that collection, fetched fresh.
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'wiki_fetch').map((i) => i.args.url))).toEqual([OTHER_URL]);
  await expect(pane.locator('[data-wiki-url]')).toHaveValue(OTHER_URL);

  // A collection with nothing in it says so rather than failing silently.
  await open(page, '@nothing-docu');
  await expect(pane.locator('.rpwk-note')).toContainText('Nothing in');
});

test('the open page is remembered per project', async ({ page }) => {
  await mountWiki(page);
  await open(page, PAGE_URL);
  await expect.poll(() => page.evaluate(() => localStorage.getItem('xnaut-wiki-url:/tmp/smoke'))).toBe(PAGE_URL);

  // A second project does not inherit it.
  const other = await page.evaluate(() => localStorage.getItem('xnaut-wiki-url:/tmp/other'));
  expect(other).toBeNull();

  // Remounting the same project comes back to the page that was open.
  await page.evaluate(() => {
    const host = document.getElementById('wiki-test-host');
    const wiki = window.xnautWiki.create();
    host.innerHTML = wiki.page();
    return wiki.attach(host, '/tmp/smoke');
  });
  await expect(page.locator('#wiki-test-host .rpwk-article h1')).toHaveText('LLM agents');
});

test('the Wiki tab raises no errors while doing all of that', async ({ page }) => {
  const pane = await mountWiki(page);
  await open(page, PAGE_URL);
  await pane.locator('[data-wiki-hist]').click();
  await pane.locator('.rpwk-nav a', { hasText: 'Deploy' }).click();
  await page.waitForTimeout(300);
  const errors = await page.evaluate(() => window.__xnautErrors || []);
  expect(errors, `the Wiki tab raised: ${errors.join(', ')}`).toEqual([]);
});
