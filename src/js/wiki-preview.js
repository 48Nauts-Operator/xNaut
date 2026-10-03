(async () => {
  const host = document.getElementById('wiki-preview'), select = document.getElementById('project-select');
  try {
    const projects = await window.__TAURI__.core.invoke('project_wiki_projects');
    for (const p of projects) select.add(new Option(p.name + ' · ' + p.key, p.key));
    const remembered = localStorage.getItem('wiki-preview-project');
    select.value = projects.some(p => p.key === remembered) ? remembered : projects.some(p => p.key === 'XNAUT') ? 'XNAUT' : projects[0]?.key || '';
    select.onchange = () => { localStorage.setItem('wiki-preview-project', select.value); window.xnautProjectWiki.mount(host, select.value); };
    select.onchange();
  } catch (error) { host.textContent = String(error); host.classList.add('preview-error'); }
})();
