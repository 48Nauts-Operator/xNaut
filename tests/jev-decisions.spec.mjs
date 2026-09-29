import {test,expect} from '@playwright/test';
async function setup(page){
 await page.goto('/?stub=1');await page.waitForSelector('#btn-help');
 await page.evaluate(()=>{
  const rows=Array.from({length:12},(_,i)=>({id:`d${i}`,at:'2026-09-29T10:00:00Z',request:i===0?'<img src=x onerror=alert(1)> Audit JobUp':`Read ticket ${i}`,context:'cortana',status:'active',mode:'active',outcome:'answered',beforeCount:128,afterCount:4,selected:['read_fixture'],attempted:true,model:'jev-1.13.0',usage:{input_tokens:1000},latencyMs:240,candidates:[{name:'read_fixture',description:'Read a fixture',probability:.91,selected:true}],calls:[],reason:'Selected schemas preloaded; all permitted tools remain discoverable',policy:{threshold:.65,maxTools:16}}));
  const orig=window.__TAURI__.core.invoke;let config={mode:'shadow',threshold:.65,maxTools:16,timeoutMs:5000,gatewayEndpoint:'',future:'preserved'};
  window.__TAURI__.core.invoke=async(cmd,args)=>{
   if(cmd==='jev_decisions_settings_get')return {config,model:'jev-1.13.0'};
   if(cmd==='jev_decisions_settings_save'){config=args.config;window.savedDecisionConfig=config;return config;}
   if(cmd==='jev_decisions_list'){
    if(window.failDecisions)throw new Error('Storage offline');
    const filtered=rows.filter(r=>(!args.search||r.request.includes(args.search))&&(!args.status||r.status===args.status));
    return {rows:filtered.slice(args.page*args.size,(args.page+1)*args.size),total:filtered.length,page:args.page};
   }
   if(cmd==='jev_decision_get')return rows.find(r=>r.id===args.id);
   return orig(cmd,args);
  };
  window.xnautAttachObservatoryTab();
 });
 await page.getByRole('tab',{name:'Decisions',exact:true}).click();
}
test('decision table paginates; inspector separates recommendations from execution and escapes text',async({page})=>{
 await setup(page);await expect(page.locator('[data-jd-rows] tr')).toHaveCount(10);
 await expect(page.locator('[data-jd-rows] img')).toHaveCount(0);
 await page.locator('[data-jd-open]').first().click();
 await expect(page.getByRole('region',{name:'Decision details'})).toContainText('91%');
 await expect(page.getByRole('region',{name:'Decision details'})).toContainText('Not called');
 await expect(page.getByRole('region',{name:'Decision details'})).toContainText('$0.000042');
 await page.getByRole('button',{name:'Next Decisions page'}).click();await expect(page.locator('[data-jd-rows] tr')).toHaveCount(2);
 await page.getByRole('combobox',{name:'Decisions rows per page'}).selectOption('5');await expect(page.locator('[data-jd-rows] tr')).toHaveCount(5);
 await page.getByRole('searchbox').fill('JobUp');await expect(page.locator('[data-jd-count]')).toHaveText('1–1 of 1');
 await page.evaluate(()=>window.failDecisions=true);await page.getByRole('button',{name:'Refresh decisions'}).click();await expect(page.locator('[data-jd-error]')).toContainText('last successful read');
});
test('selection settings persist explicit mode and preserve future fields without running inference',async({page})=>{
 await setup(page);await page.getByRole('combobox',{name:'Jev tool selection mode'}).selectOption('active');
 await page.getByRole('button',{name:'Save selection settings'}).click();
 await expect(page.locator('[data-jd-mode]')).toContainText('active');
 expect(await page.evaluate(()=>window.savedDecisionConfig)).toMatchObject({mode:'active',future:'preserved'});
 expect(await page.evaluate(()=>window.__xnautInvokes.some(c=>c.cmd==='jev_decisions_probe'))).toBe(false);
});
