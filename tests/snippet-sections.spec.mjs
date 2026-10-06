import { test, expect } from '@playwright/test';
const command = 'printf "%s\\n" \\\n  "safe fixture"\n# this is code, not a heading';
const content = ['Notes before the headings.', '# AWS Config', 'Use a development profile.', '## Get AWS config', '```sh', command, '```', '## View active file path', '```sh', 'printf "%s" ~/.aws/config', '```', '# Other checks', '```sh', 'pwd', '```'].join('\n');
const snippets = [{id:'aws',name:'AWS Checks',category:'Cloud',content},{id:'plain',name:'Plain commands',category:'Local',content:'pwd\necho safe'},{id:'unfenced',name:'Headed plain',content:'# Inspect\npwd\necho safe'}];
async function open(page) {
 await page.addInitScript(seed=>{
  if (!localStorage.getItem('xnaut-snippets')) localStorage.setItem('xnaut-snippets',JSON.stringify(seed));
  window.__copied=[];
  Object.defineProperty(navigator,'clipboard',{value:{writeText:async text=>window.__copied.push(text)},configurable:true});
 },snippets);
 await page.goto('/?stub=1');
 await page.getByRole('button',{name:'More actions',exact:true}).click();
 await page.getByRole('menuitem',{name:'Command Snippets',exact:true}).click();
 await expect(page.locator('#snippets-panel')).toBeVisible();
}
test('sections preserve nesting, fenced commands, and keyboard collapse without execution', async({page})=>{
 await open(page);await page.getByRole('button',{name:'AWS Checks',exact:true}).click();
 await expect(page.getByLabel('Compact view')).not.toBeChecked();
 await expect(page.locator('.snippet-prose').first()).toHaveText('Notes before the headings.');
 const section=page.locator('details.snippet-section').filter({has:page.getByRole('heading',{name:'AWS Config',exact:true})});
 await expect(section.locator(':scope > .snippet-section-content > details')).toHaveCount(2);
 await expect(page.getByRole('heading',{name:'this is code, not a heading',exact:true})).toHaveCount(0);
 await expect(page.locator('[data-snippet-id="aws"] .snippet-cmd').first().locator('code')).toHaveText(command);
 const summary=section.locator(':scope > summary');await summary.focus();await page.keyboard.press('Enter');await expect(section).not.toHaveAttribute('open','');
 expect(await page.evaluate(()=>window.__xnautInvokes.filter(i=>i.cmd==='write_to_terminal'))).toHaveLength(0);
 await page.keyboard.press('Space');await expect(section).toHaveAttribute('open','');
 await page.locator('[data-snippet-id="aws"] .copy-cmd').first().click();expect(await page.evaluate(()=>window.__copied.at(-1))).toBe(command);
 // A synthetic terminal only: verify exact IPC bytes, never execute a shell.
 await page.evaluate(()=>{eval("tabs=[{id:'fixture',focusedPaneIndex:0,terminals:[{sessionId:'fixture-terminal'}]}]; activeTabId='fixture';");});
 await page.locator('[data-snippet-id="aws"] .run-cmd').first().click();
 expect(await page.evaluate(()=>window.__xnautInvokes.findLast(i=>i.cmd==='write_to_terminal').args)).toEqual({sessionId:'fixture-terminal',data:command+'\n'});
 if(process.env.XNAUT_SNIPPET_SCREENSHOT) await page.screenshot({path:process.env.XNAUT_SNIPPET_SCREENSHOT});
});
test('compact view preserves multiline blocks, search, source, edit and saved preference',async({page})=>{
 await open(page);await page.locator('#snippet-search').fill('AWS');await page.getByRole('button',{name:'AWS Checks',exact:true}).click();
 await page.getByLabel('Compact view').check();await expect(page.locator('.snippet-section')).toHaveCount(0);await expect(page.locator('.snippet-cmd')).toHaveCount(3);
 await expect(page.locator('.snippet-cmd').first().locator('code')).toHaveText(command);
 await page.getByLabel('Compact view').uncheck();await expect(page.getByRole('heading',{name:'AWS Config',exact:true})).toBeVisible();
 await page.locator('.edit-snippet').click();await expect(page.locator('#snippet-content')).toHaveValue(content);await page.locator('#btn-cancel-snippet').click();
 expect(await page.evaluate(()=>JSON.parse(localStorage.getItem('xnaut-snippets'))[0].content)).toBe(content);
 await page.getByLabel('Compact view').check();await page.reload();await page.getByRole('button',{name:'More actions',exact:true}).click();await page.getByRole('menuitem',{name:'Command Snippets',exact:true}).click();await expect(page.getByLabel('Compact view')).toBeChecked();
});
test('plain and unfenced headed snippets remain usable, and HTML is inert',async({page})=>{
 await open(page);await page.getByRole('button',{name:'Plain commands',exact:true}).click();await expect(page.locator('[data-snippet-id="plain"] .snippet-cmd')).toHaveCount(2);
 await page.getByRole('button',{name:'Plain commands',exact:true}).click();await page.getByRole('button',{name:'Headed plain',exact:true}).click();await expect(page.getByRole('heading',{name:'Inspect',exact:true})).toBeVisible();await page.getByLabel('Compact view').check();await expect(page.locator('[data-snippet-id="unfenced"] .snippet-cmd')).toHaveCount(2);
 const parsed=await page.evaluate(()=>window.xnautSnippetSections.parse('Before\nTitle\n=====\n~~~sh\n# still code\n<img src=x onerror=alert(1)>\n~~~'));
 expect(parsed.blocks[1].title).toBe('Title');expect(parsed.commands).toEqual(['# still code\n<img src=x onerror=alert(1)>']);
});

test('Run never sends terminal IPC for a nonterminal panel or an empty session', async ({page}) => {
 await open(page);await page.getByRole('button',{name:'AWS Checks',exact:true}).click();
 for (const sessionId of [null, '', '   ']) {
  await page.evaluate(sessionId => {
   eval("tabs=[{id:'no-terminal',isPanel:true,focusedPaneIndex:0,terminals:[{}]}]; activeTabId='no-terminal';");
   if (sessionId !== null) eval('tabs')[0].terminals[0].sessionId=sessionId;
   window.__xnautInvokes.length=0;
  }, sessionId);
  await page.locator('[data-snippet-id="aws"] .run-cmd').first().click();
  expect(await page.evaluate(()=>window.__xnautInvokes.filter(i=>i.cmd==='write_to_terminal'))).toEqual([]);
 }
});
