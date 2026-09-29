// Decision table → evidence inspector inspired by 48Nauts/skill-dash,
// public/app.js (MIT, Copyright 2026 48Nauts). Reimplemented for Tauri and
// tool preloading: recommendations and actual calls are separate evidence.
(() => {
  'use strict';
  const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
  const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const percent = n => Number.isFinite(n) ? `${Math.round(n * 100)}%` : '—';
  const time = value => { const d = new Date(value); return Number.isNaN(+d) ? 'Unknown time' : d.toLocaleString(); };
  let styled = false;
  function styles() {
    if (styled) return; styled = true;
    const el = document.createElement('style'); el.textContent = `
.jd { min-width:0; color:var(--foreground,#eee); font-size:12px; }
.jd-top,.jd-controls,.jd-pager { display:flex;flex-wrap:wrap;align-items:center;gap:10px; }
.jd-top { justify-content:space-between;margin-bottom:12px; }.jd h3 {font-size:17px;margin:0 0 5px;}.jd p {line-height:1.6;margin:5px 0;color:var(--muted-foreground,#aaa);}
.jd-config,.jd-detail,.jd-table-wrap {border:1px solid var(--border,#333);border-radius:10px;background:var(--card,#17191d);padding:14px;margin:12px 0;}
.jd-config summary {cursor:pointer;}.jd-config label,.jd-controls label {display:flex;align-items:center;gap:6px;}.jd input,.jd select {color:inherit;background:var(--background,#111);border:1px solid var(--border,#444);border-radius:6px;padding:7px;font:inherit;}.jd input[type=number]{width:76px;}.jd input[type=search]{min-width:180px;max-width:100%;}.jd-config .jd-controls{margin-top:12px;}
.jd table {width:100%;border-collapse:collapse;font-size:12px;}.jd th {text-align:left;font-size:10px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted-foreground,#999);font-weight:500;padding:9px;}.jd td {padding:10px 9px;border-top:1px solid var(--border,#333);vertical-align:top;}.jd-table-wrap{padding:0;overflow-x:auto;}.jd-request {min-width:210px;max-width:460px;overflow-wrap:anywhere;}.jd small{display:block;color:var(--muted-foreground,#999);margin-top:5px;font-size:10px;}.jd-state {font-size:10px;display:inline-block;border:1px solid var(--border,#444);padding:3px 7px;border-radius:5px;}.jd-state.active{color:#5ccbae;}.jd-state.shadow{color:var(--accent,#e6bd42);}.jd-state.fallback{color:#efaa73;}
.jd-pager{justify-content:flex-end;}.jd-pager span{margin-right:auto;}.jd-error{color:#f18b7e!important;}.jd-inspector-head{display:flex;align-items:center;gap:10px;}.jd-inspector-head h3{margin-right:auto;}.jd-detail pre {white-space:pre-wrap;overflow-wrap:anywhere;max-height:340px;overflow:auto;padding:10px;background:var(--background,#111);border-radius:6px;font-size:11px;}.jd-metrics{display:flex;gap:24px;flex-wrap:wrap;margin:14px 0;}.jd-metrics b{display:block;font-size:17px;font-weight:500;margin-top:3px;}.jd-metrics span{font-size:10px;color:var(--muted-foreground,#aaa);}.jd-prob{width:135px;}.jd-bar{height:4px;background:var(--muted,#303334);border-radius:2px;margin-top:5px;}.jd-bar i{display:block;height:100%;background:var(--accent,#e6bd42);border-radius:2px;}.jd-selected{color:#5ccbae;}.jd-detail details{margin-top:12px;}.jd-empty{text-align:center;padding:28px!important;}.jd [hidden]{display:none!important;}
@media(max-width:800px){.jd-request{min-width:160px;}.jd th,.jd td{padding:8px 6px;}.jd-metrics{gap:12px;}.jd-config .jd-controls{align-items:flex-start;}}
`; document.head.appendChild(el);
  }
  function mount(parent) {
    styles();
    const el = document.createElement('div'); el.className = 'jd'; parent.appendChild(el);
    el.innerHTML = `<div class="jd-top"><div><h3>Decisions</h3><p>What Jev recommended, what xNaut loaded, and what the agent actually used.</p></div><button class="obs-btn" data-jd-refresh>Refresh decisions</button></div>
      <details class="jd-config" open><summary>Tool selection · <span data-jd-mode>Loading…</span></summary>
      <p>Independent tool relevance judgments run together through NautGate. Search and loading remain available for omitted tools. Probabilities indicate relevance, not permission or proof of correctness.</p>
      <form data-jd-config class="jd-controls"><label>Mode <select name="mode" aria-label="Jev tool selection mode"><option value="off">Off</option><option value="shadow">Shadow — observe</option><option value="active">Active — preload</option></select></label><label>Minimum relevance <input name="threshold" aria-label="Minimum tool relevance" type="number" min="0" max="1" step="0.05"></label><label>Tools <input name="maxTools" aria-label="Maximum preloaded tools" type="number" min="1" max="64"></label><label>Timeout (ms) <input name="timeoutMs" aria-label="Jev timeout in milliseconds" type="number" min="500" max="15000" step="100"></label><label>NautGate endpoint <input name="gatewayEndpoint" aria-label="Jev NautGate endpoint" type="url" placeholder="Use configured NautGate"></label><button class="obs-btn" type="submit">Save selection settings</button><button class="obs-btn" type="button" data-jd-test>Test Jev connection</button></form>
      <p data-jd-policy>Off makes no calls. Shadow records recommendations. Active preloads them. Enabling sends the current request, up to four recent prose messages and tool descriptions to hosted Jev through NautGate. Settings apply to the next turn. An optional endpoint uses the same NautGate client key; only enter a gateway you control.</p><p>Experimental policy; tune using measured results. Connection tests use a synthetic ticket request, incur API usage, and execute no tools.</p><p data-jd-config-status role="status"></p></details>
      <div class="jd-controls"><label>Search <input type="search" data-jd-search placeholder="Request or conversation…"></label><label>Status <select data-jd-status aria-label="Decision status"><option value="">All decisions</option><option value="active">Applied</option><option value="shadow">Shadow</option><option value="fallback">Fallback</option><option value="evaluating">Evaluating</option></select></label><label>Rows <select data-jd-size aria-label="Decisions rows per page"><option>5</option><option selected>10</option><option>25</option></select></label></div>
      <div class="jd-metrics" data-jd-summary aria-label="Filtered decision totals" hidden></div><p class="jd-error" data-jd-error role="alert" hidden></p><div class="jd-table-wrap"><table><thead><tr><th>Request</th><th>Selection</th><th>Tools</th><th>Jev time</th><th>Estimate</th><th></th></tr></thead><tbody data-jd-rows><tr><td class="jd-empty" colspan="6">Loading decisions…</td></tr></tbody></table></div>
      <div class="jd-pager"><span data-jd-count aria-live="polite"></span><button class="obs-btn" data-jd-prev aria-label="Previous Decisions page">Previous</button><button class="obs-btn" data-jd-next aria-label="Next Decisions page">Next</button></div><section class="jd-detail" data-jd-detail aria-label="Decision details" hidden></section>`;
    let page = 0, size = 10, config = null, sequence = 0, detailId = '', debounce;
    const q = selector => el.querySelector(selector);
    const configStatus = (text, error=false) => { q('[data-jd-config-status]').textContent = text; q('[data-jd-config-status]').classList.toggle('jd-error',error); };
    const price = r => !r.attempted ? 'No call' : r.model === 'jev-1.13.0' && Number.isFinite(r.usage?.input_tokens) ? `$${(r.usage.input_tokens * .042 / 1e6).toFixed(6)}` : 'Unknown';
    async function loadConfig() {
      try {
        const result = await invoke('jev_decisions_settings_get');
        if (!result?.config) throw new Error('Decision settings unavailable');
        config = result.config;
        for (const name of ['mode','threshold','maxTools','timeoutMs','gatewayEndpoint']) q(`[name="${name}"]`).value = config[name] ?? '';
        q('[data-jd-mode]').textContent = `${config.mode} · ${result.model || 'Jev'} · NautGate`;
      } catch (e) { configStatus(String(e),true); }
    }
    async function refresh() {
      const seq = ++sequence;
      try {
        const result = await invoke('jev_decisions_list',{page,size,search:q('[data-jd-search]').value,status:q('[data-jd-status]').value});
        if (seq !== sequence) return;
        if (!result || !Array.isArray(result.rows)) throw new Error('Decision history unavailable');
        page = result.page;
        const stats=q('[data-jd-summary]');stats.hidden=!result.summary;
        if(result.summary) stats.innerHTML=`<span>DECISIONS · FILTERED<b>${Number(result.total)}</b></span><span>APPLIED<b>${Number(result.summary.applied)}</b></span><span>FALLBACKS<b>${Number(result.summary.fallback)}</b></span><span>AVERAGE JEV TIME<b>${Math.round(Number(result.summary.averageLatencyMs))} ms</b></span>`;
        q('[data-jd-error]').hidden = true;
        q('[data-jd-rows]').innerHTML = result.rows.length ? result.rows.map(r => `<tr><td class="jd-request">${esc(r.request || 'No request text')}<small>${esc(time(r.at))} · ${esc(r.context)}</small></td><td><span class="jd-state ${['active','shadow','fallback'].includes(r.status)?r.status:''}">${esc(r.status==='active'?'Applied':r.status)}</span><small>${esc(r.outcome)}</small></td><td>${r.selected?.length || 0} recommended<small>${Number(r.beforeCount)} → ${Number(r.afterCount)} schemas · ${(r.calls||[]).filter(c=>!c.catalogOperation&&c.status==='completed').length} successful calls</small></td><td>${Number(r.latencyMs || 0)} ms</td><td>${esc(price(r))}</td><td><button class="obs-btn" data-jd-open="${esc(r.id)}">Inspect</button></td></tr>`).join('') : '<tr><td colspan="6" class="jd-empty">No matching decisions yet. Enable Shadow or Active, then send an agent chat request.</td></tr>';
        q('[data-jd-count]').textContent = result.total ? `${page*size+1}–${Math.min((page+1)*size,result.total)} of ${result.total}` : '0 decisions';
        q('[data-jd-prev]').disabled = page===0; q('[data-jd-next]').disabled = (page+1)*size>=result.total;
        el.querySelectorAll('[data-jd-open]').forEach(button=>{ button.onclick=()=>openDetail(button.dataset.jdOpen); });
      } catch (e) { if (seq===sequence) { q('[data-jd-error]').hidden=false; q('[data-jd-error]').textContent=`Could not refresh decision history: ${String(e)}. Any rows shown are from the last successful read.`; } }
    }
    async function openDetail(id) {
      detailId = id; const panel=q('[data-jd-detail]'); panel.hidden=false; panel.textContent='Loading decision…';
      try {
        const r=await invoke('jev_decision_get',{id}); if (detailId!==id) return;
        if (!r?.id) throw new Error('Decision not found');
        const used=new Map(); for (const call of r.calls||[]) { const values=used.get(call.name)||[]; values.push(call.status);used.set(call.name,values); }
        panel.innerHTML=`<div class="jd-inspector-head"><h3>Tool selection receipt</h3><button class="obs-btn" data-jd-copy>Copy receipt</button><button class="obs-btn" data-jd-close>Close details</button></div><p>${esc(r.request)}</p><p><b>${esc(r.reason)}</b></p><div class="jd-metrics"><span>MODE<b>${esc(r.mode)}</b></span><span>JEV TIME<b>${Number(r.latencyMs)} ms</b></span><span>SCHEMAS SENT<b>${Number(r.beforeCount)} → ${Number(r.afterCount)}</b></span><span>THIS REQUEST<b>${esc(price(r))}</b></span></div><p>${esc(r.model||'No model receipt')} · ${esc(r.questionVersion)} · threshold ${esc(r.policy?.threshold)} · top ${esc(r.policy?.maxTools)} · outcome ${esc(r.outcome)}</p><div class="jd-table-wrap"><table><thead><tr><th>Candidate</th><th>Relevance probability</th><th>Recommendation</th><th>Observed calls</th></tr></thead><tbody>${(r.candidates||[]).map(c=>`<tr><td class="jd-request">${esc(c.name)}<small>${esc(c.description)}</small></td><td class="jd-prob">${percent(c.probability)}<div class="jd-bar"><i style="width:${Math.max(0,Math.min(100,Number(c.probability)*100))}%"></i></div></td><td class="${c.selected?'jd-selected':''}">${c.selected?(r.status==='active'?'Preloaded':'Recommended only'):'Not selected'}</td><td>${esc(used.get(c.name)?.join(', ')||'Not called')}</td></tr>`).join('')||'<tr><td colspan="4">No valid candidate scores recorded.</td></tr>'}</tbody></table></div><details><summary>Actual tool activity (${(r.calls||[]).length})</summary><pre>${esc(JSON.stringify(r.calls||[],null,2))}</pre></details><details><summary>Questions, probabilities, policy and usage</summary><pre>${esc(JSON.stringify(r,null,2))}</pre></details><p data-jd-copy-status role="status"></p>`;
        panel.querySelector('[data-jd-close]').onclick=()=>{detailId='';panel.hidden=true;};
        panel.querySelector('[data-jd-copy]').onclick=async()=>{try {await navigator.clipboard.writeText(JSON.stringify(r,null,2));panel.querySelector('[data-jd-copy-status]').textContent='Receipt copied.';}catch(_){panel.querySelector('[data-jd-copy-status]').textContent='Copy failed. The receipt is available below.';}};
        panel.scrollIntoView({block:'nearest'});
      } catch(e) { if(detailId===id) panel.textContent=`Could not load decision: ${String(e)}`; }
    }
    q('[data-jd-config]').onsubmit=async event=>{
      event.preventDefault(); if(!config) return;
      const button=event.submitter; button.disabled=true;
      try {
        const next={...config,mode:q('[name=mode]').value,threshold:Number(q('[name=threshold]').value),maxTools:Number(q('[name=maxTools]').value),timeoutMs:Number(q('[name=timeoutMs]').value),gatewayEndpoint:q('[name=gatewayEndpoint]').value.trim()};
        config=await invoke('jev_decisions_settings_save',{config:next}); await loadConfig();configStatus('Saved. Applies to the next agent turn.');
      }catch(e){configStatus(`Settings were not saved: ${String(e)}`,true);}finally{button.disabled=false;}
    };
    q('[data-jd-test]').onclick=async()=>{
      const button=q('[data-jd-test]');button.disabled=true;configStatus('Testing with a synthetic request…');
      try { const record=await invoke('jev_decisions_probe');configStatus(record.reason,record.status==='fallback');await refresh();await openDetail(record.id);void window.xnautJevCost?.refresh(); }catch(e){configStatus(String(e),true);}finally{button.disabled=false;}
    };
    q('[data-jd-refresh]').onclick=refresh;
    q('[data-jd-prev]').onclick=()=>{page=Math.max(0,page-1);void refresh();};q('[data-jd-next]').onclick=()=>{page++;void refresh();};
    q('[data-jd-status]').onchange=()=>{page=0;void refresh();};q('[data-jd-size]').onchange=()=>{size=Number(q('[data-jd-size]').value);page=0;void refresh();};
    q('[data-jd-search]').oninput=()=>{clearTimeout(debounce);debounce=setTimeout(()=>{page=0;void refresh();},200);};
    void loadConfig(); void refresh();
    return {refresh};
  }
  window.xnautJevDecisions={mount};
})();
