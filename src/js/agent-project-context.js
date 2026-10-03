// XNAUT-455: conversation scope comes from owner references, never assistant
// prose or the last terminal's cwd. Matching is local and deterministic.
(function () {
  'use strict';
  const words = value => String(value || '').replace(/([a-z]{2,}|\d)([A-Z])/g, '$1 $2')
    .toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim().replace(/\s+/g, ' ');
  const contains = (text, name) => name.length >= 3 && (` ${text} `).includes(` ${name} `);
  // These also occur as ordinary instructions/UI nouns. Require an explicit
  // project qualifier instead of treating 'keep the fix' as the Keep app.
  const commonNames = new Set(['keep', 'legacy', 'guardian', 'cockpit']);
  function mentions(text, projects) {
    const normalized = words(text);
    return projects.filter(project => {
      const names = [project.key, project.name, project.root?.split('/').filter(Boolean).pop()].filter(Boolean);
      return names.some(name => {
        const full = words(name);
        if (commonNames.has(full)) return ['app', 'project', 'repository', 'repo'].some(qualifier =>
          contains(normalized, `${full} ${qualifier}`) || contains(normalized, `${qualifier} ${full}`))
          || new RegExp(`(^|[^a-z0-9])${full}-[0-9]+`, 'i').test(String(text));
        if (contains(normalized, full)) return true;
        // ChessTrainer -> "Chess App" / "Chess project". Only qualified
        // aliases: a bare common noun must not redirect the conversation.
        const base = full.replace(/ (trainer|app|application|platform|system)$/, '');
        return base !== full && ['app', 'application', 'project'].some(suffix => contains(normalized, `${base} ${suffix}`));
      });
    });
  }
  function resolve({text = '', thread = {}, profile = {}, projects = [], fallback = ''}) {
    const find = value => projects.find(p => value && [p.key, p.root, p.name].includes(value));
    if (thread.journalPinnedProject) {
      return {project:find(thread.journalPinnedProject) || null, reason:'pinned'};
    }
    const messages = [...(thread.messages || [])].reverse().filter(m => m.role === 'user' && !m.voiceTranscript);
    for (const content of [text, ...messages.map(m => m.text || '')]) {
      const matches = mentions(content, projects);
      if (matches.length > 1) return {project:null, reason:'multiple'};
      if (matches.length === 1) return {project:matches[0], reason:'conversation'};
    }
    const saved = messages.find(m => Object.hasOwn(m, 'journalProject'));
    // An explicitly unresolved turn stays unresolved on "continue".
    const project = saved ? find(saved.journalProject) : find(thread.workspace) || find(profile.default_project) || (!messages.length && find(fallback));
    return {project:project || null, reason:project ? 'context' : 'unknown'};
  }
  function follow(project) {
    if (!project) return;
    const current = document.querySelector('.rpws-nav button.active')?.dataset.sub;
    const tab = current === 'wiki' ? 'wiki' : 'journal';
    const root = project.root || project.key;
    localStorage.setItem('xnaut-workspace-subtab:' + root, tab);
    window.xnautRightPaneSetRoot?.(root);
    window.xnautRightPaneShow?.('workspace');
  }
  window.xnautAgentProjectContext = {resolve, mentions, follow};
})();
