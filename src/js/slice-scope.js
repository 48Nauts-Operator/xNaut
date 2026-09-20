// Who owns this path right now — the one answer every save has to ask.
//
// During a build, several agents are writing into several worktrees at once.
// The right pane can be rooted at any of them (XNAUT-106), which means a human
// can now click a file an agent is halfway through writing. Opening it is fine
// and is the whole point of the ticket. SAVING it is a merge conflict with
// extra steps: the agent commits its version seconds later, and whichever lands
// second quietly wins.
//
// So the rule lives here, once, rather than in each of the three surfaces that
// can write: the markdown pane, the built-in editor, and $EDITOR-in-a-terminal.
//
// The source of truth is window.xnautBuild.queue — the same list the Build run
// pane and the slice tabs read, republished by publishBuildToSwarm on every
// status change. Nothing is cached here: a slice that finished one second ago
// must stop being read-only one second ago.
(function () {
  'use strict';

  const clean = (p) => String(p || '').replace(/\/+$/, '');

  // Longest-prefix match, not first match. Build worktrees sit side by side
  // under one parent, and one slug can be a prefix of another
  // (nautloom/api and nautloom/api-docs), so a first match would attribute a
  // file to the wrong agent — the exact confusion the provenance band exists
  // to prevent. The boundary check ('/' after the prefix) is what stops
  // .../api-docs/x.js from matching .../api.
  function ownerOf(path, onlyRunning) {
    const file = clean(path);
    if (!file) return null;
    const queue = (window.xnautBuild && window.xnautBuild.queue) || [];
    let best = null;
    let bestLen = -1;
    for (const slice of queue) {
      if (!slice || !slice.wt) continue;
      if (onlyRunning && slice.status !== 'running') continue;
      const wt = clean(slice.wt);
      if (!wt) continue;
      if (file !== wt && !file.startsWith(wt + '/')) continue;
      if (wt.length > bestLen) { best = slice; bestLen = wt.length; }
    }
    return best;
  }

  // The RUNNING slice whose worktree contains this path, or null.
  window.xnautLiveSliceFor = (path) => ownerOf(path, true);
  // The slice whose worktree contains this path whatever its status — used for
  // provenance ("which worktree am I looking at"), never for the write guard.
  window.xnautSliceFor = (path) => ownerOf(path, false);

  // Refuse a write into a worktree an agent is actively writing, and say whose
  // it is. Returns true when the write was blocked, so a caller reads as:
  //   if (window.xnautSliceWriteBlocked(path)) return;
  window.xnautSliceWriteBlocked = (path) => {
    const slice = ownerOf(path, true);
    if (!slice) return false;
    const who = slice.title || slice.id || slice.branch || 'an agent';
    const msg = `Read-only while ${who} is building here. Stop the slice, or edit after it finishes.`;
    if (typeof window.xnautToast === 'function') window.xnautToast(msg);
    else console.warn('[slice-scope] %s', msg);
    return true;
  };
})();
