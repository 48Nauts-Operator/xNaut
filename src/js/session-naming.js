// One rule for `<agent>-<project>` session names (XNAUT-86).
//
// Three surfaces ask the same question about the same strings and, until this
// file existed, answered it three different ways:
//
//   sidebar.js::sessionsFor        does this session belong to this project?
//   observatory-panel.js::attribute  …source 3, the same question
//   observatory-panel.js::openNewSession  what shall I call a new one?
//
// The rule below is the Observatory's, lifted whole: `slug` drops separators
// rather than collapsing them, so `DAT.AG`, `dat-ag` and `datag` are one name.
// The sidebar's copy compared raw strings and was CASE SENSITIVE, which is a
// bug rather than a difference of opinion: every session xNAUT creates goes
// through `zellij::session_name`, which lowercases, so a session opened for
// `Bucky` is really called `cl-bucky` and the sidebar's `'Bucky'.startsWith('bucky')`
// answered false. The project row then showed no live dot and would not attach,
// while the session was running the whole time. 41 of the 45 projects on this
// machine carry a capital in their name.
//
// Nothing here talks to a backend: these are string rules, and they are the
// same string rules Rust applies, mirrored deliberately. `nameFor` in
// particular must predict `zellij::session_name` exactly, because the caller
// names a tab and looks a session up by the answer before `zellij_open_command`
// has returned the real one.
(function () {
  // The coding agents a session can be opened for. The key is the session-name
  // prefix the shell wrappers already use (`cl-Bucky`, `cx-Bucky`), the label is
  // what a person picks from, and `command` is the binary the pane runs.
  //
  // `xnaut` is deliberately NOT here: it prefixes dispatched fleet runs
  // (`xnaut-claude-01m2zr…`, see sandbox.rs::launch_env::session_name), which
  // belong to a ticket and a worktree rather than to a project, and offering to
  // start one from a project page would mint an agent with no work to do.
  const AGENTS = [
    { key: 'cl', label: 'Claude Code', command: 'claude' },
    { key: 'cx', label: 'Codex', command: 'codex' },
    { key: 'pi', label: 'Pi', command: 'pi' },
  ];

  // Every prefix a session name can carry, including the fleet one, because
  // READING a name has to cope with names we do not offer to create.
  const PREFIXES = AGENTS.map((agent) => agent.key).concat(['xnaut']);

  // Comparison key: lowercase, every separator dropped.
  function slug(value) {
    return String(value == null ? '' : value).toLowerCase().replace(/[^a-z0-9]+/g, '');
  }

  // The project half of a session name, slugged. '' when the name carries no
  // agent prefix, which is how a session that is not ours says so.
  function stripAgent(name) {
    const text = String(name == null ? '' : name).trim();
    const match = new RegExp('^(' + PREFIXES.join('|') + ')-(.+)$', 'i').exec(text);
    return match ? slug(match[2]) : '';
  }

  // The agent a session belongs to, or null when the name names none.
  function agentOf(name) {
    const text = String(name == null ? '' : name).trim();
    const match = /^([a-zA-Z]{2,5})-/.exec(text);
    if (!match) return null;
    const key = match[1].toLowerCase();
    return AGENTS.find((agent) => agent.key === key) || null;
  }

  // What to call an agent in a row. Falls back to the raw prefix rather than
  // inventing a name, and to 'agent' when there is no prefix at all.
  function agentLabel(name) {
    const agent = agentOf(name);
    if (agent) return agent.label;
    const match = /^([a-zA-Z]{2,5})-/.exec(String(name == null ? '' : name).trim());
    return match ? match[1].toLowerCase() : 'agent';
  }

  // Every string that could legitimately stand for this project inside a
  // session name: its name, its key, and the last segment of its path. Accepts
  // a PM project record (`name`/`key`/`source_path`) or a sidebar task row
  // (`name`/`id`/`path`), because both ask this question about the same
  // sessions and neither should have to convert itself into the other.
  //
  // Tokens shorter than three characters are dropped: a two-letter key would
  // swallow half the machine.
  function tokensOf(entity) {
    const record = entity || {};
    const path = String(record.source_path || record.path || '').replace(/\/+$/, '');
    const raw = [record.name, record.key, record.id, path.split('/').pop()];
    const seen = [];
    for (const value of raw) {
      const token = slug(value);
      if (token.length >= 3 && !seen.includes(token)) seen.push(token);
    }
    return seen;
  }

  // Does this session name belong to this project?
  //
  // Prefix matching runs BOTH ways on purpose: zellij truncates a session name
  // at 24 characters, so `cl-nautflow-incident-loo` is shorter than the project
  // it belongs to, while `cx-xnaut-safety-net` is longer than `xnaut`.
  function belongsTo(sessionName, entity) {
    const rest = stripAgent(sessionName);
    if (rest.length < 3) return false;
    return tokensOf(entity).some(
      (token) => token === rest || token.startsWith(rest) || rest.startsWith(token),
    );
  }

  // Mirrors `zellij::session_name` (src-tauri/src/zellij.rs): lowercase,
  // non-alphanumerics collapsed to a single dash, dashes trimmed, capped at the
  // 24 characters zellij 0.44 accepts. A divergence here is not cosmetic — it
  // means xNAUT looks for a session under a name that was never created.
  function sessionName(raw) {
    const name = String(raw == null ? '' : raw)
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 24)
      .replace(/-+$/, '');
    return name || 'task';
  }

  // The name a NEW session for this project takes: `<agent>-<project>`, so the
  // sidebar and the Observatory find it by the same rule they find a
  // hand-started `cl-Bucky` by — rather than the `xnaut-tab-<timestamp>` form a
  // plain terminal tab uses, which belongs to no project at all.
  function nameFor(agentKey, entity) {
    const key = String(agentKey == null ? '' : agentKey).trim().toLowerCase();
    const agent = AGENTS.find((item) => item.key === key);
    if (!agent) throw new Error('unknown agent for a session name: ' + agentKey);
    const record = entity || {};
    const path = String(record.source_path || record.path || '').replace(/\/+$/, '');
    const project = record.name || record.key || record.id || path.split('/').pop() || '';
    return sessionName(agent.key + '-' + project);
  }

  window.xnautSessions = {
    AGENTS,
    agentOf,
    agentLabel,
    belongsTo,
    nameFor,
    sessionName,
    slug,
    stripAgent,
    tokensOf,
  };
})();
