import {test, expect} from '@playwright/test';

test.beforeEach(async ({page}) => {
  await page.addInitScript(()=>{localStorage.setItem('xnaut-right-pane-visible','1');localStorage.setItem('xnaut-sidebar-visible','1');});
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
});

test('new project requires a repository and offers Forgejo and GitHub', async ({page}) => {
  await page.evaluate(() => window.xnautAttachChatTab({chatKey:'repository-setup',title:'Setup'}));
  await page.locator('#xnaut-right-pane-host').getByRole('button',{name:'New project',exact:true}).click();
  const pane = page.locator('.rpnp');
  await expect(pane.getByRole('button',{name:'No repo',exact:true})).toHaveCount(0);
  await expect(pane.getByRole('button',{name:'Forgejo',exact:true})).toBeVisible();
  await expect(pane.getByRole('button',{name:'GitHub',exact:true})).toBeVisible();
  await pane.locator('.rpnp-name').fill('Example');
  await pane.locator('.rpnp-path').fill('/tmp/example');
  await expect(pane.getByRole('button',{name:'Create project',exact:true})).toBeDisabled();
  await pane.locator('.rpnp-url').fill('git@github.com:owner/example.git');
  await expect(pane.getByRole('button',{name:'Create project',exact:true})).toBeEnabled();
});

test('settings keep the local folder separate from the required repository and show pending delivery', async ({page}) => {
  await page.evaluate(() => {
    const project={key:'SMOKE',name:'Smoke Test',purpose:'Repository delivery',source_path:'/tmp/smoke',source_repo:'/tmp/smoke',forge_remote:'ssh://git@forge.example:2222/team/app.git',revision:1,flow_type:'standard'};
    window.__xnautStub.pm_module_status={enabled:true,configured:true,valid:true,repo_path:'/tmp/control',git_repository:true};
    window.__xnautStub.project_mcp_info={url:'http://127.0.0.1:5000',token:'fixture',read_token:'fixture-read'};
    window.__xnautStub.pm_project_update={...project,forge_remote:'https://github.com/team/app.git',revision:2};
    window.__xnautStub.pm_project_import_existing=[project]; window.__xnautStub.pm_project_list=[project];
    window.__xnautStub.repository_transfer_list=[{run_id:'run-a',ticket:'SMOKE-1',state:'pushed',error:'PR connection unavailable',pr_url:null}];
    const host=document.createElement('div');host.id='transfer-settings';document.body.append(host);
    window.xnautCreateProjectManagementPanel('transfer-test',host,{project:'SMOKE',section:'settings'});
  });
  const pane=page.locator('#transfer-settings');
  await expect(pane.locator('.pmw-settings-source')).toHaveValue('/tmp/smoke');
  await expect(pane.locator('.pmw-settings-remote')).toHaveValue('ssh://git@forge.example:2222/team/app.git');
  await expect(pane.locator('.pmw-transfers')).toContainText('SMOKE-1: pushed — PR connection unavailable');
  await pane.locator('.pmw-settings-remote').fill('https://github.com/team/app.git');
  await pane.getByRole('button',{name:'Save settings',exact:true}).click();
  await expect.poll(()=>page.evaluate(()=>window.__xnautInvokes.filter(x=>x.cmd==='pm_project_update').at(-1)?.args?.request?.forge_remote)).toBe('https://github.com/team/app.git');
  await expect(pane.locator('.pmw-stage-badge')).toContainText('Revision 2');
  await pane.locator('.pmw-settings-remote').scrollIntoViewIfNeeded();
  await page.screenshot({path:'test-results/repository-settings.png'});
});

test('project notes queue a copy for Git while local saves survive queue errors', async ({page}) => {
  await page.evaluate(async()=>{
    const invoke=window.__TAURI__.core.invoke;
    window.__noteUploads=[];
    window.__TAURI__.core.invoke=(cmd,args)=>{
      if(cmd==='repository_notebook_queue') {window.__noteUploads.push(args);return Promise.reject('Repository needs setup');}
      return invoke(cmd,args);
    };
    const host=document.createElement('div');host.id='repository-notes';host.style.cssText='position:fixed;inset:40px;z-index:9999;background:#181818';document.body.append(host);
    await window.xnautNotebook.mount(host,'/tmp/smoke');
  });
  const pane=page.locator('#repository-notes');
  await pane.getByRole('button',{name:'+ Add note',exact:true}).click();
  await pane.getByLabel('Note title').fill('Review reminders');
  await pane.getByLabel('Note text').fill('- [ ] Read the audit\nKeep the recording');
  await pane.getByRole('button',{name:'Done',exact:true}).click();
  await expect(pane.getByRole('status')).toContainText('Saved on this device · repository sync pending: Repository needs setup');
  await expect.poll(()=>page.evaluate(()=>window.__noteUploads.at(-1)?.data.notes[0].body)).toContain('Keep the recording');
  expect(await page.evaluate(()=>JSON.parse(window.xnautConversationStorage.getItem(window.__noteUploads.at(-1).key)).notes[0].title)).toBe('Review reminders');
});
