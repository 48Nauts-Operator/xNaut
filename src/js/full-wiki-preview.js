// XNAUT-455: presentation bootstrap for the separate full-app test bundle.
// The native compile-time flag, not this URL, disables background automation.
(function () {
  if (!new URLSearchParams(location.search).has('full-wiki-preview')) return;
  window.xnautFullWikiPreview = true;
  localStorage.setItem('xnaut-sidebar-visible', '1');
  localStorage.setItem('xnaut-right-pane-visible', '1');
  localStorage.setItem('xnaut-right-pane-width', '680');
  localStorage.setItem('xnaut-workspace-subtab', 'journal');
  document.addEventListener('DOMContentLoaded', () => {
    const timer = setInterval(async () => {
      if (!window.xnautStartupHealth?.sealed() || !window.__TAURI__?.core || !window.xnautRightPaneShow?.('workspace')) return;
      clearInterval(timer);
      try {
        const projects = await window.__TAURI__.core.invoke('project_wiki_projects');
        const p = projects.find(p => p.key === 'XNAUT') || projects[0];
        if (p) {
          window.xnautOpenWorkspace?.({ project: p.key });
          localStorage.setItem('xnaut-workspace-subtab:' + p.root, 'journal');
          window.xnautRightPaneSetRoot(p.root);
          window.xnautRightPaneShow('workspace');
          if (new URLSearchParams(location.search).has('journal-preview')) window.xnautOpenAgentSpace?.();
        }
      } catch (error) { console.error('Full Wiki preview project load:', error); }
    }, 300);
  });
})();
