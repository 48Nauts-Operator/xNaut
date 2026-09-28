// ABOUTME: XNAUT main application - Native terminal with Tauri backend integration
// ABOUTME: Manages terminals, AI features, SSH, workflows, and all UI interactions using Tauri invoke/listen

// Show that JavaScript loaded
console.log('🎯 app.js loaded successfully!');

// Tauri API - will be set when available
let invoke, listen;

// State
let settings = {};
let tabs = [];
let activeTabId = null;
let sessionCounter = 0;
// Workspace scoping (Orca/CMUX model): every tab belongs to a project workspace.
// 'home' holds standalone terminals + the global panels (Tasks/Automations/PM).
let activeProjectId = 'home';
let activeProjectPath = null; // local path of the active project; drives the open-repo button
const activeTabByProject = {}; // projectId -> last active tabId
let terminalOutputBuffer = '';
let maxBufferSize = 5000;
let commandHistory = [];
let isFirstTerminal = true;
let currentCommandBuffer = '';
let maxHistorySize = 1000;
let sshProfiles = [];
let editingSSHProfileId = null;
let activeSSHConnections = new Map();
let chatHistory = [];
let triggers = [];
let editingTriggerId = null;
let commandSnippets = [];
let editingSnippetId = null;
let snippetCategories = ['Docker', 'Deployment', 'Build', 'Git', 'Database', 'Testing'];
let activeSnippetCategory = null; // Filter by category

// UI Elements - will be initialized when DOM is ready
let statusDot;
let statusText;
let tabsContainer;
let terminalContainer;

// Track current directory for each session
const sessionDirectories = new Map();
// Per-frontend-session cache of {dir, branch} used to auto-name tabs.
const sessionContext = new Map();

// ==================== Split Screen Layout Engine ====================

// Layout templates: CSS Grid configurations for each layout type
// Each pane gets an area name ('a','b','c','d') mapped to grid-template-areas
const LAYOUT_TEMPLATES = {
  single: {
    columns: '1fr',
    rows: '1fr',
    areas: '"a"',
    panes: ['a']
  },
  vsplit: {
    columns: '1fr 1fr',
    rows: '1fr',
    areas: '"a b"',
    panes: ['a', 'b']
  },
  hsplit: {
    columns: '1fr',
    rows: '1fr 1fr',
    areas: '"a" "b"',
    panes: ['a', 'b']
  },
  // 3-pane: left full-height + two stacked right
  'left-right2': {
    columns: '1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b" "a c"',
    panes: ['a', 'b', 'c']
  },
  // 3-pane: two stacked left + right full-height
  'left2-right': {
    columns: '1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b" "c b"',
    panes: ['a', 'b', 'c']
  },
  // 3-pane: top full-width + two side-by-side bottom
  'top-bottom2': {
    columns: '1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a a" "b c"',
    panes: ['a', 'b', 'c']
  },
  // 3-pane: two side-by-side top + bottom full-width
  'top2-bottom': {
    columns: '1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b" "c c"',
    panes: ['a', 'b', 'c']
  },
  grid: {
    columns: '1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b" "c d"',
    panes: ['a', 'b', 'c', 'd']
  },
  // 5-pane: 3 columns top + 2 columns bottom
  'grid-5a': {
    columns: '1fr 1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b c" "d e e"',
    panes: ['a', 'b', 'c', 'd', 'e']
  },
  // 5-pane: 2 columns top + 3 columns bottom
  'grid-5b': {
    columns: '1fr 1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a a b" "c d e"',
    panes: ['a', 'b', 'c', 'd', 'e']
  },
  // 6-pane: 3x2 grid
  'grid-6': {
    columns: '1fr 1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b c" "d e f"',
    panes: ['a', 'b', 'c', 'd', 'e', 'f']
  },
  // 7-pane: 4 top + 3 bottom
  'grid-7': {
    columns: '1fr 1fr 1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b c d" "e f g g"',
    panes: ['a', 'b', 'c', 'd', 'e', 'f', 'g']
  },
  // 8-pane: 4x2 grid
  'grid-8': {
    columns: '1fr 1fr 1fr 1fr',
    rows: '1fr 1fr',
    areas: '"a b c d" "e f g h"',
    panes: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h']
  },
  // 9-pane: 3x3 grid
  'grid-9': {
    columns: '1fr 1fr 1fr',
    rows: '1fr 1fr 1fr',
    areas: '"a b c" "d e f" "g h i"',
    panes: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i']
  }
};

// Generate dynamic grid layouts for any pane count
function generateDynamicGrid(paneCount) {
  const cols = Math.ceil(Math.sqrt(paneCount));
  const rows = Math.ceil(paneCount / cols);
  const paneIds = 'abcdefghijklmnop'.slice(0, paneCount).split('');

  let areas = '';
  let idx = 0;
  for (let r = 0; r < rows; r++) {
    let row = '"';
    for (let c = 0; c < cols; c++) {
      row += (c > 0 ? ' ' : '') + (idx < paneCount ? paneIds[idx] : paneIds[paneCount - 1]);
      idx++;
    }
    row += '"';
    areas += (r > 0 ? ' ' : '') + row;
  }

  LAYOUT_TEMPLATES[`grid-${paneCount}`] = {
    columns: Array(cols).fill('1fr').join(' '),
    rows: Array(rows).fill('1fr').join(' '),
    areas: areas,
    panes: paneIds,
  };
}

// Pre-generate grids for 10-16 panes
for (let i = 10; i <= 16; i++) generateDynamicGrid(i);

// State machine: given current layout and split direction, what's the next layout?
function getNextLayout(currentLayout, direction) {
  const paneCount = LAYOUT_TEMPLATES[currentLayout].panes.length;

  if (paneCount >= 16) return null; // Max 16 panes

  if (paneCount === 1) {
    return direction === 'vertical' ? 'vsplit' : 'hsplit';
  }

  if (paneCount === 2) {
    if (currentLayout === 'vsplit') {
      return direction === 'vertical' ? 'top2-bottom' : 'left-right2';
    }
    if (currentLayout === 'hsplit') {
      return direction === 'vertical' ? 'top-bottom2' : 'left2-right';
    }
  }

  if (paneCount === 3) {
    return 'grid';
  }

  if (paneCount === 4) {
    return direction === 'vertical' ? 'grid-5a' : 'grid-5b';
  }

  if (paneCount === 5) {
    return 'grid-6';
  }

  if (paneCount === 6) {
    return 'grid-7';
  }

  if (paneCount === 7) {
    return 'grid-8';
  }

  if (paneCount >= 8) {
    // Dynamic: generate grid-N template on the fly
    const next = paneCount + 1;
    const templateName = `grid-${next}`;
    if (!LAYOUT_TEMPLATES[templateName]) {
      generateDynamicGrid(next);
    }
    return templateName;
  }

  return null;
}

// Apply a layout template to the terminal container for the given tab
function applyLayout(tab) {
  const template = LAYOUT_TEMPLATES[tab.layoutType];
  if (!template) return;

  // Apply custom sizes if user has resized via dividers
  const cols = tab.colSizes ? tab.colSizes.join(' ') : template.columns;
  const rows = tab.rowSizes ? tab.rowSizes.join(' ') : template.rows;

  if (tab.layoutType === 'single') {
    terminalContainer.classList.remove('grid-mode');
    terminalContainer.style.display = 'flex';
    terminalContainer.style.gridTemplateColumns = '';
    terminalContainer.style.gridTemplateRows = '';
    terminalContainer.style.gridTemplateAreas = '';
  } else {
    terminalContainer.classList.add('grid-mode');
    terminalContainer.style.display = 'grid';
    terminalContainer.style.gridTemplateColumns = cols;
    terminalContainer.style.gridTemplateRows = rows;
    terminalContainer.style.gridTemplateAreas = template.areas;
  }

  // Assign grid areas to each pane
  tab.terminals.forEach((terminal, i) => {
    const paneId = terminal.paneId || template.panes[i] || 'a';
    terminal.paneId = paneId;
    terminal.pane.style.gridArea = paneId;
  });

  // Update focus indicators
  updateFocusIndicator(tab);

  // Remove old dividers and add new ones
  removeSplitDividers();
  if (tab.layoutType !== 'single') {
    addSplitDividers(tab);
  }

  // Refit all terminals after layout change
  setTimeout(() => {
    tab.terminals.forEach(t => {
      if (t.fitAddon) {
        try {
          t.fitAddon.fit();
          // Notify backend of new size
          if (t.sessionId && invoke) {
            window.xnautZellijSettle(t.sessionId);
            invoke('resize_terminal', {
              sessionId: t.sessionId,
              cols: t.term.cols,
              rows: t.term.rows
            }).catch(() => {});
          }
        } catch (e) { /* ignore */ }
      }
    });
  }, 50);
}

// Split the currently focused pane in a direction ('vertical' or 'horizontal')
// Binary tree split algorithm (inspired by Warp's implementation)
// See: https://dev.to/warpdotdev/using-tree-data-structures-to-implement-terminal-split-panes

async function splitPane(direction, kind) {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || tab.terminals.length >= 16) return;

  const focusedTerminal = tab.terminals[tab.focusedPaneIndex || 0];
  if (!focusedTerminal) return;

  const focusedPane = focusedTerminal.pane;
  const parent = focusedPane.parentNode;

  // Create a branch node (flex container) replacing the focused leaf
  const branch = document.createElement('div');
  branch.className = 'split-branch';
  branch.style.cssText = 'display:flex; flex:' + (focusedPane.style.flex || '1') + '; min-height:0; min-width:0; overflow:hidden; flex-direction:' + (direction === 'vertical' ? 'row' : 'column') + ';';

  // Replace focused pane with the branch
  parent.replaceChild(branch, focusedPane);

  // Add focused pane as first child of branch
  focusedPane.style.flex = '1';
  branch.appendChild(focusedPane);

  // Add a resize divider
  const divider = document.createElement('div');
  divider.className = 'split-resize-handle';
  divider.style.cssText = direction === 'vertical'
    ? 'width:4px; cursor:col-resize; background:var(--border); flex-shrink:0;'
    : 'height:4px; cursor:row-resize; background:var(--border); flex-shrink:0;';
  setupResizeHandle(divider, branch, direction);
  branch.appendChild(divider);

  // Phase 6: when kind='browser', the new pane is a Tauri child webview
  // instead of a PTY terminal. This gives terminal+browser side-by-side
  // in a single tab without needing two separate tabs.
  if (kind === 'browser' && typeof window.xnautCreateBrowserPane === 'function') {
    try {
      const entry = await window.xnautCreateBrowserPane(tab.id, branch);
      if (entry) tab.terminals.push(entry);
    } catch (e) {
      console.error('Failed to add browser pane in split:', e);
    }
  } else if (kind === 'markdown' && typeof window.xnautCreateMarkdownPane === 'function') {
    // Phase 7: markdown editor pane next to a terminal — write notes
    // beside your work without switching tabs.
    try {
      const entry = await window.xnautCreateMarkdownPane(tab.id, branch, {});
      if (entry) tab.terminals.push(entry);
    } catch (e) {
      console.error('Failed to add markdown pane in split:', e);
    }
  } else if (kind === 'diff' && typeof window.xnautCreateDiffPane === 'function') {
    // Phase 8a: diff viewer pane next to a terminal — review what an agent
    // is doing in a worktree while the agent keeps writing in the terminal.
    try {
      const entry = await window.xnautCreateDiffPane(tab.id, branch, {});
      if (entry) tab.terminals.push(entry);
    } catch (e) {
      console.error('Failed to add diff pane in split:', e);
    }
  } else if (kind === 'graph' && typeof window.xnautCreateGraphPane === 'function') {
    // Vault knowledge-graph orb next to your work.
    try {
      const entry = await window.xnautCreateGraphPane(tab.id, branch, splitPane._graphOpts || {});
      if (entry) tab.terminals.push(entry);
    } catch (e) {
      console.error('Failed to add graph pane in split:', e);
    }
  } else {
    // Create new terminal pane as second child
    await createTerminal(tab.id, 'p' + (++sessionCounter), branch);
  }

  // Refit all
  refitAllTerminals(tab);
  console.log('Split ' + direction + (kind ? ' (' + kind + ')' : '') + ': ' + tab.terminals.length + ' panes');
}

function setupResizeHandle(handle, branch, direction) {
  let startPos = 0;
  let startSizes = [];

  handle.addEventListener('mousedown', (e) => {
    e.preventDefault();
    startPos = direction === 'vertical' ? e.clientX : e.clientY;
    const children = Array.from(branch.children).filter(c => !c.classList.contains('split-resize-handle'));
    startSizes = children.map(c => direction === 'vertical' ? c.offsetWidth : c.offsetHeight);

    const onMouseMove = (e) => {
      const delta = (direction === 'vertical' ? e.clientX : e.clientY) - startPos;
      const total = startSizes.reduce((a, b) => a + b, 0);
      if (total === 0) return;
      const newFirst = Math.max(50, startSizes[0] + delta);
      const newSecond = Math.max(50, total - newFirst);
      children[0].style.flex = (newFirst / total).toFixed(4);
      if (children[1]) children[1].style.flex = (newSecond / total).toFixed(4);
    };

    const onMouseUp = () => {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      // Refit terminals after resize
      const tab = tabs.find(t => t.id === activeTabId);
      if (tab) refitAllTerminals(tab);
    };

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  });
}

// Place a floating element (a context menu) where the click was.
//
// Every menu in the app got this wrong the same way. The interface zooms with
// CSS `zoom` on the root, so a fixed-position child is laid out in a space that
// is multiplied by the zoom, while event.clientX/Y arrive already in zoomed
// pixels. Assigning one to the other therefore lands the menu at click x zoom:
// at 1.25 a right-click near the top of the Files pane opened its menu a third
// of the way down the screen. window.innerWidth/Height are in the same zoomed
// space and need the same division before they can clamp anything.
window.xnautPlaceAtClick = function placeAtClick(el, x, y, pad = 8) {
  const zoom = Number(window.xnautUiZoom) > 0 ? Number(window.xnautUiZoom) : 1;
  const r = el.getBoundingClientRect();
  const maxLeft = window.innerWidth / zoom - r.width / zoom - pad;
  const maxTop = window.innerHeight / zoom - r.height / zoom - pad;
  el.style.left = `${Math.max(0, Math.min(x / zoom, maxLeft))}px`;
  el.style.top = `${Math.max(0, Math.min(y / zoom, maxTop))}px`;
};

// Terminals opt OUT of the interface zoom (see applyAppZoom) and scale their
// font instead, so this is the size xterm is actually given.
function terminalFontSize() {
  const zoom = Number(window.xnautUiZoom) > 0 ? Number(window.xnautUiZoom) : 1;
  return (settings.fontSize || 14) * zoom;
}

function refitAllTerminals(tab) {
  requestAnimationFrame(() => {
    setTimeout(() => {
      tab.terminals.forEach(t => {
        if (t.fitAddon) {
          try {
            t.fitAddon.fit();
            window.xnautLastTermSize = { cols: t.term.cols, rows: t.term.rows };
            window.xnautZellijSettle(t.sessionId);
            invoke('resize_terminal', { sessionId: t.sessionId, cols: t.term.cols, rows: t.term.rows }).catch(() => {});
          } catch (e) {}
        }
      });
    }, 100);
  });
}

// Close the currently focused pane
window.xnautClosePaneByElement = (paneElement, tabId) => closePaneByElement(paneElement, tabId);

async function closePaneByElement(paneElement, tabId) {
  const tab = tabs.find(t => t.id === tabId);
  if (!tab) return;

  const idx = tab.terminals.findIndex(t => t.pane === paneElement);
  if (idx < 0) return;

  if (tab.terminals.length <= 1) {
    closeTab(tabId);
    return;
  }

  const terminal = tab.terminals[idx];
  try {
    // Free the underlying resource without removing the pane DOM (the tree
    // collapse below promotes the sibling and removes this pane element).
    if (terminal.kind === 'browser' && terminal.label) {
      await invoke('browser_pane_destroy', { label: terminal.label }).catch(() => {});
      if (window.xnautForgetBrowserPane) window.xnautForgetBrowserPane(terminal.label);
    } else if (terminal.kind === 'graph' && terminal.label) {
      if (window.xnautForgetGraphPane) window.xnautForgetGraphPane(terminal.label);
    } else if (terminal.sessionId) {
      await invoke('close_terminal', { sessionId: terminal.sessionId });
      if (terminal.handleResize) window.removeEventListener('resize', terminal.handleResize);
    }
  } catch (e) {
    console.error('Error closing pane:', e);
  }

  // Tree collapse: remove pane and promote its sibling
  const parent = paneElement.parentNode;
  if (parent && parent.classList.contains('split-branch')) {
    // Find the sibling (the other child that isn't a resize handle)
    const siblings = Array.from(parent.children).filter(
      c => c !== paneElement && !c.classList.contains('split-resize-handle')
    );
    const sibling = siblings[0];

    if (sibling) {
      // Replace the branch with the sibling
      const grandparent = parent.parentNode;
      sibling.style.flex = parent.style.flex || '1';
      grandparent.replaceChild(sibling, parent);
    }
  } else {
    // Fallback: just remove the pane
    if (paneElement.parentNode) paneElement.parentNode.removeChild(paneElement);
  }

  tab.terminals.splice(idx, 1);

  // Fix focus
  if (tab.focusedPaneIndex >= tab.terminals.length) {
    tab.focusedPaneIndex = tab.terminals.length - 1;
  }

  // Refit remaining terminals
  refitAllTerminals(tab);

  // Force shell redraw
  setTimeout(() => {
    tab.terminals.forEach(async (t) => {
      try {
        await invoke('write_to_terminal', { sessionId: t.sessionId, data: '\x0c' });
      } catch (e) {}
    });
  }, 300);
}

async function closePane() {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || tab.terminals.length <= 1) {
    // Last pane — close the whole tab
    if (activeTabId) closeTab(activeTabId);
    return;
  }

  const focusedIdx = tab.focusedPaneIndex || 0;
  const terminal = tab.terminals[focusedIdx];

  if (!terminal) return;

  // Close the PTY session
  try {
    await invoke('close_terminal', { sessionId: terminal.sessionId });
    window.removeEventListener('resize', terminal.handleResize);
  } catch (e) {
    console.error('Error closing pane:', e);
  }

  // Remove from DOM and tab
  if (terminal.pane.parentNode) {
    terminal.pane.parentNode.removeChild(terminal.pane);
  }
  tab.terminals.splice(focusedIdx, 1);

  // Determine new layout
  const paneCount = tab.terminals.length;
  if (paneCount === 1) {
    tab.layoutType = 'single';
  } else if (paneCount === 2) {
    tab.layoutType = 'vsplit';
  } else if (paneCount === 3) {
    tab.layoutType = 'left-right2';
  } else if (paneCount === 4) {
    tab.layoutType = 'grid';
  } else if (paneCount === 5) {
    tab.layoutType = 'grid-5a';
  }

  // Reassign pane IDs based on new layout
  const newTemplate = LAYOUT_TEMPLATES[tab.layoutType];
  tab.terminals.forEach((t, i) => {
    t.paneId = newTemplate.panes[i];
  });

  // Adjust focus index
  tab.focusedPaneIndex = Math.min(focusedIdx, tab.terminals.length - 1);
  tab.colSizes = null;
  tab.rowSizes = null;

  applyLayout(tab);

  // Focus the new active pane
  const newFocused = tab.terminals[tab.focusedPaneIndex];
  if (newFocused) newFocused.term.focus();
}

// Navigate between panes using arrow direction
function navigatePane(arrowDir) {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || tab.terminals.length <= 1) return;

  const template = LAYOUT_TEMPLATES[tab.layoutType];
  if (!template) return;

  // Parse grid areas into a 2D map for spatial navigation
  const areaRows = template.areas.replace(/"/g, '').split(/\s*"\s*/).filter(Boolean);
  const grid = areaRows.map(row => row.trim().split(/\s+/));

  // Find current position in grid
  const currentPaneId = tab.terminals[tab.focusedPaneIndex]?.paneId || 'a';
  let curRow = -1, curCol = -1;
  for (let r = 0; r < grid.length; r++) {
    for (let c = 0; c < grid[r].length; c++) {
      if (grid[r][c] === currentPaneId) {
        curRow = r;
        curCol = c;
        break;
      }
    }
    if (curRow >= 0) break;
  }

  // Move in direction
  let targetRow = curRow, targetCol = curCol;
  if (arrowDir === 'ArrowLeft') targetCol = Math.max(0, curCol - 1);
  else if (arrowDir === 'ArrowRight') targetCol = Math.min(grid[0].length - 1, curCol + 1);
  else if (arrowDir === 'ArrowUp') targetRow = Math.max(0, curRow - 1);
  else if (arrowDir === 'ArrowDown') targetRow = Math.min(grid.length - 1, curRow + 1);

  const targetPaneId = grid[targetRow]?.[targetCol];
  if (!targetPaneId || targetPaneId === currentPaneId) return;

  // Find terminal index with this pane ID
  const idx = tab.terminals.findIndex(t => t.paneId === targetPaneId);
  if (idx >= 0) {
    tab.focusedPaneIndex = idx;
    updateFocusIndicator(tab);
    tab.terminals[idx].term.focus();
  }
}

// Update CSS classes for focused/unfocused panes
function updateFocusIndicator(tab) {
  if (!tab) return;
  tab.terminals.forEach((terminal, i) => {
    if (i === tab.focusedPaneIndex) {
      terminal.pane.classList.add('pane-focused');
      terminal.pane.classList.remove('pane-unfocused');
      // Use frontend sessionId because per-pane element ids are frontend-scoped
      updateSharedStatusBar(terminal.frontendSessionId || terminal.pane.dataset.sessionId);
    } else {
      terminal.pane.classList.remove('pane-focused');
      terminal.pane.classList.add('pane-unfocused');
    }
  });
}

// Remove all existing split dividers
function removeSplitDividers() {
  document.querySelectorAll('.split-divider').forEach(d => d.remove());
}

// Add draggable dividers between panes
function addSplitDividers(tab) {
  const template = LAYOUT_TEMPLATES[tab.layoutType];
  if (!template) return;

  // Determine divider positions from layout
  const cols = template.columns.split(' ').length;
  const rows = template.rows.split(' ').length;

  // Vertical dividers (between columns)
  if (cols > 1) {
    const divider = document.createElement('div');
    divider.className = 'split-divider split-divider-vertical';
    divider.style.left = '50%';
    terminalContainer.appendChild(divider);
    setupDividerDrag(divider, 'vertical', tab);
  }

  // Horizontal dividers (between rows)
  if (rows > 1) {
    const divider = document.createElement('div');
    divider.className = 'split-divider split-divider-horizontal';
    divider.style.top = '50%';
    terminalContainer.appendChild(divider);
    setupDividerDrag(divider, 'horizontal', tab);
  }
}

// Make a divider draggable to resize panes
function setupDividerDrag(divider, orientation, tab) {
  let isDragging = false;

  divider.addEventListener('mousedown', (e) => {
    isDragging = true;
    document.body.style.cursor = orientation === 'vertical' ? 'col-resize' : 'row-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  });

  const onMouseMove = (e) => {
    if (!isDragging) return;

    const rect = terminalContainer.getBoundingClientRect();

    if (orientation === 'vertical') {
      const pct = ((e.clientX - rect.left) / rect.width) * 100;
      const clamped = Math.max(20, Math.min(80, pct));
      tab.colSizes = [`${clamped}%`, `${100 - clamped}%`];
      divider.style.left = clamped + '%';
    } else {
      const pct = ((e.clientY - rect.top) / rect.height) * 100;
      const clamped = Math.max(20, Math.min(80, pct));
      tab.rowSizes = [`${clamped}%`, `${100 - clamped}%`];
      divider.style.top = clamped + '%';
    }

    // Apply updated sizes
    const template = LAYOUT_TEMPLATES[tab.layoutType];
    terminalContainer.style.gridTemplateColumns = tab.colSizes ? tab.colSizes.join(' ') : template.columns;
    terminalContainer.style.gridTemplateRows = tab.rowSizes ? tab.rowSizes.join(' ') : template.rows;

    // Refit terminals
    requestAnimationFrame(() => {
      tab.terminals.forEach(t => {
        if (t.fitAddon) try { t.fitAddon.fit(); } catch (e) { /* ignore */ }
      });
    });
  };

  const onMouseUp = () => {
    if (!isDragging) return;
    isDragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';

    // Final refit + notify backend
    tab.terminals.forEach(t => {
      if (t.fitAddon) {
        try {
          t.fitAddon.fit();
          if (t.sessionId && invoke) {
            window.xnautZellijSettle(t.sessionId);
            invoke('resize_terminal', {
              sessionId: t.sessionId,
              cols: t.term.cols,
              rows: t.term.rows
            }).catch(() => {});
          }
        } catch (e) { /* ignore */ }
      }
    });
  };

  document.addEventListener('mousemove', onMouseMove);
  document.addEventListener('mouseup', onMouseUp);
}

// ==================== End Split Screen Layout Engine ====================

// Returns true if the given backend sessionId is the currently focused pane
// in the active tab. The shared status bar should only be updated for that
// session — otherwise multiple terminals fight over it and cause flicker.
function isFocusedBackendSession(backendSessionId) {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || !tab.terminals.length) return false;
  const focused = tab.terminals[tab.focusedPaneIndex || 0];
  return focused && focused.sessionId === backendSessionId;
}

// Update directory and git status in the status bar
async function updateDirectoryStatus(sessionId, backendSessionId, directory = null) {
  try {
    // Get home directory for path formatting
    const homeDir = await invoke('get_home_directory');

    // Use provided directory or get from map (default to home)
    let currentDir = directory || sessionDirectories.get(sessionId) || homeDir;
    sessionDirectories.set(sessionId, currentDir);

    const statusPath = document.getElementById(`status-path-${sessionId}`);
    const displayPath = currentDir.replace(homeDir, '~');
    if (statusPath) {
      statusPath.textContent = displayPath;
    }
    // Only update the shared status bar when this is the focused pane —
    // otherwise concurrent polls from multiple terminals cause flicker.
    const isFocused = isFocusedBackendSession(backendSessionId);
    if (isFocused) {
      const sharedPath = document.getElementById('shared-status-path');
      if (sharedPath) sharedPath.textContent = displayPath;
    }

    // Get git info for current directory
    let branch = null;
    try {
      const gitInfo = await invoke('get_git_info', { path: currentDir });

      const statusGit = document.getElementById(`status-git-${sessionId}`);
      const sharedGit = isFocused ? document.getElementById('shared-status-git') : null;
      const gitHtml = gitInfo && gitInfo.is_repo
        ? `<span class="git-branch">⎇ ${gitInfo.branch}</span>${gitInfo.changes > 0 ? ` <span class="git-stats">• ${gitInfo.changes} ${gitInfo.changes === 1 ? 'change' : 'changes'}</span>` : ''}`
        : '';
      if (statusGit) statusGit.innerHTML = gitHtml;
      if (sharedGit) sharedGit.innerHTML = gitHtml;
      if (gitInfo && gitInfo.is_repo) branch = gitInfo.branch;
    } catch (gitError) {
      // Not in a git repo, silently ignore
      const statusGit = document.getElementById(`status-git-${sessionId}`);
      if (statusGit) {
        statusGit.textContent = '';
      }
    }

    // Cache context and refresh tab auto-name (only for the focused pane)
    sessionContext.set(sessionId, { dir: currentDir, branch });
    if (isFocused) refreshAutoTabName(backendSessionId);
  } catch (error) {
    console.error('Error updating directory status:', error);
  }
}

// Compute the auto-name for a tab based on its focused pane's cwd/branch.
// Returns null if no good name can be derived (e.g. no terminals yet).
function computeAutoTabName(tab) {
  if (!tab || !tab.terminals.length) return null;
  const focused = tab.terminals[tab.focusedPaneIndex || 0];
  const ctx = sessionContext.get(focused.frontendSessionId);
  if (!ctx) return null;
  if (ctx.branch) return `⎇ ${ctx.branch}`;
  if (ctx.dir) {
    // Last path segment, with ~ if home
    const seg = ctx.dir.replace(/\/$/, '').split('/').pop();
    return seg || ctx.dir;
  }
  return null;
}

// Refresh the displayed name of the tab containing the given backend session.
// Skipped when the user has manually renamed the tab (saved name wins).
function refreshAutoTabName(backendSessionId) {
  const tab = tabs.find(t => t.terminals.some(term => term.sessionId === backendSessionId));
  if (!tab) return;
  const savedNames = loadTabNames();
  if (savedNames[tab.id]) return; // user override
  // A session tab is named after its session (or the owner's alias for it),
  // never after the folder the shell happens to be in.
  if (tab.zellijSession) return;
  const auto = computeAutoTabName(tab);
  if (auto && auto !== tab.name) {
    tab.name = auto;
    renderTabs();
  }
}

// Function to wait for Tauri API to be available
async function waitForTauri(maxWaitMs = 3000) {
  const startTime = Date.now();
  let attempts = 0;

  while (!window.__TAURI__) {
    attempts++;
    const elapsed = Date.now() - startTime;

    if (elapsed > maxWaitMs) {
      console.error(`❌ Tauri API did not load after ${attempts} attempts (${elapsed}ms)`);
      console.error('window.__TAURI__ =', window.__TAURI__);
      console.error('This likely means withGlobalTauri is not enabled or the build failed');
      throw new Error(`Tauri API did not load within ${maxWaitMs}ms`);
    }

    if (attempts % 10 === 1) {
      console.log(`⏳ Waiting for Tauri API... (attempt ${attempts}, ${elapsed}ms)`);
    }

    await new Promise(resolve => setTimeout(resolve, 50));
  }

  console.log(`✅ Tauri API available after ${attempts} attempts (${Date.now() - startTime}ms)`);

  // Set up invoke and listen
  invoke = window.__TAURI__.core.invoke;
  listen = window.__TAURI__.event.listen;

  console.log('✅ invoke and listen functions ready');
  return true;
}

// Wait for DOM to be ready before initializing
document.addEventListener('DOMContentLoaded', async () => {
  console.log('🚀 DOM Ready, initializing xNAUT...');

  // NOW we can safely access DOM elements
  statusDot = document.getElementById('status-dot');
  statusText = document.getElementById('status-text');
  tabsContainer = document.getElementById('tabs-container');
  terminalContainer = document.getElementById('terminal-container');

  // Drag a file/folder from the right-pane tree onto a terminal to insert its path.
  if (terminalContainer) {
    terminalContainer.addEventListener('dragover', (e) => {
      if (e.dataTransfer && Array.from(e.dataTransfer.types || []).includes('application/x-xnaut-path')) {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
      }
    });
    terminalContainer.addEventListener('drop', (e) => {
      const path = e.dataTransfer && (e.dataTransfer.getData('application/x-xnaut-path') || e.dataTransfer.getData('text/plain'));
      if (!path) return;
      e.preventDefault();
      if (window.xnautInjectPathIntoTerminal) window.xnautInjectPathIntoTerminal(path);
    });
  }

  console.log('DOM Elements:', {
    statusDot: !!statusDot,
    statusText: !!statusText,
    tabsContainer: !!tabsContainer,
    terminalContainer: !!terminalContainer
  });

  // Wait for Tauri API
  try {
    console.log('Waiting for Tauri API...');
    if (statusText) statusText.textContent = 'Loading Tauri API...';

    await waitForTauri();

    console.log('Tauri API ready, starting initialization...');

    // Initialize the application
    await init();

    initVerifyPill();
  } catch (error) {
    console.error('❌ Failed to load Tauri API:', error);
    if (statusText) statusText.textContent = '❌ Tauri API Missing';

    // This is the earliest thing that can fail and the one that leaves the
    // most inert window, so it is the one that most needed a surface that is
    // not alert() (XNAUT-75). Nothing after this point runs, so seal here too.
    const health = startupHealth();
    health.fail('Tauri API', error);
    health.seal();
  }
});

// The startup health record (js/startup-health.js), which is loaded before this
// file and assigns window.xnautStartupHealth. The fallback is not defensive
// habit: this is the reporting path for a broken startup, and a surface that
// throws while reporting a failure would restore exactly the silence XNAUT-75
// exists to end. If the recorder itself did not load, init still runs.
function startupHealth() {
  const real = window.xnautStartupHealth;
  if (real) return real;
  const noop = () => {};
  return { pass: noop, fail: noop, seal: noop, show: noop, hide: noop, report: () => '', steps: () => [], failures: () => [], sealed: () => false };
}

async function init() {
  console.log('🚀 XNAUT Initializing...');
  console.log('✅ Tauri API available');
  const health = startupHealth();

  try {
    // Each data load is isolated. These all used to run bare, so a single throw
    // skipped setupEventListeners() and createNewTab() below — leaving a window
    // with no terminal and dead +, three-dot and theme buttons, and no visible
    // error, because the catch reported via alert(), which was a no-op in
    // WKWebView until dialogs.js replaced it (XNAUT-80).
    // A fresh profile hit exactly that (XNAUT-74). Losing one panel's state is
    // survivable; losing the whole UI is not.
    // Every phase reports its outcome to the startup health record, pass and
    // fail alike (XNAUT-75). The failures raise a visible banner; the passes
    // are what let the detail answer "which subsystems came up", which is the
    // question a user with a half-working window is actually asking.
    const step = async (label, fn) => {
      try {
        await fn();
        health.pass(label);
      } catch (e) {
        console.error(`⚠️ init step "${label}" failed (continuing):`, e);
        health.fail(label, e);
      }
    };

    await step('settings', loadSettings);
    await step('command history', loadCommandHistory);
    await step('ssh profiles', loadSSHProfiles);
    await step('triggers', loadTriggers);
    await step('chat sessions', initChatSessions);
    await step('snippets', loadSnippets);
    await step('notification permission', requestNotificationPermission);
    await step('shared status bar', initSharedStatusBar);
    await step('active worklog', checkActiveWorklog);
    await step('clawproxy', checkClawProxy);
    console.log('✅ Data loaded, setting up event listeners...');
    try {
      setupEventListeners();
      console.log('✅ Event listeners set up');
      health.pass('event listeners');
    } catch (err) {
      console.error('❌ Event listeners error:', err);
      health.fail('event listeners', err);
      throw err;
    }

    // Create initial terminal tab
    console.log('📝 Creating initial terminal tab...');
    createNewTab();
    health.pass('first terminal tab');

    console.log('✅ XNAUT Ready!');
    if (statusText) statusText.textContent = 'Ready';

    // Wire up drag-and-drop of files into the focused terminal
    setupTerminalDragDrop();
    setupMobileBridgeListener();
    // Automation fires are announced app-wide, not per-panel. The listener
    // used to live in the Automations panel's constructor, so a fire on an
    // app that had never opened it went unseen (tron rig, 2026-09-01).
    if (window.xnautWireAutomationFire) window.xnautWireAutomationFire();

    // Check for updates after startup
    setTimeout(() => checkForUpdates(), 3000);
    if (statusDot) statusDot.classList.add('connected');
  } catch (error) {
    console.error('❌ Initialization error:', error);
    if (statusText) statusText.textContent = '❌ Init Failed';
    // Was alert(), which is invisible in WKWebView, and is a toast since
    // XNAUT-80 — gone in eight seconds whether or not anyone was looking. A
    // failed startup is a state, so it gets a banner that stays (XNAUT-75).
    health.fail('initialization', error);
  }
  // Seal after the whole run, pass or fail, so the detail can say how many of
  // how many came up rather than reporting on a list still being written.
  health.seal();
}

function updateStatus(message) {
  statusText.textContent = message;
}

// Shared Status Bar (one per app, controls apply to focused pane)
// ==================== Settings Panel ====================
// ==================== Auto-Update ====================
// Asked of the app rather than hardcoded. The constant that used to live here
// stopped being bumped at 1.5.0, so every release after it compared against
// 1.5.0 and concluded an update was available, including the one running.
async function runningVersion() {
  try {
    return (await window.__TAURI__?.app?.getVersion?.()) || null;
  } catch (e) {
    return null;
  }
}

function compareVersions(a, b) {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    if ((pa[i] || 0) > (pb[i] || 0)) return 1;
    if ((pa[i] || 0) < (pb[i] || 0)) return -1;
  }
  return 0;
}

async function checkForUpdates() {
  try {
    if (!window.__TAURI__) return;
    // Worktree test bundles always trail the newest release; nagging them to
    // "update" would replace the build under test. Dev paths skip the check.
    try {
      const res = await window.__TAURI__.path.resourceDir();
      if (/worktrees|target[\/\\]release/.test(String(res))) return;
    } catch (_) { /* path API missing: fall through to the normal check */ }
    // Without a version to compare against there is no honest answer, so say
    // nothing rather than offer an update we cannot justify.
    const current = await runningVersion();
    if (!current) return;
    const { check } = window.__TAURI__['updater'] || {};
    if (!check) {
      console.log('Updater plugin not available, checking GitHub API...');
      const resp = await fetch('https://api.github.com/repos/48Nauts-Operator/xNaut/releases/latest');
      if (!resp.ok) return;
      const release = await resp.json();
      const latestVersion = release.tag_name?.replace('v', '');
      if (latestVersion && compareVersions(latestVersion, current) > 0) {
        showUpdateBanner(latestVersion, release.html_url);
      }
      return;
    }
    const update = await check();
    if (update?.available) {
      const remoteVer = update.version?.replace('v', '');
      if (remoteVer && compareVersions(remoteVer, current) <= 0) {
        console.log('Update check: already on latest version', current);
        return;
      }
      showUpdateBanner(update.version, null, update);
    }
  } catch (e) {
    // Not cosmetic: debug.log carries six of these from the field, all
    // "error sending request for url (...latest.json)". A check that fails is a
    // banner that never appears, so the line has to be greppable.
    console.warn('[updater] check failed:', e);
  }
}

// ---- The download half (XNAUT-70) ----
//
// This used to be a 60s Promise.race against downloadAndInstall(), and that
// race is why the bug survived every release: when the timer won, the real
// rejection LOST the race and became a promise nobody read, so the banner said
// "Failed - download manually" whatever had actually happened. The evidence the
// ticket asked for was being destroyed by the code meant to report it.
//
// Three defects were hiding behind it, each of which alone produces the
// reported "sticks on Downloading...":
//
//   1. No timeout ever reached Rust. downloadAndInstall() was called with no
//      options, so tauri-plugin-updater built its reqwest client with no
//      timeout at all - updater.rs:559 applies one only `if let Some(timeout)`.
//      A stalled connect or hung TLS handshake therefore hangs FOREVER.
//   2. No progress was subscribed. The plugin emits Started/Progress/Finished
//      over a channel; passing no onEvent left the UI unable to tell "never
//      issued a request" from "downloading 26MB slowly" - which is exactly the
//      question the ticket lists as STILL UNKNOWN, and it can only be answered
//      on the machine that has the problem.
//   3. 60s is shorter than the download. The macOS payload is ~26.5MB, so any
//      link under ~4Mbit reported failure on a download that was going fine.
//
// And success was a dead end too: the plugin's macOS install path never
// relaunches (there is no restart anywhere in updater.rs for macos), so
// "Tauri will restart automatically" was simply untrue - the app updated on
// disk and the button sat on "Downloading..." forever.
//
// The rule now: never invent a verdict. We describe what the download is doing,
// we let the real error be the error, and we say which phase it died in.

// Total request budget handed to reqwest. Long enough for a 26MB payload on a
// slow link, short enough that a wedged socket eventually errors instead of
// hanging until the app is quit.
const UPDATE_REQUEST_TIMEOUT_MS = 15 * 60 * 1000;
// No bytes for this long is REPORTED as stalled - not cancelled. The download
// keeps running and its real outcome is still logged when it arrives.
const UPDATE_STALL_MS = 90 * 1000;

// A seam, like tests/static-server.mjs's __xnautStub: a test cannot wait 90s.
function updateStallMs() {
  return Number(window.xnautUpdateStallMs) || UPDATE_STALL_MS;
}

function formatMb(bytes) {
  return (bytes / 1048576).toFixed(1) + ' MB';
}

// Drives one download+install attempt and narrates it honestly.
// `ui` is { button, text, version, openManually }.
async function downloadUpdate(updateObj, ui) {
  const state = {
    started: false,      // a Started event arrived: the request was answered
    downloaded: false,   // a Finished event arrived: we are installing now
    received: 0,
    total: null,
    lastProgressAt: Date.now(),
    stalledShown: false,
  };

  const say = (msg) => { ui.button.textContent = msg; };
  const offerManual = (msg) => {
    say(msg);
    ui.button.disabled = false;
    ui.button.onclick = ui.openManually;
  };

  say('Connecting...');
  ui.button.disabled = true;

  const watchdog = setInterval(() => {
    if (state.downloaded || Date.now() - state.lastProgressAt < updateStallMs()) return;
    if (state.stalledShown) return;
    state.stalledShown = true;
    // "no response" and "stalled" are different failures and used to look
    // identical. The first says the request never got off the ground; the
    // second says bytes started and stopped.
    console.warn(`[updater] no progress for ${updateStallMs()}ms after ${state.received} bytes (started=${state.started})`);
    offerManual(state.started ? 'Stalled — download manually' : 'No response — download manually');
  }, Math.max(200, Math.min(1000, updateStallMs())));

  const onEvent = (e) => {
    const kind = e && e.event;
    if (kind === 'Started') {
      state.started = true;
      const len = e.data && e.data.contentLength;
      state.total = typeof len === 'number' ? len : null;
    } else if (kind === 'Progress') {
      state.started = true;
      state.received += (e.data && e.data.chunkLength) || 0;
    } else if (kind === 'Finished') {
      state.downloaded = true;
      state.lastProgressAt = Date.now();
      say('Installing...');
      return;
    } else {
      return;
    }
    state.lastProgressAt = Date.now();
    if (state.stalledShown) {
      // Bytes resumed after we said it had stopped. Take the claim back.
      state.stalledShown = false;
      ui.button.disabled = true;
      ui.button.onclick = null;
    }
    say(state.total
      ? `Downloading ${formatMb(state.received)} / ${formatMb(state.total)}`
      : `Downloading ${formatMb(state.received)}`);
  };

  try {
    await updateObj.downloadAndInstall(onEvent, { timeout: UPDATE_REQUEST_TIMEOUT_MS });
    clearInterval(watchdog);
    updateInstalled(ui);
  } catch (e) {
    clearInterval(watchdog);
    // Which phase died is the whole diagnosis, and it was unknowable before.
    const phase = state.downloaded ? 'install' : state.started ? 'download' : 'request';
    const why = (e && e.message) || String(e);
    console.error(`[updater] ${phase} failed after ${state.received} bytes:`, e);
    ui.text.textContent = `xNAUT v${ui.version} ${phase} failed: ${why}`;
    offerManual(`${phase} failed — download manually`);
  }
}

// downloadAndInstall() resolved: the new app is on disk and we are still the
// old process. Say so, and restart if the process plugin is there to do it.
function updateInstalled(ui) {
  const relaunch = window.__TAURI__ && window.__TAURI__.process && window.__TAURI__.process.relaunch;
  ui.text.textContent = `xNAUT v${ui.version} is installed.`;
  if (relaunch) {
    ui.button.textContent = 'Restart now';
    ui.button.disabled = false;
    ui.button.onclick = () => {
      ui.button.textContent = 'Restarting...';
      ui.button.disabled = true;
      window.__TAURI__.process.relaunch().catch((e) => {
        console.error('[updater] relaunch failed:', e);
        ui.button.textContent = 'Quit and reopen to finish';
      });
    };
  } else {
    ui.button.textContent = 'Quit and reopen to finish';
    ui.button.disabled = true;
  }
}

function showUpdateBanner(version, downloadUrl, updateObj) {
  const existing = document.getElementById('update-banner');
  if (existing) existing.remove();

  const banner = document.createElement('div');
  banner.id = 'update-banner';
  // In the flow above the top bar, not fixed over it. Fixed with no layout
  // offset made every top-bar control unclickable for as long as the banner
  // was up, and the only way out was the small dismiss button.
  banner.style.cssText = 'flex:0 0 auto; background:linear-gradient(90deg, #3b82f6, #6366f1); color:white; padding:8px 16px; display:flex; justify-content:center; align-items:center; gap:12px; font-size:13px; font-weight:500;';

  const text = document.createElement('span');
  text.textContent = 'xNAUT v' + version + ' is available!';

  const updateBtn = document.createElement('button');
  updateBtn.textContent = 'Update Now';
  updateBtn.style.cssText = 'background:white; color:#3b82f6; border:none; padding:4px 16px; border-radius:4px; font-size:12px; font-weight:600; cursor:pointer;';
  const manualUrl = downloadUrl || 'https://github.com/48Nauts-Operator/xNaut/releases/latest';
  const openManually = () => {
    if (window.__TAURI__?.shell?.open) window.__TAURI__.shell.open(manualUrl);
    else window.open(manualUrl, '_blank');
  };

  updateBtn.onclick = async () => {
    // No in-app updater object (the GitHub-API fallback path) means there is
    // nothing to download in-app; send them to the release page.
    if (!updateObj || !updateObj.downloadAndInstall) { openManually(); return; }
    await downloadUpdate(updateObj, { button: updateBtn, text, version, openManually });
  };

  const dismiss = document.createElement('button');
  dismiss.textContent = '×';
  dismiss.style.cssText = 'background:none; border:none; color:white; cursor:pointer; font-size:18px; margin-left:8px;';
  dismiss.onclick = () => banner.remove();

  banner.appendChild(text);
  banner.appendChild(updateBtn);
  banner.appendChild(dismiss);
  (document.getElementById('app') || document.body).prepend(banner);
}

// ==================== Theme Import ====================
window.importTheme = function() {
  const text = document.getElementById('theme-import-text')?.value?.trim();
  if (!text) { alert('Paste or load a theme first'); return; }

  let theme;
  try {
    // Try JSON first
    if (text.startsWith('{')) {
      theme = parseJsonTheme(JSON.parse(text));
    } else {
      // Try Warp YAML (simple parser for key: value format)
      theme = parseWarpYaml(text);
    }
  } catch (e) {
    alert('Failed to parse theme: ' + e.message);
    return;
  }

  if (!theme || !theme.bg || !theme.fg) {
    alert('Theme must have at least bg and fg colors');
    return;
  }

  // Add to presets
  const name = theme.name || 'Imported Theme';
  THEME_PRESETS[name] = theme;
  settings.activeTheme = name;

  // Save custom themes to localStorage
  const customThemes = JSON.parse(localStorage.getItem('xnaut-custom-themes') || '{}');
  customThemes[name] = theme;
  localStorage.setItem('xnaut-custom-themes', JSON.stringify(customThemes));

  applyThemeFromSettings(name);
  alert('Theme "' + name + '" imported!');
};

function parseJsonTheme(json) {
  // Support multiple JSON formats: our own, Windows Terminal, iTerm2-like
  if (json.terminal_colors) {
    // Warp JSON format
    return {
      name: json.name || 'Imported',
      bg: json.background, fg: json.foreground, cursor: json.cursor || json.foreground,
      chrome: json.background, selection: 'rgba(255,255,255,0.2)',
      black: json.terminal_colors?.normal?.black, red: json.terminal_colors?.normal?.red,
      green: json.terminal_colors?.normal?.green, yellow: json.terminal_colors?.normal?.yellow,
      blue: json.terminal_colors?.normal?.blue, magenta: json.terminal_colors?.normal?.magenta,
      cyan: json.terminal_colors?.normal?.cyan, white: json.terminal_colors?.normal?.white,
      brightBlack: json.terminal_colors?.bright?.black, brightRed: json.terminal_colors?.bright?.red,
      brightGreen: json.terminal_colors?.bright?.green, brightYellow: json.terminal_colors?.bright?.yellow,
      brightBlue: json.terminal_colors?.bright?.blue, brightMagenta: json.terminal_colors?.bright?.magenta,
      brightCyan: json.terminal_colors?.bright?.cyan, brightWhite: json.terminal_colors?.bright?.white,
    };
  }
  // Our own format or Windows Terminal format
  return {
    name: json.name || 'Imported',
    bg: json.bg || json.background, fg: json.fg || json.foreground,
    cursor: json.cursor || json.cursorColor || json.fg || json.foreground,
    chrome: json.chrome || json.bg || json.background,
    selection: json.selection || json.selectionBackground || 'rgba(255,255,255,0.2)',
    black: json.black, red: json.red, green: json.green, yellow: json.yellow,
    blue: json.blue, magenta: json.magenta || json.purple, cyan: json.cyan, white: json.white,
    brightBlack: json.brightBlack, brightRed: json.brightRed, brightGreen: json.brightGreen,
    brightYellow: json.brightYellow, brightBlue: json.brightBlue,
    brightMagenta: json.brightMagenta || json.brightPurple, brightCyan: json.brightCyan,
    brightWhite: json.brightWhite,
  };
}

function parseWarpYaml(yaml) {
  // Simple YAML parser for Warp theme files
  const lines = yaml.split('\n');
  const flat = {};
  let section = '';
  let subsection = '';
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const indent = line.length - line.trimStart().length;
    const [key, ...valParts] = trimmed.split(':');
    const val = valParts.join(':').trim().replace(/['"]/g, '');
    if (!val) {
      if (indent === 0) section = key.trim();
      else if (indent <= 4) subsection = key.trim();
      continue;
    }
    const fullKey = section ? (subsection && indent > 4 ? section + '.' + subsection + '.' + key.trim() : section + '.' + key.trim()) : key.trim();
    flat[fullKey] = val;
  }

  return {
    name: flat['name'] || 'Imported Warp Theme',
    bg: flat['background'], fg: flat['foreground'],
    cursor: flat['cursor'] || flat['foreground'],
    chrome: flat['background'], selection: 'rgba(255,255,255,0.2)',
    black: flat['terminal_colors.normal.black'], red: flat['terminal_colors.normal.red'],
    green: flat['terminal_colors.normal.green'], yellow: flat['terminal_colors.normal.yellow'],
    blue: flat['terminal_colors.normal.blue'], magenta: flat['terminal_colors.normal.magenta'],
    cyan: flat['terminal_colors.normal.cyan'], white: flat['terminal_colors.normal.white'],
    brightBlack: flat['terminal_colors.bright.black'], brightRed: flat['terminal_colors.bright.red'],
    brightGreen: flat['terminal_colors.bright.green'], brightYellow: flat['terminal_colors.bright.yellow'],
    brightBlue: flat['terminal_colors.bright.blue'], brightMagenta: flat['terminal_colors.bright.magenta'],
    brightCyan: flat['terminal_colors.bright.cyan'], brightWhite: flat['terminal_colors.bright.white'],
  };
}

// Custom themes loaded after THEME_PRESETS definition (see below)


// ==================== Work Session Logger ====================
let worklogActive = false;

async function toggleWorkLog() {
  if (worklogActive) {
    // Stop logging
    try {
      const session = await invoke('worklog_stop');
      worklogActive = false;
      updateWorkLogUI();

      // Show summary in editor panel. Both reads name the session that just
      // stopped: worklog_stop clears the active one, so an unqualified read
      // errors here every time and the panel fell back to a one-line stub.
      const summary = await invoke('worklog_summary', { sessionId: session.id })
        .catch(() => 'Session logged with ' + session.entries.length + ' commands.');
      const qrSvg = await invoke('worklog_qr', { sessionId: session.id }).catch(() => '');

      const panel = document.getElementById('editor-panel');
      const preview = document.getElementById('editor-preview');
      const filename = document.getElementById('editor-filename');
      const highlighted = document.getElementById('editor-highlighted');
      const textarea = document.getElementById('editor-textarea');
      const lineNumbers = document.getElementById('editor-line-numbers');

      if (panel && preview) {
        filename.textContent = 'Work Session Summary';
        if (textarea) textarea.style.display = 'none';
        if (highlighted) highlighted.style.display = 'none';
        if (lineNumbers) lineNumbers.style.display = 'none';
        preview.style.display = 'block';

        // generate_summary is a Rust METHOD, never a serialised field, so the
        // old `session.generate_summary` here was always undefined and the
        // fallback one-liner always won. The command returns the real thing.
        let html = '';
        if (typeof marked !== 'undefined') {
          html = marked.parse(summary);
        }
        // The QR encodes the session and its merkle root, so it is only offered
        // when one was actually rendered; a heading over nothing claimed a proof
        // that was not on the page.
        if (qrSvg) {
          html += '<div style="margin-top:20px; text-align:center;"><h3>Verification QR Code</h3><p style="font-size:11px; color:var(--text-secondary);">Scan to verify this work session is authentic</p>';
          html += '<div style="background:white; display:inline-block; padding:12px; border-radius:8px;">' + qrSvg + '</div>';
          html += '<p style="font-size:10px; color:var(--text-secondary); margin-top:8px;">Merkle Root: <code>' + (session.merkle_root || 'N/A').substring(0, 16) + '...</code></p></div>';
        }
        preview.innerHTML = html;
        panel.style.display = 'flex';
        requestAnimationFrame(() => resizeAllTerminals());
      }

      // Add report action buttons
      const btnRow = document.createElement('div');
      btnRow.style.cssText = 'display:flex; gap:8px; justify-content:center; margin:16px 0;';

      const saveReportBtn = document.createElement('button');
      saveReportBtn.textContent = 'Open Report in Browser';
      saveReportBtn.className = 'btn btn-primary';
      saveReportBtn.addEventListener('click', async () => {
        try {
          const reportPath = await invoke('worklog_save_report');
          // Use terminal to open the file in default browser
          const tab = tabs.find(t => t.id === activeTabId);
          if (tab && tab.terminals.length) {
            const terminal = tab.terminals[tab.focusedPaneIndex || 0];
            if (terminal) {
              await invoke('write_to_terminal', { sessionId: terminal.sessionId, data: 'open "' + reportPath + '"\n' });
            }
          }
        } catch (e) {
          alert('Failed to save report: ' + e);
        }
      });

      const copyMdBtn = document.createElement('button');
      copyMdBtn.textContent = 'Copy as Markdown';
      copyMdBtn.className = 'btn';
      copyMdBtn.addEventListener('click', async () => {
        try {
          const summary = session.entries.map((e, i) =>
            (i+1) + '. `' + e.timestamp.substring(11,19) + '` — `' + e.command + '` (' + e.directory + ')'
          ).join('\n');
          const md = '# Work Session: ' + session.project + ' — ' + session.client + '\n\n' +
            '**Date:** ' + session.started.substring(0,10) + '\n' +
            '**Commands:** ' + session.entries.length + '\n' +
            '**Merkle Root:** `' + (session.merkle_root || 'N/A').substring(0,16) + '`\n\n' +
            '## Commands\n\n' + summary;
          await navigator.clipboard.writeText(md);
          copyMdBtn.textContent = 'Copied!';
          setTimeout(() => { copyMdBtn.textContent = 'Copy as Markdown'; }, 2000);
        } catch (e) {
          alert('Failed: ' + e);
        }
      });

      btnRow.appendChild(saveReportBtn);
      btnRow.appendChild(copyMdBtn);
      preview.appendChild(btnRow);
    } catch (e) {
      alert('Failed to stop work log: ' + e);
    }
  } else {
    // Start logging with defaults (prompt may not work in Tauri WebView)
    let client = 'Personal';
    let project = 'General';
    try {
      client = await window.xnautPromptDialog('Client name:', 'Personal', 'Start') || 'Personal';
      project = await window.xnautPromptDialog('Project name:', 'General', 'Start') || 'General';
    } catch (e) {
      // prompt not available in this WebView
    }

    try {
      const session = await invoke('worklog_start', { client, project });
      worklogActive = true;
      updateWorkLogUI();
      console.log('Work log started:', session.id);
    } catch (e) {
      console.error('Failed to start work log:', e);
      // Attached to the clock rather than raised as a toast: the failure is a
      // property of that control, and it stays readable after a toast expires.
      const clock = document.getElementById('btn-worklog-clock');
      if (clock) clock.title = 'Work log failed to start: ' + e;
    }
  }
}

function updateWorkLogUI() {
  const menuItem = document.getElementById('menu-worklog');
  if (menuItem) {
    menuItem.textContent = worklogActive ? '⏹ Stop Work Log' : 'Start Work Log';
    menuItem.style.color = worklogActive ? '#ef4444' : 'var(--text-primary)';
  }

  // Topbar clock: red while recording, so the state is visible without opening
  // the menu. Clicking it again stops and opens the summary.
  const clock = document.getElementById('btn-worklog-clock');
  if (clock) {
    clock.style.color = worklogActive ? '#ef4444' : '';
    clock.title = worklogActive ? 'Stop Work Log — opens the summary' : 'Start Work Log';
    clock.setAttribute('aria-pressed', worklogActive ? 'true' : 'false');
    clock.setAttribute('aria-label', worklogActive ? 'Stop work log' : 'Start work log');
  }

  // Add/remove recording indicator in status bar
  const statusBar = document.getElementById('shared-status-bar');
  let indicator = document.getElementById('worklog-indicator');
  if (worklogActive && statusBar) {
    if (!indicator) {
      indicator = document.createElement('div');
      indicator.id = 'worklog-indicator';
      indicator.style.cssText = 'display:flex; align-items:center; gap:4px; padding:2px 8px; background:rgba(239,68,68,0.15); border-radius:3px; font-size:11px; color:#ef4444;';
      indicator.innerHTML = '<span style="width:6px; height:6px; border-radius:50%; background:#ef4444; animation:pulse 1.5s infinite;"></span> Recording';
      statusBar.querySelector('.status-bar-left')?.appendChild(indicator);
    }
  } else if (indicator) {
    indicator.remove();
  }
}

// A report over the last week, built from what the agents actually did rather
// than from what anyone typed (XNAUT-267). No session required: the sources it
// reads have been accumulating on disk for weeks, so this works the first time
// it is ever clicked.
async function worklogRangeReport(days) {
  const to = new Date();
  const from = new Date(to.getTime() - days * 24 * 60 * 60 * 1000);
  try {
    const path = await invoke('worklog_report_range', { from: from.toISOString(), to: to.toISOString() });
    const tab = tabs.find(t => t.id === activeTabId);
    const terminal = tab?.terminals?.[tab.focusedPaneIndex || 0];
    if (terminal) {
      await invoke('write_to_terminal', { sessionId: terminal.sessionId, data: 'open "' + path + '"\n' });
    } else {
      alert('Report saved to ' + path);
    }
  } catch (e) {
    alert('Failed to build the report: ' + e);
  }
}

// Auto-log commands when worklog is active
async function worklogAutoLog(command, directory) {
  if (!worklogActive) return;
  try {
    await invoke('worklog_log', { command, directory, outputSummary: null });
  } catch (e) {
    console.error('Worklog auto-log failed:', e);
  }
}

// Check for active worklog on startup
// ==================== ClawProxy Privacy Monitor ====================
let clawproxyRunning = false;

async function checkClawProxy() {
  try {
    const result = await invoke('check_clawproxy');
    clawproxyRunning = result.available;
    updatePrivacyIndicator(result.available ? result.stats : null);
  } catch (e) {
    clawproxyRunning = false;
  }
}

async function startClawProxy() {
  try {
    await invoke('start_clawproxy');
    // Wait a moment for it to start
    setTimeout(() => checkClawProxy(), 2000);
  } catch (e) {
    alert('Failed to start ClawProxy: ' + e);
  }
}

function updatePrivacyIndicator(stats) {
  const statusBar = document.getElementById('shared-status-bar');
  if (!statusBar) return;

  let indicator = document.getElementById('privacy-indicator');
  if (!clawproxyRunning) {
    if (indicator) indicator.remove();
    return;
  }

  if (!indicator) {
    indicator = document.createElement('div');
    indicator.id = 'privacy-indicator';
    indicator.style.cssText = 'display:flex; align-items:center; gap:4px; padding:2px 8px; border-radius:3px; font-size:11px; cursor:pointer;';
    indicator.title = 'Privacy Monitor (ClawProxy)';
    indicator.onclick = showPrivacyPanel;
    statusBar.querySelector('.status-bar-left')?.appendChild(indicator);
  }

  const criticals = stats?.findings_critical || 0;
  const warnings = stats?.findings_warning || 0;

  if (criticals > 0) {
    indicator.style.background = 'rgba(239,68,68,0.15)';
    indicator.style.color = '#ef4444';
    indicator.innerHTML = '🔴 ' + criticals + ' critical';
  } else if (warnings > 0) {
    indicator.style.background = 'rgba(245,158,11,0.15)';
    indicator.style.color = '#f59e0b';
    indicator.innerHTML = '🟡 ' + warnings + ' warnings';
  } else {
    indicator.style.background = 'rgba(16,185,129,0.15)';
    indicator.style.color = '#10b981';
    indicator.innerHTML = '🟢 Clean';
  }
}

async function showPrivacyPanel() {
  try {
    const [stats, alerts] = await Promise.all([
      invoke('get_privacy_stats'),
      invoke('get_privacy_alerts')
    ]);

    const panel = document.getElementById('editor-panel');
    const preview = document.getElementById('editor-preview');
    const filename = document.getElementById('editor-filename');
    const highlighted = document.getElementById('editor-highlighted');
    const textarea = document.getElementById('editor-textarea');
    const lineNumbers = document.getElementById('editor-line-numbers');

    if (!panel || !preview) return;

    filename.textContent = 'Privacy Monitor';
    if (textarea) textarea.style.display = 'none';
    if (highlighted) highlighted.style.display = 'none';
    if (lineNumbers) lineNumbers.style.display = 'none';
    preview.style.display = 'block';

    const totalCalls = stats.total_requests || 0;
    const totalCost = (stats.total_cost || 0).toFixed(4);
    const alertItems = Array.isArray(alerts) ? alerts : (alerts.alerts || []);

    let html = '<h2>Privacy Assessment</h2>';
    html += '<div style="display:flex; gap:16px; margin:12px 0;">';
    html += '<div style="padding:12px; background:var(--bg-secondary); border-radius:6px; flex:1; text-align:center;"><div style="font-size:11px; color:var(--text-secondary);">API Calls</div><div style="font-size:20px; font-weight:600;">' + totalCalls + '</div></div>';
    html += '<div style="padding:12px; background:var(--bg-secondary); border-radius:6px; flex:1; text-align:center;"><div style="font-size:11px; color:var(--text-secondary);">Est. Cost</div><div style="font-size:20px; font-weight:600;">$' + totalCost + '</div></div>';
    html += '<div style="padding:12px; background:var(--bg-secondary); border-radius:6px; flex:1; text-align:center;"><div style="font-size:11px; color:var(--text-secondary);">Alerts</div><div style="font-size:20px; font-weight:600; color:' + (alertItems.length > 0 ? '#f59e0b' : '#10b981') + ';">' + alertItems.length + '</div></div>';
    html += '</div>';

    if (alertItems.length > 0) {
      html += '<h3>Alerts</h3>';
      alertItems.slice(0, 20).forEach(function(alert) {
        const sev = (alert.severity || 'warning').toLowerCase();
        const color = sev === 'critical' ? '#ef4444' : '#f59e0b';
        html += '<div style="padding:8px; margin:4px 0; border-left:3px solid ' + color + '; background:var(--bg-secondary); border-radius:0 4px 4px 0; font-size:12px;">';
        html += '<strong style="color:' + color + ';">' + (alert.severity || 'WARNING') + '</strong> — ' + (alert.pattern || alert.type || 'Unknown') + '<br>';
        html += '<code style="font-size:11px; color:var(--text-secondary);">' + (alert.redacted || alert.match || '') + '</code>';
        html += '</div>';
      });
    } else {
      html += '<p style="color:#10b981; text-align:center; padding:20px;">✓ No privacy concerns detected</p>';
    }

    preview.innerHTML = html;
    panel.style.display = 'flex';
    requestAnimationFrame(function() { resizeAllTerminals(); });
  } catch (e) {
    alert('ClawProxy not running. Start it from Settings > AI.');
  }
}

// Poll ClawProxy status every 10 seconds
setInterval(function() { if (clawproxyRunning) checkClawProxy(); }, 10000);

// ==================== Drag & Drop Attachments ====================
// Quote a path so spaces/special chars don't break shell parsing.
function shellQuotePath(p) {
  if (!/[ \t"'$`\\!()*?\[\]{}|;<>&#]/.test(p)) return p;
  return "'" + p.replace(/'/g, "'\\''") + "'";
}

// Send text to the currently focused PTY session
async function sendToFocusedTerminal(text) {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || !tab.terminals.length) return false;
  const terminal = tab.terminals[tab.focusedPaneIndex || 0];
  if (!terminal || !terminal.sessionId) return false;
  try {
    await invoke('write_to_terminal', { sessionId: terminal.sessionId, data: text });
    return true;
  } catch (e) {
    console.error('Failed to write attachment path to terminal:', e);
    return false;
  }
}

function showDropOverlay() {
  let overlay = document.getElementById('xnaut-drop-overlay');
  if (overlay) return;
  overlay = document.createElement('div');
  overlay.id = 'xnaut-drop-overlay';
  overlay.style.cssText = `
    position: fixed; inset: 0; z-index: 99999; pointer-events: none;
    background: rgba(59, 130, 246, 0.12);
    border: 3px dashed #3B82F6; box-sizing: border-box;
    display: flex; align-items: center; justify-content: center;
    color: #E8E8ED; font-family: 'Inter', system-ui, sans-serif;
    font-size: 18px; font-weight: 600;
    text-shadow: 0 1px 2px rgba(0,0,0,0.6);
  `;
  overlay.textContent = 'Drop file to attach path to terminal';
  document.body.appendChild(overlay);
}

function hideDropOverlay() {
  const overlay = document.getElementById('xnaut-drop-overlay');
  if (overlay) overlay.remove();
}

// Mobile bridge (XNAUT-32): sessions created from the phone become desktop
// tabs, mirroring the agent-launcher adoption path.
function setupMobileBridgeListener() {
  if (!window.__TAURI__ || !window.__TAURI__.event) return;
  const { listen } = window.__TAURI__.event;
  listen('mobile-session-created', (event) => {
    const sessionId = event && event.payload && event.payload.sessionId;
    if (sessionId) window.xnautAttachAgentTab(sessionId, 'Mobile');
  }).catch((e) => console.warn('mobile-session-created listener failed:', e));
}

function setupTerminalDragDrop() {
  if (!window.__TAURI__ || !window.__TAURI__.event) return;
  const { listen } = window.__TAURI__.event;

  // Tauri 2 emits OS-level drag events with absolute file paths.
  // We use those rather than HTML5 because the WebView intercepts drops at OS level.
  listen('tauri://drag-enter', () => showDropOverlay()).catch(() => {});
  listen('tauri://drag-over', () => showDropOverlay()).catch(() => {});
  listen('tauri://drag-leave', () => hideDropOverlay()).catch(() => {});

  listen('tauri://drag-drop', async (event) => {
    hideDropOverlay();
    const paths = event && event.payload && event.payload.paths;
    if (!Array.isArray(paths) || paths.length === 0) return;
    // Inject each path quoted, separated by a space, then a trailing space.
    const text = paths.map(shellQuotePath).join(' ') + ' ';
    await sendToFocusedTerminal(text);
  }).catch((e) => {
    console.warn('drag-drop listener failed:', e);
  });

  console.log('🖱️ Drag-drop attachments enabled');
}

// ==================== AI Explainer Panel (#53) ====================
async function explainScreen() {
  // Grab recent terminal output, strip ANSI codes
  const rawOutput = terminalOutputBuffer.slice(-3000);
  const cleanOutput = rawOutput.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '').replace(/\x1b\][^\x07]*\x07/g, '').trim();

  if (!cleanOutput) {
    alert('No terminal output to explain. Run some commands first.');
    return;
  }

  const panel = document.getElementById('editor-panel');
  const preview = document.getElementById('editor-preview');
  const filename = document.getElementById('editor-filename');
  const highlighted = document.getElementById('editor-highlighted');
  const textarea = document.getElementById('editor-textarea');
  const lineNumbers = document.getElementById('editor-line-numbers');

  if (!panel || !preview) return;

  filename.textContent = 'Explain Screen';
  if (textarea) textarea.style.display = 'none';
  if (highlighted) highlighted.style.display = 'none';
  if (lineNumbers) lineNumbers.style.display = 'none';
  preview.style.display = 'block';
  preview.innerHTML = '<p style="color:var(--text-secondary);">Analyzing terminal output...</p>';
  panel.style.display = 'flex';
  requestAnimationFrame(function() { resizeAllTerminals(); });

  const prompt = 'Explain what is happening in this terminal session. What commands were run? What do the outputs mean? Are there any errors or warnings? Be concise and clear.\n\nTerminal output:\n' + cleanOutput.slice(-2000);

  try {
    const response = await callAI(prompt);
    if (!response || response.trim() === '') throw new Error('Empty response. Check AI provider in Settings.');
    if (typeof marked !== 'undefined') {
      preview.innerHTML = marked.parse(response);
    } else {
      preview.textContent = response;
    }
  } catch (e) {
    const errMsg = e.message || e;
    // Name the endpoint actually dialled: "connection refused" without it sent
    // André chasing a port that was never in his settings.
    const endpoint = aiSettingsChatEndpoint(settings.llmProvider || '') || 'not configured';
    preview.innerHTML = '<p style="color:#ef4444;">Failed: ' + errMsg + '</p><p style="color:var(--text-secondary); font-size:12px; margin-top:8px;">Provider: ' + (settings.llmProvider || 'none') + '<br>Model: ' + (settings.llmModel || 'none') + '<br>Endpoint: ' + endpoint + '<br><br>Check the provider and endpoint under Settings &gt; AI.</p>';
  }
}

// ==================== AI Theme Generator (#46) ====================
window.generateAITheme = async function() {
  const description = document.getElementById('ai-theme-prompt')?.value?.trim();
  if (!description) { alert('Describe the theme you want'); return; }

  const btn = document.getElementById('btn-generate-theme');
  if (btn) { btn.textContent = 'Generating...'; btn.disabled = true; }

  const prompt = 'Create a terminal color theme called "' + description + '". Return ONLY JSON, nothing else: {"name":"...","bg":"#hex","fg":"#hex","cursor":"#hex","chrome":"#hex","black":"#hex","red":"#hex","green":"#hex","yellow":"#hex","blue":"#hex","magenta":"#hex","cyan":"#hex","white":"#hex","brightBlack":"#hex","brightRed":"#hex","brightGreen":"#hex","brightYellow":"#hex","brightBlue":"#hex","brightMagenta":"#hex","brightCyan":"#hex","brightWhite":"#hex"}';

  try {
    if (btn) btn.textContent = 'Asking AI...';

    // Race between AI call and 30s timeout
    let response;
    const timeoutPromise = new Promise(function(_, reject) {
      setTimeout(function() { reject(new Error('Timeout — AI took too long (30s)')); }, 30000);
    });

    response = await Promise.race([callAI(prompt), timeoutPromise]);
    if (!response) throw new Error('Empty response from AI');
    if (btn) btn.textContent = 'Parsing theme...';

    // Clean response: remove line wrapping, emojis, prefixes, code fences
    let cleaned = response
      .replace(/```(?:json)?\s*/g, '').replace(/```/g, '')
      .replace(/\n/g, '')
      .replace(/\r/g, '')
      .trim();

    // Find JSON object
    const jsonMatch = cleaned.match(/\{[^{}]*"bg"[^{}]*\}/);
    if (!jsonMatch) {
      const anyJson = cleaned.match(/\{.*\}/);
      if (!anyJson) {
        alert('No theme JSON found in response:\n\n' + response.slice(0, 300));
        if (btn) { btn.textContent = 'Generate'; btn.disabled = false; }
        return;
      }
      cleaned = anyJson[0];
    } else {
      cleaned = jsonMatch[0];
    }

    let theme;
    try {
      theme = JSON.parse(cleaned);
    } catch (parseErr) {
      alert('JSON parse error:\n\n' + cleaned.slice(0, 300));
      if (btn) { btn.textContent = 'Generate'; btn.disabled = false; }
      return;
    }

    if (!theme.bg || !theme.fg) {
      alert('Theme missing colors. Got:\n\n' + JSON.stringify(theme).slice(0, 300));
      if (btn) { btn.textContent = 'Generate'; btn.disabled = false; }
      return;
    }
    theme.selection = 'rgba(255,255,255,0.2)';
    if (!theme.chrome) theme.chrome = theme.bg;
    if (!theme.cursor) theme.cursor = theme.fg;

    // Show preview
    showThemePreview(theme);

    const name = theme.name || description.slice(0, 25);
    theme._aiGenerated = true;
    THEME_PRESETS[name] = theme;

    const customThemes = JSON.parse(localStorage.getItem('xnaut-custom-themes') || '{}');
    customThemes[name] = theme;
    localStorage.setItem('xnaut-custom-themes', JSON.stringify(customThemes));

    applyThemeFromSettings(name);

    if (btn) { btn.textContent = 'Applied! Generate Another'; btn.disabled = false; }
  } catch (e) {
    alert('Theme generation failed: ' + (e.message || e));
    if (btn) { btn.textContent = 'Generate'; btn.disabled = false; }
  }
};

function showThemePreview(theme) {
  const strip = document.getElementById('theme-preview-strip');
  if (!strip) return;

  const r = theme.red || '#ff5555';
  const g = theme.green || '#50fa7b';
  const y = theme.yellow || '#f1fa8c';
  const b = theme.blue || '#6272a4';
  const m = theme.magenta || '#ff79c6';
  const c = theme.cyan || '#8be9fd';

  strip.innerHTML = '<div style="background:' + theme.bg + '; border-radius:8px; padding:12px; font-family:\'JetBrains Mono NF\',monospace; font-size:11px; line-height:1.6;">' +
    '<div style="display:flex; justify-content:space-between; margin-bottom:8px;">' +
      '<span style="color:' + theme.fg + '; opacity:0.5; font-size:10px;">Terminal Preview</span>' +
      '<div style="display:flex; gap:4px;">' +
        [r,g,y,b,m,c,theme.brightRed,theme.brightGreen].filter(Boolean).map(function(col) {
          return '<div style="width:10px; height:10px; border-radius:50%; background:' + col + ';"></div>';
        }).join('') +
      '</div>' +
    '</div>' +
    '<div><span style="color:' + g + ';">➜</span> <span style="color:' + c + ';">~/projects</span> <span style="color:' + theme.fg + ';">git status</span></div>' +
    '<div style="color:' + theme.fg + ';">On branch <span style="color:' + m + ';">main</span></div>' +
    '<div style="color:' + g + ';">  modified:   src/app.js</div>' +
    '<div style="color:' + r + ';">  deleted:    old-file.ts</div>' +
    '<div style="color:' + y + ';">  untracked:  new-feature/</div>' +
    '<div style="margin-top:4px;"><span style="color:' + g + ';">➜</span> <span style="color:' + c + ';">~/projects</span> <span style="color:' + b + ';background:' + theme.cursor + '; padding:0 2px;">█</span></div>' +
  '</div>';
  strip.style.display = 'block';
}

// Unified AI call helper — routes through Rust backend to avoid CSP issues
async function callAI(prompt) {
  const provider = settings.llmProvider || 'anthropic';
  const model = settings.llmModel || '';

  console.log('callAI: provider=' + provider + ', model=' + model);

  try {
    const apiKey = getAPIKey() || '';
    const response = await invoke('ask_ai', { prompt: prompt, context: '', provider: provider, apiKey: apiKey, model: model });
    if (!response || response.trim() === '') {
      throw new Error('Empty response from ' + provider + ' (model: ' + model + '). Check Settings > AI.');
    }
    return response;
  } catch (e) {
    const errMsg = typeof e === 'string' ? e : (e.message || JSON.stringify(e));
    throw new Error(provider + ': ' + errMsg);
  }
}

async function checkActiveWorklog() {
  try {
    const session = await invoke('worklog_status');
    if (session) {
      worklogActive = true;
      updateWorkLogUI();
      return;
    }
    await offerOrphanedWorklog();
  } catch (e) {}
}

// A work log that was running when the app was closed.
//
// The session survives on disk; only the in-memory pointer to it dies with the
// process, so before this the monitor simply stopped recording and the hours
// went missing from the PM Space dashboard. XNAUT-139.
//
// It asks rather than resuming by itself: time passed between the close and the
// relaunch that nobody worked, and where several logs were left open, adopting
// the newest silently would close whichever one was real.
async function offerOrphanedWorklog() {
  const orphans = await invoke('worklog_orphans').catch(() => []);
  if (!orphans || !orphans.length) return;

  const s = orphans[0];
  const existing = document.getElementById('worklog-resume-bar');
  if (existing) existing.remove();

  const bar = document.createElement('div');
  bar.id = 'worklog-resume-bar';
  // In the flow as the first child of #app, not fixed over it. The update
  // banner shipped fixed with no layout offset and covered every top-bar
  // control until it was dismissed; there is no reason to repeat that here.
  bar.style.cssText = 'flex:0 0 auto; background:rgba(239,68,68,0.15); border-bottom:1px solid rgba(239,68,68,0.4); color:var(--text-primary); padding:8px 16px; display:flex; align-items:center; gap:12px; font-size:13px;';

  const started = new Date(s.started);
  const mins = Math.max(0, Math.round((Date.now() - started.getTime()) / 60000));
  const ago = mins < 60 ? `${mins}m` : `${Math.floor(mins / 60)}h ${mins % 60}m`;
  const more = orphans.length > 1 ? ` (+${orphans.length - 1} more)` : '';

  const text = document.createElement('span');
  text.style.flex = '1';
  text.textContent = `Work log left running: ${s.client} / ${s.project}, started ${ago} ago, `
    + `${s.entries.length} command${s.entries.length === 1 ? '' : 's'}${more}. Continue?`;

  const cont = document.createElement('button');
  cont.textContent = 'Continue';
  cont.style.cssText = 'background:#ef4444; color:white; border:none; padding:4px 14px; border-radius:4px; font-size:12px; font-weight:600; cursor:pointer;';
  cont.onclick = async () => {
    try {
      await invoke('worklog_resume', { id: s.id });
      worklogActive = true;
      updateWorkLogUI();
      bar.remove();
      // Another may be waiting behind this one.
      await offerOrphanedWorklog();
    } catch (e) {
      text.textContent = 'Could not resume: ' + e;
    }
  };

  const close = document.createElement('button');
  close.textContent = 'Close it';
  close.style.cssText = 'background:none; color:var(--text-primary); border:1px solid var(--border); padding:4px 14px; border-radius:4px; font-size:12px; cursor:pointer;';
  close.onclick = async () => {
    try {
      await invoke('worklog_discard', { id: s.id });
      bar.remove();
      await offerOrphanedWorklog();
    } catch (e) {
      text.textContent = 'Could not close it: ' + e;
    }
  };

  bar.append(text, cont, close);
  (document.getElementById('app') || document.body).prepend(bar);
}

window.toggleSettingsPanel = function() {
  const panel = document.getElementById('settings-panel');
  if (!panel) return;
  if (panel.style.display === 'none' || !panel.style.display) {
    panel.style.display = 'flex';
    loadSettingsSection('ai');
    document.getElementById('settings-search-input')?.focus();
  } else {
    panel.style.display = 'none';
  }
}

function loadSettingsSection(section) {
  const content = document.getElementById('settings-content');
  if (!content) return;

  // Update nav active state
  document.querySelectorAll('.settings-nav-item').forEach(item => {
    item.classList.toggle('active', item.dataset.section === section);
  });

  const sections = {
    ai: () => `
      <h3>AI Providers</h3>
      <div class="settings-group">
        <h4>Local Providers</h4>
        <div class="settings-row">
          <label><span class="status-dot-sm gray" id="ollama-status"></span>Ollama</label>
          <input type="url" id="set-ollama-url" value="${settings.ollamaUrl || 'http://localhost:11434'}" placeholder="http://localhost:11434">
          <button class="btn-test" data-test-provider="ollama">Test</button>
        </div>
        <div class="settings-row">
          <label><span class="status-dot-sm gray" id="lmstudio-status"></span>LM Studio</label>
          <input type="url" id="set-lmstudio-url" value="${settings.lmstudioUrl || 'http://localhost:1234'}" placeholder="http://localhost:1234">
          <button class="btn-test" data-test-provider="lmstudio">Test</button>
        </div>
        <div class="settings-row">
          <label>Agent harness</label>
          <input type="checkbox" id="set-harness-local" style="width:auto; flex:none;" ${settings.harnessLocal ? 'checked' : ''}>
        </div>
        <p style="color:var(--text-secondary); font-size:12px; margin:4px 0 0;">Run the coding agents against the local provider above instead of their own cloud — for working without a subscription. Codex and pi speak OpenAI and work today; Claude Code needs the Anthropic translation shim (XNAUT-72), so it keeps using your subscription until that lands.</p>
      </div>
      <div class="settings-group">
        <h4>Cloud Providers</h4>
        <div class="settings-row">
          <label>Anthropic</label>
          <input type="password" id="set-api-anthropic" value="${settings.apiKeyAnthropic || ''}" placeholder="sk-ant-...">
        </div>
        <div class="settings-row">
          <label>OpenAI</label>
          <input type="password" id="set-api-openai" value="${settings.apiKeyOpenAI || ''}" placeholder="sk-...">
        </div>
        <div class="settings-row">
          <label>OpenRouter</label>
          <input type="password" id="set-api-openrouter" value="${settings.apiKeyOpenRouter || ''}" placeholder="sk-or-...">
        </div>
        <div class="settings-row">
          <label>Perplexity</label>
          <input type="password" id="set-api-perplexity" value="${settings.apiKeyPerplexity || ''}" placeholder="pplx-...">
        </div>
        <div class="settings-row">
          <label>NautGate URL</label>
          <input type="url" id="set-nautgate-url" value="${settings.nautgateUrl || 'http://localhost:8090/v1'}" placeholder="http://localhost:8090/v1">
        </div>
        <div class="settings-row">
          <label>NautGate Token</label>
          <input type="password" id="set-api-nautgate" value="${settings.apiKeyNautGate || ''}" placeholder="Optional — routes agents via NautGate (cloud)">
        </div>
      </div>
      <div class="settings-group">
        <h4>Default Model</h4>
        <div class="settings-row">
          <label>Provider</label>
          <select id="set-default-provider">
            <option value="ollama" ${settings.llmProvider === 'ollama' ? 'selected' : ''}>Ollama (Local)</option>
            <option value="lmstudio" ${settings.llmProvider === 'lmstudio' ? 'selected' : ''}>LM Studio (Local)</option>
            <option value="anthropic" ${settings.llmProvider === 'anthropic' ? 'selected' : ''}>Anthropic</option>
            <option value="openai" ${settings.llmProvider === 'openai' ? 'selected' : ''}>OpenAI</option>
            <option value="openrouter" ${settings.llmProvider === 'openrouter' ? 'selected' : ''}>OpenRouter</option>
            <option value="perplexity" ${settings.llmProvider === 'perplexity' ? 'selected' : ''}>Perplexity</option>
            <option value="nautgate" ${settings.llmProvider === 'nautgate' ? 'selected' : ''}>NautGate (Cloud)</option>
          </select>
        </div>
        <div class="settings-row">
          <label>Model</label>
          <select id="set-default-model"></select>
        </div>
      </div>
      <div class="settings-group">
        <h4>MCP Servers</h4>
        <div class="settings-row">
          <label><span class="status-dot-sm gray" id="excalidraw-mcp-status"></span>Excalidraw Local</label>
          <input type="checkbox" id="set-mcp-excalidraw-enabled">
          <button class="btn-test" id="btn-test-mcp-excalidraw">Test</button>
        </div>
        <div class="settings-row">
          <label>Endpoint</label>
          <input type="url" id="set-mcp-excalidraw-url" value="http://127.0.0.1:3001/mcp">
        </div>
        <div class="settings-row">
          <label>Remote Key</label>
          <input type="password" id="set-mcp-excalidraw-key" placeholder="Optional for a hosted endpoint">
        </div>
      </div>
      <div class="settings-group">
        <h4>Voice (Kokoro)</h4>
        <div class="settings-row">
          <label>Enable Voice</label>
          <input type="checkbox" id="set-voice-enabled" ${settings.voiceEnabled ? 'checked' : ''}>
        </div>
        <div class="settings-row">
          <label>Endpoint</label>
          <input type="url" id="set-kokoro-url" value="${settings.kokoroUrl || 'http://localhost:8880'}" placeholder="http://localhost:8880">
        </div>
      </div>
      <div class="settings-group">
        <h4>Privacy Monitor (ClawProxy)</h4>
        <div class="settings-row">
          <label><span class="status-dot-sm ${clawproxyRunning ? 'green' : 'gray'}" id="clawproxy-status"></span>ClawProxy</label>
          <span style="color:var(--text-secondary); font-size:12px;">${clawproxyRunning ? 'Running on :8099' : 'Not running'}</span>
          <button class="btn-test" id="btn-start-clawproxy">${clawproxyRunning ? 'Running' : 'Start'}</button>
        </div>
        <p style="color:var(--text-secondary); font-size:11px; margin-top:4px;">Routes AI traffic through privacy scanner. Detects leaked secrets, API keys, credentials.</p>
      </div>
      <button id="btn-save-ai" class="btn btn-primary" style="width:100%; margin-top:8px;">Save AI Settings</button>
    `,
    appearance: () => {
      const customThemes = JSON.parse(localStorage.getItem('xnaut-custom-themes') || '{}');
      function themeCard(name, colors, deletable) {
        const isActive = settings.activeTheme === name;
        const delBtn = deletable ? '<button class="delete-theme-btn" data-theme="' + name + '" style="background:none; border:none; color:var(--text-secondary); cursor:pointer; font-size:14px; padding:0 4px;" title="Delete">×</button>' : '';
        return '<div class="theme-card" data-theme="' + name + '" style="cursor:pointer; padding:6px 12px; border-radius:4px; background:' + (isActive ? 'rgba(59,130,246,0.1)' : 'rgba(255,255,255,0.03)') + '; display:flex; justify-content:space-between; align-items:center;">' +
          '<div style="display:flex; align-items:center; gap:10px;">' +
            '<span style="width:16px; height:16px; border-radius:3px; background:' + colors.bg + '; border:1px solid var(--border); display:inline-block;"></span>' +
            '<span style="font-size:13px;">' + name + '</span>' +
          '</div>' +
          '<div style="display:flex; align-items:center; gap:4px;">' +
            [colors.red, colors.green, colors.blue, colors.yellow].filter(Boolean).map(function(c) { return '<span style="width:6px; height:6px; border-radius:50%; background:' + c + ';"></span>'; }).join('') +
            delBtn +
          '</div>' +
        '</div>';
      }
      const defaultGrid = DEFAULT_THEME_NAMES.filter(function(n) { return THEME_PRESETS[n]; }).map(function(n) { return themeCard(n, THEME_PRESETS[n], false); }).join('');
      const bundledGrid = (typeof BUNDLED_THEME_NAMES !== 'undefined' ? BUNDLED_THEME_NAMES : []).filter(function(n) { return THEME_PRESETS[n]; }).map(function(n) { return themeCard(n, THEME_PRESETS[n], false); }).join('');
      const importedNames = Object.keys(customThemes).filter(function(n) { return !customThemes[n]._aiGenerated; });
      const aiNames = Object.keys(customThemes).filter(function(n) { return customThemes[n]._aiGenerated; });
      const importedGrid = importedNames.map(function(n) { return themeCard(n, THEME_PRESETS[n] || customThemes[n], true); }).join('');
      const aiGrid = aiNames.map(function(n) { return themeCard(n, THEME_PRESETS[n] || customThemes[n], true); }).join('');
      return `
        <h3>Default Themes</h3>
        <div class="settings-group" style="display:flex; flex-direction:column; gap:3px;">${defaultGrid}</div>
        ${aiGrid ? '<h3>AI Generated</h3><div class="settings-group" style="display:flex; flex-direction:column; gap:3px;">' + aiGrid + '</div>' : ''}
        ${importedGrid ? '<h3>Imported</h3><div class="settings-group" style="display:flex; flex-direction:column; gap:3px;">' + importedGrid + '</div>' : ''}
        ${bundledGrid ? '<h3>Bundled Themes</h3><div class="settings-group" style="display:flex; flex-direction:column; gap:3px;">' + bundledGrid + '</div>' : ''}
        <h3>AI Theme Generator</h3>
        <div class="settings-group">
          <p style="color:var(--text-secondary); font-size:12px; margin-bottom:8px;">Describe a vibe and AI creates a matching color theme</p>
          <input type="text" id="ai-theme-prompt" placeholder="e.g. dark cyberpunk with neon accents, calm forest greens, warm sunset..." style="width:100%; padding:8px 10px; background:var(--bg-primary); border:1px solid var(--border); border-radius:4px; color:var(--text-primary); font-size:13px;">
          <button id="btn-generate-theme" class="btn btn-primary" style="width:100%; margin-top:6px;">Generate Theme</button>
          <div id="theme-preview-strip" style="margin-top:8px; display:none;">
            <div style="display:flex; border-radius:6px; overflow:hidden; height:80px;">
              <div id="tp-bg" style="flex:3; display:flex; flex-direction:column; justify-content:center; padding:8px;">
                <span id="tp-title" style="font-family:monospace; font-size:11px; font-weight:600;">Theme Preview</span>
                <span id="tp-sample" style="font-family:monospace; font-size:10px; margin-top:4px;">$ ls -la ~/projects</span>
              </div>
              <div id="tp-colors" style="flex:1; display:flex; flex-direction:column;"></div>
            </div>
          </div>
        </div>
        <h3>Import Theme</h3>
        <div class="settings-group">
          <p style="color:var(--text-secondary); font-size:12px; margin-bottom:8px;">Import from <a href="#" id="link-warp-themes" style="color:var(--accent);">Warp Themes (100+)</a> (YAML) or JSON theme files</p>
          <div class="settings-row">
            <input type="file" id="theme-import-file" accept=".json,.yaml,.yml" style="font-size:12px;">
          </div>
          <div class="settings-row">
            <label>Or paste JSON:</label>
          </div>
          <textarea id="theme-import-text" placeholder='{"name":"My Theme","bg":"#1e1e2e","fg":"#cdd6f4",...}' style="width:100%; height:60px; background:var(--bg-primary); color:var(--text-primary); border:1px solid var(--border); border-radius:4px; font-family:monospace; font-size:11px; padding:8px; resize:vertical;"></textarea>
          <button id="btn-import-theme" class="btn btn-sm" style="margin-top:6px;">Import Theme</button>
        </div>
        <h3>Custom Colors</h3>
        <div class="settings-group">
          <div class="settings-row"><label>Background</label><input type="color" id="set-color-bg" value="${settings.terminalBgColor || '#1e1e1e'}"></div>
          <div class="settings-row"><label>Foreground</label><input type="color" id="set-color-fg" value="${settings.terminalTextColor || '#ffffff'}"></div>
          <div class="settings-row"><label>Cursor</label><input type="color" id="set-color-cursor" value="${settings.terminalCursorColor || '#3b82f6'}"></div>
          <div class="settings-row"><label>Chrome</label><input type="color" id="set-color-chrome" value="${settings.appChromeColor || '#1a1a1f'}"></div>
        </div>
        <h3>Font</h3>
        <div class="settings-group">
          <div class="settings-row">
            <label>Family</label>
            <select id="set-font-family">
              <optgroup label="Bundled (Nerd Font + Ligatures)">
                <option value="JetBrains Mono NF" ${settings.terminalFontFamily==='JetBrains Mono NF'?'selected':''}>JetBrains Mono NF</option>
                <option value="Fira Code NF" ${settings.terminalFontFamily==='Fira Code NF'?'selected':''}>Fira Code NF</option>
                <option value="Cascadia Code NF" ${settings.terminalFontFamily==='Cascadia Code NF'?'selected':''}>Cascadia Code NF</option>
                <option value="Source Code Pro NF" ${settings.terminalFontFamily==='Source Code Pro NF'?'selected':''}>Source Code Pro NF</option>
              </optgroup>
              <optgroup label="System Fonts">
                <option value="default" ${(settings.terminalFontFamily||'default')==='default'?'selected':''}>SF Mono / System</option>
                <option value="JetBrains Mono" ${settings.terminalFontFamily==='JetBrains Mono'?'selected':''}>JetBrains Mono</option>
                <option value="Fira Code" ${settings.terminalFontFamily==='Fira Code'?'selected':''}>Fira Code</option>
                <option value="Menlo" ${settings.terminalFontFamily==='Menlo'?'selected':''}>Menlo</option>
                <option value="Monaco" ${settings.terminalFontFamily==='Monaco'?'selected':''}>Monaco</option>
              </optgroup>
            </select>
          </div>
          <div class="settings-row">
            <label>Ligatures</label>
            <select id="set-ligatures">
              <option value="normal" ${(settings.fontLigatures||'normal')==='normal'?'selected':''}>Enabled</option>
              <option value="none" ${settings.fontLigatures==='none'?'selected':''}>Disabled</option>
            </select>
          </div>
          <div class="settings-row">
            <label>Size</label>
            <input type="number" id="set-font-size" value="${settings.fontSize || 14}" min="10" max="24" style="width:60px;">
          </div>
          <div class="settings-row">
            <label>Opacity</label>
            <input type="range" id="set-opacity" min="0" max="100" value="${settings.terminalOpacity ?? 100}" style="flex:1; max-width:150px;">
            <span id="set-opacity-val">${settings.terminalOpacity ?? 100}%</span>
          </div>
        </div>
        <button id="btn-save-appearance" class="btn btn-primary" style="width:100%; margin-top:8px;">Save Appearance</button>
      `;
    },
    shortcuts: () => {
      const rows = Object.entries(keybindings).map(([action, binding]) =>
        `<div class="settings-row" data-action="${action}">
          <label>${binding.label || action}</label>
          <button class="btn-test" style="min-width:100px; font-family:monospace;" data-rebind="${action}">${formatBinding(binding)}</button>
        </div>`
      ).join('');
      return `
        <h3>Keyboard Shortcuts</h3>
        <div class="settings-group">${rows}</div>
        <button id="btn-reset-keys" class="btn btn-primary" style="width:100%; margin-top:8px; background:#6c757d;">Reset All to Defaults</button>
      `;
    },
    nautify: () => `
      <h3>Shell</h3>
      <div class="settings-group">
        <div class="settings-row">
          <label>Default Shell</label>
          <select id="set-shell-type">
            <option value="default" ${(!settings.shellType || settings.shellType==='default')?'selected':''}>System Default (zsh)</option>
            <option value="/bin/zsh" ${settings.shellType==='/bin/zsh'?'selected':''}>Zsh</option>
            <option value="/bin/bash" ${settings.shellType==='/bin/bash'?'selected':''}>Bash</option>
            <option value="/bin/fish" ${settings.shellType==='/bin/fish'?'selected':''}>Fish</option>
          </select>
        </div>
      </div>
      <h3>SSH Profiles</h3>
      <div class="settings-group">
        <div id="ssh-profiles-list" style="font-size:13px; color:var(--text-secondary);">Loading...</div>
        <button id="btn-manage-ssh" class="btn btn-primary" style="width:100%; margin-top:8px;">Manage SSH Profiles</button>
      </div>
      <button id="btn-save-nautify" class="btn btn-primary" style="width:100%; margin-top:8px;">Save Shell Settings</button>
    `,
    triggers: () => `
      <h3>Triggers & Notifications</h3>
      <p style="color:var(--text-secondary); font-size:13px; margin-bottom:16px;">Match a keyword or a regex against terminal output and raise a desktop notification.</p>
      <button id="btn-manage-triggers" class="btn btn-primary" style="width:100%; margin-top:8px;">Manage Triggers</button>
    `,
    // Tasks Mode v1.6 — body rendered by tasks-mode-glue.js into the host div.
    tasksmode: () => `<div id="tasksmode-settings-host">Loading…</div>`,
    guardrails: () => `<div id="kill-switches-host">Loading…</div><div id="veto-settings-host">Loading…</div>`,
    // The core team's switch, threshold and idle reason (XNAUT-357).
    coreteam: () => `<div id="core-team-settings-host">Loading…</div>`,
    // Issues in from GitHub, Forgejo and Linear (XNAUT-382).
    issueintake: () => `<div id="issue-intake-settings-host">Loading…</div>`,
    // Mobile companion bridge (XNAUT-32) — filled async from mobile_info.
    mobile: () => `
      <h3>Mobile Companion</h3>
      <p style="color:var(--text-secondary); font-size:13px; margin-bottom:16px;">Mirror and control your sessions from the phone. Scan the QR with the camera — works over your tailnet, token-gated.</p>
      <div class="settings-group" id="mobile-pairing">Loading…</div>
    `,
  };

  content.innerHTML = (sections[section] || sections.ai)();

  // Name the pane after the section it is showing. Two things depend on this and
  // both were broken without it:
  //
  // A screen reader had no way to tell one settings pane from another. The nav
  // rail is labelled, the pane it drives was anonymous, so moving through the
  // sections announced nothing changing.
  //
  // And the GUI smoke walk could press all seven sections but verify none of
  // them, because every section exposes the identical nav rail to the
  // accessibility tree and differs only in this pane. Nine of nineteen surfaces
  // came back "pressed, unverifiable", which the release gate treats as a
  // refusal -- correctly, since "the click landed" is not "the pane rendered".
  // That gap is what let a Settings walk read green through two releases.
  //
  // aria-label on a plain div surfaces as AXDescription, which is what the
  // harness reads. A heading inside the pane would not do: several sections
  // start with the same words, and an ambiguous label is refused outright.
  const paneName = { ai: 'AI', tasksmode: 'Tasks Mode', appearance: 'Appearance',
    shortcuts: 'Keyboard Shortcuts', mobile: 'Mobile', nautify: 'Nautify',
    triggers: 'Triggers', coreteam: 'Core Team', issueintake: 'Issue Intake' }[section] || section;
  content.setAttribute('role', 'group');
  content.setAttribute('aria-label', `${paneName} settings pane`);

  // Post-render hooks
  // CSP forbids inline onclick attributes in the bundled app (Tauri's CSP
  // nonce injection makes browsers ignore 'unsafe-inline') — every settings
  // control binds here instead.
  content.querySelectorAll('[data-test-provider]').forEach((btn) => {
    btn.onclick = () => testProvider(btn.dataset.testProvider, btn);
  });
  content.querySelectorAll('[data-rebind]').forEach((btn) => {
    btn.onclick = () => rebindKey(btn.dataset.rebind, btn);
  });
  const _bind = (id, fn) => { const el = document.getElementById(id); if (el) el.onclick = fn; };
  // Mobile companion pairing panel (XNAUT-32) — async fill from the backend.
  if (section === 'mobile') {
    invoke('mobile_info').then((info) => {
      const host = document.getElementById('mobile-pairing');
      if (!host) return;
      host.innerHTML = `
        <div style="display:flex; gap:20px; align-items:flex-start;">
          <div id="mobile-qr" style="flex-shrink:0; border-radius:8px; overflow:hidden; background:#161616; padding:8px;"></div>
          <div style="display:flex; flex-direction:column; gap:10px; min-width:0;">
            <div><div style="font-size:11px; color:var(--text-secondary); letter-spacing:.08em;">URL</div>
              <code id="mobile-url" style="font-size:12px; word-break:break-all;"></code></div>
            <div><div style="font-size:11px; color:var(--text-secondary); letter-spacing:.08em;">TOKEN</div>
              <code id="mobile-token" style="font-size:12px; word-break:break-all;"></code></div>
            <div style="font-size:12px; color:var(--text-secondary);">Bridge ${info.enabled ? 'running on port ' + info.port : 'disabled in mobile.json'} · phone must be on the same tailnet.</div>
            <button id="btn-copy-mobile-url" class="btn btn-primary" style="align-self:flex-start;">Copy URL</button>
          </div>
        </div>`;
      // QR arrives as trusted, backend-generated SVG; URL/token via textContent.
      document.getElementById('mobile-qr').innerHTML = info.qrSvg;
      document.getElementById('mobile-url').textContent = info.url;
      document.getElementById('mobile-token').textContent = info.token;
      document.getElementById('btn-copy-mobile-url').onclick = () => navigator.clipboard.writeText(info.url);
    }).catch((e) => {
      const host = document.getElementById('mobile-pairing');
      if (host) host.textContent = 'Failed to load pairing info: ' + e;
    });
  }

  _bind('btn-save-ai', function () { saveAISettings(this); });
  _bind('btn-test-mcp-excalidraw', async function () {
    const dot = document.getElementById('excalidraw-mcp-status');
    this.disabled = true; this.textContent = 'Testing...';
    try {
      await saveMcpSettings();
      const endpoint = document.getElementById('set-mcp-excalidraw-url')?.value || '';
      if (/^http:\/\/(?:127\.0\.0\.1|localhost):3001\/mcp\/?$/i.test(endpoint)) {
        await invoke('mcp_start_local_excalidraw');
      }
      const tools = await invoke('mcp_list_tools', { server: 'excalidraw' });
      if (dot) dot.className = 'status-dot-sm green';
      this.textContent = `${tools.length} tools`;
    } catch (error) {
      if (dot) dot.className = 'status-dot-sm red';
      this.textContent = 'Failed';
      console.error('Excalidraw MCP test failed:', error);
    } finally { this.disabled = false; }
  });
  _bind('btn-save-nautify', function () { saveNautifySettings(this); });
  _bind('btn-reset-keys', () => resetAllKeybindings());
  _bind('btn-manage-ssh', () => { toggleSettingsPanel(); loadSSHProfiles(); showSSHModal(); });
  _bind('btn-manage-triggers', () => { toggleSettingsPanel(); showModal('triggers-modal'); renderTriggers(); });
  const provSelect = document.getElementById('set-default-provider');
  if (provSelect) provSelect.onchange = () => updateModelDropdown();

  // The policy editor is a window.* export with one call site, and this is it.
  // It shipped with none once (XNAUT-189): an exported global nothing calls is
  // a feature that exists in the source and nowhere else.
  if (section === 'guardrails' && typeof window.xnautRenderVetoSettings === 'function') {
    window.xnautRenderVetoSettings(document.getElementById('veto-settings-host'));
  }
  // The master switches sit above the policy they override (XNAUT-231 had no
  // surface at all). Same call-site rule as the editor above it.
  if (section === 'guardrails' && typeof window.xnautRenderKillSwitches === 'function') {
    window.xnautRenderKillSwitches(document.getElementById('kill-switches-host'));
  }
  if (section === 'tasksmode' && typeof window.xnautRenderTasksModeSettings === 'function') {
    window.xnautRenderTasksModeSettings(document.getElementById('tasksmode-settings-host'));
  }
  // Same call-site rule as the two above: an exported global nothing calls is
  // a feature that exists in the source and nowhere else (XNAUT-189).
  if (section === 'coreteam' && typeof window.xnautRenderCoreTeamSettings === 'function') {
    window.xnautRenderCoreTeamSettings(document.getElementById('core-team-settings-host'));
  }
  if (section === 'issueintake' && typeof window.xnautRenderIssueIntakeSettings === 'function') {
    window.xnautRenderIssueIntakeSettings(document.getElementById('issue-intake-settings-host'));
  }
  if (section === 'ai') {
    updateModelDropdown();
    loadMcpSettingsIntoForm();
    // ClawProxy start button
    const cpBtn = document.getElementById('btn-start-clawproxy');
    if (cpBtn && !clawproxyRunning) {
      cpBtn.onclick = async () => {
        cpBtn.textContent = 'Starting...';
        await startClawProxy();
        setTimeout(() => loadSettingsSection('ai'), 3000);
      };
    }
  }
  if (section === 'appearance') {
    const slider = document.getElementById('set-opacity');
    const val = document.getElementById('set-opacity-val');
    if (slider && val) slider.oninput = () => { val.textContent = slider.value + '%'; };

    // Theme card clicks
    document.querySelectorAll('.theme-card').forEach(card => {
      card.onclick = (e) => {
        if (e.target.closest('.delete-theme-btn')) return;
        applyThemeFromSettings(card.dataset.theme);
      };
    });
    document.querySelectorAll('.delete-theme-btn').forEach(btn => {
      btn.onclick = async (e) => {
        e.stopPropagation();
        if (await window.xnautConfirmDialog('Delete theme "' + btn.dataset.theme + '"?', 'Delete')) {
          deleteCustomTheme(btn.dataset.theme);
        }
      };
    });

    // Save appearance button
    // Live font preview
    const fontSelect = document.getElementById('set-font-family');
    if (fontSelect) fontSelect.onchange = () => {
      settings.terminalFontFamily = fontSelect.value;
      applyAppearanceToAllTerminals();
    };
    const ligSelect = document.getElementById('set-ligatures');
    if (ligSelect) ligSelect.onchange = () => {
      settings.fontLigatures = ligSelect.value;
      applyAppearanceToAllTerminals();
    };
    const sizeInput = document.getElementById('set-font-size');
    if (sizeInput) sizeInput.onchange = () => {
      settings.fontSize = parseInt(sizeInput.value) || 14;
      applyAppearanceToAllTerminals();
    };

    const saveBtn = document.getElementById('btn-save-appearance');
    if (saveBtn) saveBtn.onclick = () => {
      settings.terminalBgColor = document.getElementById('set-color-bg')?.value;
      settings.terminalTextColor = document.getElementById('set-color-fg')?.value;
      settings.terminalCursorColor = document.getElementById('set-color-cursor')?.value;
      settings.appChromeColor = document.getElementById('set-color-chrome')?.value;
      saveAppearanceSettings();
      flashSavedButton(saveBtn);
    };

    // Color picker live preview
    ['set-color-bg', 'set-color-fg', 'set-color-cursor', 'set-color-chrome'].forEach(id => {
      const input = document.getElementById(id);
      if (input) input.oninput = () => {
        settings.terminalBgColor = document.getElementById('set-color-bg')?.value;
        settings.terminalTextColor = document.getElementById('set-color-fg')?.value;
        settings.terminalCursorColor = document.getElementById('set-color-cursor')?.value;
        settings.appChromeColor = document.getElementById('set-color-chrome')?.value;
        settings.activeTheme = null;
        applyAppearanceToAllTerminals();
      };
    });

    // Theme import
    const warpLink = document.getElementById('link-warp-themes');
    if (warpLink) warpLink.onclick = (e) => {
      e.preventDefault();
      if (window.__TAURI__?.shell?.open) window.__TAURI__.shell.open('https://github.com/warpdotdev/themes/tree/main/standard');
      else window.open('https://github.com/warpdotdev/themes/tree/main/standard', '_blank');
    };
    // AI Theme Generator button
    const genBtn = document.getElementById('btn-generate-theme');
    if (genBtn) {
      genBtn.addEventListener('click', function() { generateAITheme(); });
    }
    const importBtn = document.getElementById('btn-import-theme');
    if (importBtn) importBtn.onclick = () => importTheme();
    const fileInput = document.getElementById('theme-import-file');
    if (fileInput) fileInput.onchange = (e) => {
      const file = e.target.files[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = (ev) => {
        document.getElementById('theme-import-text').value = ev.target.result;
      };
      reader.readAsText(file);
    };
  }
}

// Settings save functions
const MODEL_OPTIONS = {
  anthropic: [
    { id: 'claude-sonnet-4-5-20250929', name: 'Claude Sonnet 4.5' },
    { id: 'claude-opus-4-1-20250805', name: 'Claude Opus 4.1' },
    { id: 'claude-sonnet-4-20250514', name: 'Claude Sonnet 4' },
    { id: 'claude-opus-4-20250514', name: 'Claude Opus 4' },
    { id: 'claude-3-7-sonnet-20250219', name: 'Claude Sonnet 3.7' },
    { id: 'claude-3-5-haiku-20241022', name: 'Claude Haiku 3.5' },
  ],
  openai: [
    { id: 'gpt-4o', name: 'GPT-4o' },
    { id: 'gpt-4o-mini', name: 'GPT-4o Mini' },
    { id: 'gpt-4-turbo', name: 'GPT-4 Turbo' },
    { id: 'gpt-3.5-turbo', name: 'GPT-3.5 Turbo' },
    { id: 'o1', name: 'o1' },
    { id: 'o1-mini', name: 'o1 Mini' },
  ],
  openrouter: [
    { id: 'anthropic/claude-3.5-sonnet', name: 'Claude 3.5 Sonnet' },
    { id: 'anthropic/claude-3-opus', name: 'Claude 3 Opus' },
    { id: 'openai/gpt-4o', name: 'GPT-4o' },
    { id: 'meta-llama/llama-3.1-70b-instruct', name: 'Llama 3.1 70B' },
    { id: 'google/gemini-pro-1.5', name: 'Gemini Pro 1.5' },
    { id: 'mistralai/mistral-large-latest', name: 'Mistral Large' },
  ],
  perplexity: [
    { id: 'sonar', name: 'Sonar' },
    { id: 'sonar-pro', name: 'Sonar Pro' },
    { id: 'sonar-reasoning', name: 'Sonar Reasoning' },
    { id: 'sonar-reasoning-pro', name: 'Sonar Reasoning Pro' },
    { id: 'sonar-deep-research', name: 'Sonar Deep Research' },
  ],
  nautgate: [
    { id: 'auto', name: 'Auto (NautGate routes)' },
    { id: 'openrouter/google/gemini-pro', name: 'Gemini Pro' },
    { id: 'openrouter/google/gemini-flash', name: 'Gemini Flash' },
    { id: 'openrouter/moonshotai/kimi-k2-thinking', name: 'Kimi K2 Thinking' },
    { id: 'openrouter/moonshotai/kimi-k2.6', name: 'Kimi K2.6' },
    { id: 'openrouter/deepseek/deepseek-v4-flash', name: 'DeepSeek V4 Flash' },
  ],
};

window.updateModelDropdown = async function() {
  const provider = document.getElementById('set-default-provider')?.value || 'anthropic';
  const select = document.getElementById('set-default-model');
  if (!select) return;

  // For local providers, fetch available models from their API
  if (provider === 'ollama') {
    select.innerHTML = '<option>Loading...</option>';
    try {
      const url = document.getElementById('set-ollama-url')?.value || 'http://localhost:11434';
      const data = await invoke('net_fetch_json', { url: url.replace(/\/$/, '') + '/api/tags', method: 'GET', body: null });
      const models = (data.models || []).map(m => ({ id: m.name, name: m.name }));
      select.innerHTML = models.length ? models.map(m =>
        '<option value="' + m.id + '"' + (settings.llmModel === m.id ? ' selected' : '') + '>' + m.name + '</option>'
      ).join('') : '<option>No models found</option>';
    } catch (e) {
      select.innerHTML = '<option>Ollama not reachable</option>';
    }
    return;
  }

  if (provider === 'lmstudio') {
    select.innerHTML = '<option>Loading...</option>';
    try {
      const url = document.getElementById('set-lmstudio-url')?.value || 'http://localhost:1234';
      const data = await invoke('net_fetch_json', { url: url.replace(/\/$/, '') + '/v1/models', method: 'GET', body: null });
      const models = (data.data || []).map(m => ({ id: m.id, name: m.id }));
      select.innerHTML = models.length ? models.map(m =>
        '<option value="' + m.id + '"' + (settings.llmModel === m.id ? ' selected' : '') + '>' + m.name + '</option>'
      ).join('') : '<option>No models loaded</option>';
    } catch (e) {
      select.innerHTML = '<option>LM Studio not reachable</option>';
    }
    return;
  }

  // Static model lists for cloud providers
  const live = (window.xnautModelCatalog && window.xnautModelCatalog.forProvider(provider)) || [];
  const models = live.length ? live : (MODEL_OPTIONS[provider] || []); // live catalog first, hardcoded only as fallback
  select.innerHTML = models.map(m =>
    '<option value="' + m.id + '"' + (settings.llmModel === m.id ? ' selected' : '') + '>' + m.name + '</option>'
  ).join('');
};

function normalizeOpenAIEndpoint(url) {
  const base = String(url || '').trim().replace(/\/+$/, '');
  if (!base) return '';
  return /\/v1$/i.test(base) ? base : base + '/v1';
}

function aiSettingsChatEndpoint(provider) {
  if (provider === 'lmstudio') return normalizeOpenAIEndpoint(settings.lmstudioUrl || 'http://localhost:1234');
  if (provider === 'ollama') return normalizeOpenAIEndpoint(settings.ollamaUrl || 'http://localhost:11434');
  if (provider === 'openai') return 'https://api.openai.com/v1';
  if (provider === 'openrouter') return 'https://openrouter.ai/api/v1';
  if (provider === 'perplexity') return 'https://api.perplexity.ai';
  if (provider === 'nautgate') return (settings.nautgateUrl || 'http://localhost:8090/v1').replace(/\/+$/, '');
  return '';
}

function aiSettingsChatApiKey(provider) {
  if (provider === 'openai') return settings.apiKeyOpenAI || '';
  if (provider === 'openrouter') return settings.apiKeyOpenRouter || '';
  if (provider === 'perplexity') return settings.apiKeyPerplexity || '';
  if (provider === 'nautgate') return settings.apiKeyNautGate || '';
  return '';
}

window.xnautSyncChatSettingsFromAiSettings = async function() {
  const provider = settings.llmProvider || '';
  const endpoint = aiSettingsChatEndpoint(provider);
  const model = settings.llmModel || '';
  if (!window.__TAURI__?.core?.invoke) return false;

  const current = await invoke('settings_get');
  const providerUpdates = [
    { name: 'lmstudio', endpoint: aiSettingsChatEndpoint('lmstudio'), api_key: null, enabled: true },
    { name: 'ollama', endpoint: aiSettingsChatEndpoint('ollama'), api_key: null, enabled: true },
    settings.apiKeyOpenAI ? { name: 'openai', endpoint: aiSettingsChatEndpoint('openai'), api_key: settings.apiKeyOpenAI, enabled: true } : null,
    settings.apiKeyOpenRouter ? { name: 'openrouter', endpoint: aiSettingsChatEndpoint('openrouter'), api_key: settings.apiKeyOpenRouter, enabled: true } : null,
    settings.apiKeyPerplexity ? { name: 'perplexity', endpoint: aiSettingsChatEndpoint('perplexity'), api_key: settings.apiKeyPerplexity, enabled: true } : null,
    // NautGate is first-class: its visible Settings-page URL must be durable
    // even before a token is entered. The merge below retains an existing key
    // if this webview has not hydrated it yet.
    { name: 'nautgate', endpoint: aiSettingsChatEndpoint('nautgate'), api_key: settings.apiKeyNautGate || null, enabled: true },
  ].filter(Boolean);
  // AI Settings is still backed by the legacy webview store. Merge it into
  // the Rust provider registry instead of replacing the registry wholesale:
  // replacing it dropped an already-configured NautGate row whenever the
  // legacy store had no copy of that key.
  const configuredProviders = (current.llm_providers || []).map((item) => ({ ...item }));
  providerUpdates.forEach((update) => {
    const index = configuredProviders.findIndex((item) => item.name === update.name);
    if (index < 0) {
      configuredProviders.push(update);
      return;
    }
    configuredProviders[index] = {
      ...configuredProviders[index],
      ...update,
      api_key: update.api_key || configuredProviders[index].api_key || null,
    };
  });
  await invoke('settings_set', {
    settings: {
      ...current,
      // harness_local rides along even when no default model is picked yet —
      // agent_launch reads it, and a fresh install has no model selected.
      llm: endpoint && model ? {
        ...(current.llm || {}),
        provider,
        endpoint,
        model,
        api_key: aiSettingsChatApiKey(provider) || null,
        harness_local: !!settings.harnessLocal,
      } : { ...(current.llm || {}), harness_local: !!settings.harnessLocal },
      llm_providers: configuredProviders,
    },
  });
  return !!(endpoint && model);
};

async function loadMcpSettingsIntoForm() {
  try {
    const current = await invoke('settings_get');
    const server = (current.mcp_servers || []).find((item) => item.name === 'excalidraw') || {};
    const enabled = document.getElementById('set-mcp-excalidraw-enabled');
    const url = document.getElementById('set-mcp-excalidraw-url');
    const key = document.getElementById('set-mcp-excalidraw-key');
    const configuredUrl = server.url === 'https://api.excalidraw.com/api/v1/mcp' && !server.api_key
      ? 'http://127.0.0.1:3001/mcp'
      : server.url;
    if (enabled) enabled.checked = !!server.enabled;
    if (url) url.value = configuredUrl || 'http://127.0.0.1:3001/mcp';
    if (key) key.value = server.api_key || '';
  } catch (error) {
    console.error('Failed to load MCP settings:', error);
  }
}

async function saveMcpSettings() {
  const current = await invoke('settings_get');
  const others = (current.mcp_servers || []).filter((item) => item.name !== 'excalidraw');
  const apiKey = document.getElementById('set-mcp-excalidraw-key')?.value.trim() || '';
  current.mcp_servers = others.concat([{
    name: 'excalidraw',
    enabled: !!document.getElementById('set-mcp-excalidraw-enabled')?.checked,
    url: document.getElementById('set-mcp-excalidraw-url')?.value.trim() || 'http://127.0.0.1:3001/mcp',
    api_key: apiKey || null,
  }]);
  await invoke('settings_set', { settings: current });
}

window.saveAISettings = async function(btn) {
  settings.ollamaUrl = document.getElementById('set-ollama-url')?.value;
  settings.lmstudioUrl = document.getElementById('set-lmstudio-url')?.value;
  settings.apiKeyAnthropic = document.getElementById('set-api-anthropic')?.value;
  settings.apiKeyOpenAI = document.getElementById('set-api-openai')?.value;
  settings.apiKeyOpenRouter = document.getElementById('set-api-openrouter')?.value;
  settings.apiKeyPerplexity = document.getElementById('set-api-perplexity')?.value;
  settings.nautgateUrl = document.getElementById('set-nautgate-url')?.value;
  settings.apiKeyNautGate = document.getElementById('set-api-nautgate')?.value;
  settings.llmProvider = document.getElementById('set-default-provider')?.value;
  settings.llmModel = document.getElementById('set-default-model')?.value;
  settings.harnessLocal = !!document.getElementById('set-harness-local')?.checked;
  settings.voiceEnabled = document.getElementById('set-voice-enabled')?.checked;
  settings.kokoroUrl = document.getElementById('set-kokoro-url')?.value;
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));
  try {
    await saveMcpSettings();
    await window.xnautSyncChatSettingsFromAiSettings();
    flashSavedButton(btn);
  } catch (e) {
    console.error('Failed to sync chat settings:', e);
    flashSavedButton(btn, 'Saved, chat sync failed');
  }
};

window.saveAppearanceSettings = function() {
  settings.terminalFontFamily = document.getElementById('set-font-family')?.value;
  settings.fontSize = parseInt(document.getElementById('set-font-size')?.value) || 14;
  settings.terminalOpacity = parseInt(document.getElementById('set-opacity')?.value) ?? 100;
  settings.fontLigatures = document.getElementById('set-ligatures')?.value || 'normal';
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));
  applyAppearanceToAllTerminals();
};

window.applyThemeFromSettings = function(name) {
  const colors = THEME_PRESETS[name];
  if (!colors) return;
  settings.activeTheme = name;
  settings.terminalBgColor = colors.bg;
  settings.terminalTextColor = colors.fg;
  settings.terminalCursorColor = colors.cursor;
  settings.appChromeColor = colors.chrome;
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));
  applyAppearanceToAllTerminals();
  loadSettingsSection('appearance');
};

window.saveNautifySettings = function(btn) {
  settings.shellType = document.getElementById('set-shell-type')?.value;
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));  flashSavedButton(btn);
};

window.testProvider = async function(provider, btn) {
  // Probe through the Rust backend (net_probe) — webview fetch() is blocked
  // by CORS for LM Studio/Ollama even when the server is healthy.
  const dot = document.getElementById(provider + '-status');
  const setBtn = (state, label) => {
    if (!btn) return;
    btn.classList.remove('ok', 'fail');
    if (state) btn.classList.add(state);
    btn.textContent = label;
  };
  if (dot) dot.className = 'status-dot-sm gray';
  setBtn(null, 'Testing…');
  if (btn) btn.disabled = true;
  let ok = false;
  try {
    if (provider === 'ollama') {
      const url = (document.getElementById('set-ollama-url')?.value || 'http://localhost:11434').replace(/\/$/, '');
      ok = await invoke('net_probe', { url: url + '/api/tags' });
    } else if (provider === 'lmstudio') {
      const url = (document.getElementById('set-lmstudio-url')?.value || 'http://localhost:1234').replace(/\/$/, '');
      ok = await invoke('net_probe', { url: url + '/v1/models' });
    }
  } catch (e) {
    console.error('testProvider failed:', e);
    ok = false;
  }
  if (btn) btn.disabled = false;
  if (dot) dot.className = 'status-dot-sm ' + (ok ? 'green' : 'red');
  setBtn(ok ? 'ok' : 'fail', ok ? 'Connected ✓' : 'Failed ✗');
};

// Flash a save button green with a confirmation, then restore it.
window.flashSavedButton = function (btn, label) {
  if (!btn) return;
  const original = btn.dataset.origLabel || btn.textContent;
  btn.dataset.origLabel = original;
  btn.classList.add('btn-saved');
  btn.textContent = label || '✓ Saved';
  setTimeout(() => {
    btn.classList.remove('btn-saved');
    btn.textContent = original;
  }, 2000);
};

window.rebindKey = function(action, btn) {
  btn.textContent = 'Press keys...';
  btn.style.borderColor = 'var(--accent)';
  const handler = (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) return;
    const newBinding = { ...keybindings[action], ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey };
    if (e.code.startsWith('Key') || e.code.startsWith('Arrow') || e.code.startsWith('Digit')) {
      newBinding.code = e.code; delete newBinding.key;
    } else {
      newBinding.key = e.key; delete newBinding.code;
    }
    keybindings[action] = newBinding;
    saveKeybindings();
    btn.textContent = formatBinding(newBinding);
    btn.style.borderColor = '';
    document.removeEventListener('keydown', handler, true);
  };
  document.addEventListener('keydown', handler, true);
};

window.resetAllKeybindings = function() {
  keybindings = { ...DEFAULT_KEYBINDINGS };
  saveKeybindings();
  loadSettingsSection('shortcuts');
};

function initSharedStatusBar() {
  const bar = document.getElementById('shared-status-bar');
  if (!bar) return;
  bar.innerHTML = `
    <div class="status-bar-left">
      <span class="status-icon">📁</span>
      <span class="status-path" id="shared-status-path">~</span>
      <span class="status-git" id="shared-status-git"></span>
    </div>
    <div class="status-bar-right"></div>
  `;
}

// Update shared status bar with focused pane's info
function updateSharedStatusBar(sessionId) {
  const sharedPath = document.getElementById('shared-status-path');
  const sharedGit = document.getElementById('shared-status-git');
  const panePath = document.getElementById('status-path-' + sessionId);
  const paneGit = document.getElementById('status-git-' + sessionId);
  if (sharedPath && panePath) sharedPath.textContent = panePath.textContent;
  if (sharedGit && paneGit) sharedGit.innerHTML = paneGit.innerHTML;
}

// Terminal Management
async function createTerminal(tabId, paneId, parentContainer, cwd) {
  const sessionId = `session-${++sessionCounter}`;
  paneId = paneId || 'a'; // Default to pane 'a' for single layout

  // Create Warp-style terminal structure
  const pane = document.createElement('div');
  pane.className = 'terminal-pane warp-style';
  pane.style.flex = '1';
  pane.style.gridArea = paneId;
  pane.dataset.sessionId = sessionId;
  pane.dataset.paneId = paneId;

  // Click-to-focus handler for split panes
  pane.addEventListener('mousedown', () => {
    const tab = tabs.find(t => t.id === tabId);
    if (tab) {
      const idx = tab.terminals.findIndex(t => t.pane === pane);
      if (idx >= 0 && idx !== tab.focusedPaneIndex) {
        tab.focusedPaneIndex = idx;
        updateFocusIndicator(tab);
        const terminal = tab.terminals[idx];
        if (terminal) terminal.term.focus();
      }
    }
  });

  // Hidden path/git elements for status bar tracking (no visible pane header)
  const hiddenInfo = document.createElement('div');
  hiddenInfo.style.display = 'none';
  hiddenInfo.innerHTML = `<span class="status-path" id="status-path-${sessionId}">~</span><span class="status-git" id="status-git-${sessionId}"></span>`;
  pane.appendChild(hiddenInfo);

  // Hover close button (top-right corner, appears on hover)
  const closeBtn = document.createElement('button');
  closeBtn.className = 'pane-close-btn';
  closeBtn.title = 'Close pane';
  closeBtn.textContent = '×';
  closeBtn.style.cssText = 'position:absolute; top:2px; right:4px; z-index:10; background:rgba(0,0,0,0.5); border:none; color:#6c757d; cursor:pointer; font-size:14px; padding:0 5px; line-height:1.2; border-radius:3px; opacity:0; transition:opacity 0.15s;';
  closeBtn.addEventListener('click', (e) => { e.stopPropagation(); closePaneByElement(pane, tabId); });
  pane.appendChild(closeBtn);

  // Show close button on hover
  pane.addEventListener('mouseenter', () => { closeBtn.style.opacity = '1'; });
  pane.addEventListener('mouseleave', () => { closeBtn.style.opacity = '0'; });

  // Terminal area (interactive)
  const terminalDiv = document.createElement('div');
  terminalDiv.className = 'terminal-output';
  terminalDiv.style.flex = '1';
  pane.appendChild(terminalDiv);

  // Append pane to container (parentContainer used for splits)
  (parentContainer || terminalContainer).appendChild(pane);

  // Get appearance settings
  const bgColor = settings.terminalBgColor || '#1e1e1e';
  const textColor = settings.terminalTextColor || '#ffffff';
  const cursorColor = settings.terminalCursorColor || '#3b82f6';
  const opacity = settings.terminalOpacity !== undefined ? settings.terminalOpacity : 100;

  const term = new Terminal({
    theme: buildTerminalTheme(bgColor, textColor, cursorColor),
    fontFamily: '"SF Mono", Menlo, "JetBrains Mono", "DejaVu Sans Mono", "Fira Code", monospace',
    fontSize: terminalFontSize(),
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    scrollback: 10000,
    allowTransparency: true,
    scrollOnBottom: false,
  });

  term.open(terminalDiv);

  pane.style.background = 'transparent';
  terminalDiv.style.opacity = opacity / 100;

  // Add FitAddon to make terminal fill container
  const fitAddon = new FitAddon.FitAddon();
  term.loadAddon(fitAddon);

  // Add WebLinksAddon for clickable URLs
  if (window.WebLinksAddon) {
    const webLinksAddon = new WebLinksAddon.WebLinksAddon((_event, uri) => {
      try {
        if (window.__TAURI__?.shell?.open) {
          window.__TAURI__.shell.open(uri);
        } else if (window.__TAURI__) {
          invoke('plugin:shell|open', { path: uri });
        } else {
          window.open(uri, '_blank');
        }
      } catch (e) {
        window.open(uri, '_blank');
      }
    });
    term.loadAddon(webLinksAddon);
  }



  // Fit terminal to container (with slight delay to ensure layout is ready)
  setTimeout(() => {
    fitAddon.fit();
  }, 10);

  // Add drag and drop support for files (on pane, not terminalDiv — xterm blocks drag events)
  pane.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = 'copy';
    pane.style.outline = '2px solid var(--accent)';
  });

  pane.addEventListener('dragleave', () => {
    pane.style.outline = '';
  });

  pane.addEventListener('drop', async (e) => {
    e.preventDefault();
    e.stopPropagation();
    pane.style.outline = '';
    const path = e.dataTransfer.getData('text/plain');
    if (path) {
      await insertPathToTerminal(path);
    }
  });

  // Show startup banner only on first terminal
  const showBanner = isFirstTerminal;
  if (showBanner) {
    isFirstTerminal = false;
    term.writeln('');
    term.writeln('    \x1b[1;96m██╗  ██╗███╗   ██╗ █████╗ ██╗   ██╗████████╗\x1b[0m');
    term.writeln('    \x1b[1;96m╚██╗██╔╝████╗  ██║██╔══██╗██║   ██║╚══██╔══╝\x1b[0m');
    term.writeln('    \x1b[1;96m ╚███╔╝ ██╔██╗ ██║███████║██║   ██║   ██║   \x1b[0m');
    term.writeln('    \x1b[1;96m ██╔██╗ ██║╚██╗██║██╔══██║██║   ██║   ██║   \x1b[0m');
    term.writeln('    \x1b[1;96m██╔╝ ██╗██║ ╚████║██║  ██║╚██████╔╝   ██║   \x1b[0m');
    term.writeln('    \x1b[1;96m╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝    ╚═╝   \x1b[0m');
    term.writeln('');
    term.writeln('         \x1b[1;33m⚡ AI-Powered Native Terminal ⚡\x1b[0m');
    term.writeln('');
  }

  try {
    // If this tab is attached to an existing backend session (e.g. an agent
    // launched via worktree.js or an SSH session), reuse it instead of
    // spinning up a fresh shell.
    const tabForExistingSession = tabs.find(t => t.id === tabId);
    const existingAgentSessionId = tabForExistingSession && tabForExistingSession.agentSessionId;
    let backendSessionId;
    if (existingAgentSessionId) {
      console.log('🔗 Attaching tab to existing agent session:', existingAgentSessionId);
      backendSessionId = existingAgentSessionId;
    } else {
      console.log('🔄 Attempting to create terminal session...');

      // Determine shell to use
      let shell = null;
      if (settings.shellType && settings.shellType !== 'default') {
        shell = settings.shellType === 'custom' ? settings.customShell : settings.shellType;
        console.log(`Using custom shell: ${shell}`);
      }

      // Create terminal session via Tauri. cwd (when given) opens the shell in
      // the project directory; PtyConfig.working_dir handles it backend-side.
      // Back the tab with a Zellij session named after the tab, so the work
      // outlives the app (XNAUT-66). The tab id is already stable, so reopening
      // with the same id reattaches rather than starting fresh. The backend
      // falls back to a plain shell when Zellij is not installed.
      //
      // OFF BY DEFAULT until the nesting work lands: a Zellij-backed tab shows
      // Zellij's own status bar inside xNAUT's chrome and its prefix competes
      // with xNAUT's key handling. The persistence works today — the presentation
      // does not. Opt in with:  localStorage['xnaut-persistent-tabs'] = '1'
      let persistentTabs = false;
      try { persistentTabs = localStorage.getItem('xnaut-persistent-tabs') === '1'; } catch (_) {}
      const sessionName = persistentTabs ? zellijNameForTab(tabId) : undefined;
      const config = { ...(shell ? { shell } : {}), ...(cwd ? { workingDir: cwd } : {}), ...(sessionName ? { sessionName } : {}) };
      const result = await invoke('create_terminal_session', { config });
      console.log('📦 Terminal session result:', result);

      if (!result || !result.session_id) {
        throw new Error('Invalid response from create_terminal_session: ' + JSON.stringify(result));
      }

      backendSessionId = result.session_id;
      console.log(`✅ Terminal session created: ${backendSessionId}`);
    }

    // Clear the intro banner after 3 seconds (only if banner was shown)
    if (showBanner) {
      setTimeout(async () => {
        try {
          await invoke('write_to_terminal', { sessionId: backendSessionId, data: 'clear\n' });
        } catch (e) { /* session might already be closed */ }
      }, 3000);
    }

    // Listen for terminal output
    console.log(`📡 Setting up listener for: terminal-output:${backendSessionId}`);
    await listen(`terminal-output:${backendSessionId}`, (event) => {
      // Per-chunk logging is opt-in: an agent spinner emits ~10 chunks/sec and
      // every console.log is also forwarded to debug.log over IPC — one busy
      // terminal produced 4,446 log entries in 5 min (freeze investigation
      // 2026-07-13). Set window.XNAUT_VERBOSE = true in DevTools to re-enable.
      if (window.XNAUT_VERBOSE) console.log('📥 Received terminal output:', event.payload);
      // Decode base64 data from backend as proper UTF-8
      // atob() alone mangles multi-byte UTF-8 (box-drawing chars, Unicode symbols)
      const binaryString = atob(event.payload.data);
      const bytes = Uint8Array.from(binaryString, c => c.charCodeAt(0));
      const data = new TextDecoder('utf-8').decode(bytes);
      // Detect OSC 133 shell integration markers for block-based output
      // 133;A = prompt start, 133;C = command start (user pressed enter)
      if (data.includes('\x1b]133;')) {
        if (data.includes('\x1b]133;A')) {
          // Prompt is about to be drawn — end of previous command output
          const termInfo = findTerminalBySession(sessionId);
          if (termInfo) {
            termInfo.shellIntegration = true;
            if (termInfo.commandRunning) {
              termInfo.commandRunning = false;
            }
          }
        }
        if (data.includes('\x1b]133;C')) {
          // User executed a command
          const termInfo = findTerminalBySession(sessionId);
          if (termInfo) {
            termInfo.commandRunning = true;
          }
        }
      }

      // Strip OSC 133 sequences before writing to terminal (they're control-only)
      const cleanedData = data.replace(/\x1b\]133;[A-Z]\x07/g, '');
      term.write(cleanedData);

      // Auto-scroll to bottom to show latest content at top (due to column-reverse)
      setTimeout(() => {
        term.scrollToBottom();
      }, 10);

      // Parse directory from OSC 7 sequences: \033]7;file://hostname/path\007
      const osc7Match = data.match(/\x1b\]7;file:\/\/[^\/]*(.+?)\x07/);
      if (osc7Match && osc7Match[1]) {
        const dirPath = decodeURIComponent(osc7Match[1]);
        updateDirectoryStatus(sessionId, backendSessionId, dirPath);
      }

      // Capture output for AI context
      const cleanData = data.replace(/\x1b\[[0-9;]*m/g, '');
      terminalOutputBuffer += cleanData;

      // Check triggers
      checkTriggers(data);


      // Keep buffer manageable
      if (terminalOutputBuffer.length > maxBufferSize) {
        terminalOutputBuffer = terminalOutputBuffer.slice(-maxBufferSize);
      }
    });

    // An Agent Space launch intentionally keeps this tab in the background.
    // When the user later opens the terminal, replay what the PTY emitted
    // before its xterm listener existed so the inspector is not blank.
    if (existingAgentSessionId) {
      try {
        const snapshot = await invoke('terminal_output_snapshot', { sessionId:backendSessionId });
        if (snapshot) {
          const binary = atob(snapshot);
          const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
          term.write(new TextDecoder('utf-8').decode(bytes));
          term.scrollToBottom();
        }
      } catch (_) { /* session may have ended before attachment */ }
    }

    // Listen for shell exit — auto-close pane or show exit message
    listen(`terminal-closed:${backendSessionId}`, (event) => {
      const exitCode = event.payload?.exitCode ?? -1;
      console.log(`🔚 Shell exited (session ${backendSessionId}, code ${exitCode})`);

      const tab = tabs.find(t => t.id === tabId);
      if (!tab) return;

      const termIdx = tab.terminals.findIndex(t => t.sessionId === backendSessionId);
      if (termIdx < 0) return;

      if (tab.terminals.length > 1) {
        // Split pane — remove just this pane
        const terminal = tab.terminals[termIdx];
        if (terminal.pane.parentNode) {
          terminal.pane.parentNode.removeChild(terminal.pane);
        }
        window.removeEventListener('resize', terminal.handleResize);
        tab.terminals.splice(termIdx, 1);

        // Fix layout
        const paneCount = tab.terminals.length;
        if (paneCount === 1) tab.layoutType = 'single';
        else if (paneCount === 2) tab.layoutType = 'vsplit';
        else if (paneCount === 3) tab.layoutType = 'left-right2';
        else if (paneCount === 4) tab.layoutType = 'grid';
        else if (paneCount === 5) tab.layoutType = 'grid-5a';

        const newTemplate = LAYOUT_TEMPLATES[tab.layoutType];
        tab.terminals.forEach((t, i) => { t.paneId = newTemplate.panes[i]; });
        tab.focusedPaneIndex = Math.min(tab.focusedPaneIndex || 0, tab.terminals.length - 1);
        tab.colSizes = null;
        tab.rowSizes = null;
        applyLayout(tab);

        const newFocused = tab.terminals[tab.focusedPaneIndex];
        if (newFocused) newFocused.term.focus();
      } else {
        // Single pane — close the whole tab
        closeTab(tabId);
      }
    });

    console.log('⌨️ Terminal ready for interactive use');

    // Intercept split-screen shortcuts so they don't get sent to PTY as escape sequences
    term.attachCustomKeyEventHandler((e) => {
      if (e.altKey && e.type === 'keydown') {
        // Alt+D (split vertical), Shift+Alt+D (split horizontal)
        if (e.code === 'KeyD') return false;
        // Alt+B (split → browser), Alt+M (split → markdown)
        if (e.code === 'KeyB' || e.code === 'KeyM') return false;
        // Alt+W (close pane)
        if (e.code === 'KeyW') return false;
        // Alt+Arrow keys (navigate panes)
        if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.code)) return false;
      }
      return true; // Let everything else through to xterm
    });

    // Enable keyboard input - send all keystrokes to PTY (with autocomplete)
    term.onData(async (data) => {
      // Process autocomplete before sending to PTY
      const result = handleAutocompleteInput(data, term, sessionId, backendSessionId);
      if (result === 'consumed') return; // Tab accepted a suggestion

      try {
        await invoke('write_to_terminal', {
          sessionId: backendSessionId,
          data: data
        });
      } catch (error) {
        console.error('Error writing to terminal:', error);
      }
    });

    // Update directory and git status initially
    updateDirectoryStatus(sessionId, backendSessionId);

    // Poll for directory changes every 2 seconds
    const dirPollInterval = setInterval(async () => {
      try {
        // Get current directory from backend
        const currentDir = await invoke('get_current_directory', { sessionId: backendSessionId });
        if (currentDir) {
          await updateDirectoryStatus(sessionId, backendSessionId, currentDir);
        }
      } catch (error) {
        // Session might be closed
        clearInterval(dirPollInterval);
      }
    }, 2000);

    // Handle resize
    const handleResize = async () => {
      try {
        // First fit the terminal to its container
        fitAddon.fit();

        // Then notify backend of the new size
        window.xnautZellijSettle(backendSessionId);
        await invoke('resize_terminal', {
          sessionId: backendSessionId,
          cols: term.cols,
          rows: term.rows
        });
      } catch (error) {
        console.error('Error resizing terminal:', error);
      }
    };

    window.addEventListener('resize', handleResize);
    setTimeout(handleResize, 100);

    const tab = tabs.find(t => t.id === tabId);
    if (tab) {
      tab.terminals.push({
        sessionId: backendSessionId,
        frontendSessionId: sessionId,
        paneId,
        term,
        pane,
        fitAddon,
        handleResize
      });
    }

    term.focus();

    return { sessionId: backendSessionId, term, pane, handleResize };
  } catch (error) {
    console.error('❌ Failed to create terminal session:', error);
    console.error('Error details:', {
      message: error.message,
      stack: error.stack,
      type: typeof error
    });

    term.writeln(`\r\n\x1b[1;31m❌ Error: Failed to create terminal session\x1b[0m\r\n`);
    term.writeln(`\x1b[1;33m${error.message || error}\x1b[0m\r\n`);
    term.writeln(`\x1b[1;36mCheck browser console (Cmd+Option+I) for details\x1b[0m\r\n`);

    // Show alert for critical error
    alert(`Terminal creation failed!\n\nError: ${error.message || error}\n\nPress Cmd+Option+I to open DevTools and check the Console tab for details.`);

    return null;
  }
}

// Create terminal UI for existing SSH session
async function createSSHTerminal(tabId, sshSessionId) {
  console.log('🔌 Creating SSH terminal UI for session:', sshSessionId);

  const sessionId = `ssh-${sshSessionId}`;

  // Create terminal pane (similar to regular terminal)
  const pane = document.createElement('div');
  pane.className = 'terminal-pane warp-style';
  pane.style.flex = '1';
  pane.dataset.sessionId = sessionId;

  // Hidden info for status bar tracking
  const hiddenInfo = document.createElement('div');
  hiddenInfo.style.display = 'none';
  hiddenInfo.innerHTML = `<span class="status-path" id="status-path-${sessionId}">SSH Connection</span>`;
  pane.appendChild(hiddenInfo);

  // Hover close button
  const closeBtn = document.createElement('button');
  closeBtn.className = 'pane-close-btn';
  closeBtn.title = 'Close pane';
  closeBtn.textContent = '×';
  closeBtn.style.cssText = 'position:absolute; top:2px; right:4px; z-index:10; background:rgba(0,0,0,0.5); border:none; color:#6c757d; cursor:pointer; font-size:14px; padding:0 5px; line-height:1.2; border-radius:3px; opacity:0; transition:opacity 0.15s;';
  closeBtn.addEventListener('click', (e) => { e.stopPropagation(); closePane(); });
  pane.appendChild(closeBtn);
  pane.addEventListener('mouseenter', () => { closeBtn.style.opacity = '1'; });
  pane.addEventListener('mouseleave', () => { closeBtn.style.opacity = '0'; });

  // Terminal area
  const terminalDiv = document.createElement('div');
  terminalDiv.className = 'terminal-output';
  terminalDiv.style.flex = '1';
  pane.appendChild(terminalDiv);

  // Append pane to container
  terminalContainer.appendChild(pane);

  // Get appearance settings
  const bgColor = settings.terminalBgColor || '#1e1e1e';
  const textColor = settings.terminalTextColor || '#ffffff';
  const cursorColor = settings.terminalCursorColor || '#3b82f6';
  const opacity = settings.terminalOpacity !== undefined ? settings.terminalOpacity : 100;

  // Create xterm.js terminal
  const term = new Terminal({
    theme: buildTerminalTheme(bgColor, textColor, cursorColor),
    fontFamily: '"SF Mono", Menlo, "JetBrains Mono", "DejaVu Sans Mono", "Fira Code", monospace',
    fontSize: terminalFontSize(),
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    scrollback: 10000,
    allowTransparency: true,
    scrollOnBottom: false,
  });

  term.open(terminalDiv);

  // Apply transparency
  pane.style.background = 'transparent';
  terminalDiv.style.opacity = opacity / 100;

  // Add FitAddon
  const fitAddon = new FitAddon.FitAddon();
  term.loadAddon(fitAddon);

  // Add WebLinksAddon for clickable URLs
  if (window.WebLinksAddon) {
    const webLinksAddon = new WebLinksAddon.WebLinksAddon((_event, uri) => {
      try {
        if (window.__TAURI__?.shell?.open) {
          window.__TAURI__.shell.open(uri);
        } else if (window.__TAURI__) {
          invoke('plugin:shell|open', { path: uri });
        } else {
          window.open(uri, '_blank');
        }
      } catch (e) {
        window.open(uri, '_blank');
      }
    });
    term.loadAddon(webLinksAddon);
  }

  // The channel was opened at a placeholder 80x24; only xterm knows the real
  // size, and only after it has measured the pane.
  const pushSize = () => {
    invoke('resize_ssh', { sessionId: sshSessionId, cols: term.cols, rows: term.rows })
      .catch((error) => console.error('Failed to resize SSH session:', error));
  };

  setTimeout(() => {
    fitAddon.fit();
    pushSize();
  }, 10);

  // Handle resize
  const handleResize = () => {
    if (term && fitAddon) {
      fitAddon.fit();
      pushSize();
    }
  };
  window.addEventListener('resize', handleResize);

  // Listen for SSH output. Base64 on the wire, same contract as terminal-output:
  // atob() alone mangles multi-byte UTF-8, so decode the bytes properly.
  await listen(`ssh-output-${sshSessionId}`, (event) => {
    if (!term) return;
    const binary = atob(event.payload.data);
    const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
    term.write(new TextDecoder('utf-8').decode(bytes));
  });

  // Handle terminal input - send to SSH session
  term.onData(async (data) => {
    try {
      await invoke('write_to_ssh', {
        sessionId: sshSessionId,
        data: data
      });
    } catch (error) {
      console.error('Failed to write to SSH session:', error);
    }
  });

  // Add terminal to tab
  const tab = tabs.find(t => t.id === tabId);
  if (tab) {
    tab.terminals.push({
      sessionId: sshSessionId,
      frontendSessionId: sessionId,
      term,
      pane,
      fitAddon,
      handleResize
    });
  }

  term.focus();
  console.log('✅ SSH terminal UI created');
}

// Tab Management
window.createNewTab = function() {
  const tabId = `tab-${Date.now()}`;
  const tabName = `Terminal ${tabs.length + 1}`;
  const tab = {
    id: tabId,
    name: tabName,
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null
  };

  tab.projectId = tab.projectId || activeProjectId;
  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
}

// Workspace switching (Orca/CMUX): show a project's tabs, creating its default
// tab only the first time. Global panels/terminals live under projectId 'home'.
// Open a terminal running Claude Code against the user's own LLM server
// (Settings → AI Providers) instead of Anthropic. Same harness, different
// backend: LM Studio serves Anthropic's /v1/messages natively, so nothing about
// claude changes — only where it sends the request.
// The + button opens this instead of firing straight into a shell, so agent
// harnesses are launchable without knowing which env vars to set by hand.
// Codex and Pi are deliberately absent: codex ignores OPENAI_BASE_URL and its
// --oss mode wants its own downloaded model, so a "local" entry for it would
// quietly use the cloud. Add them once they're actually verified.
function showNewTabMenu(anchor) {
  document.getElementById('new-tab-menu')?.remove();

  const items = [
    { label: 'New Agent', hint: 'Create a named specialist', run: () => window.xnautOpenNewAgent && window.xnautOpenNewAgent() },
    { label: 'Open Agent Library', hint: 'Agents and their threads', run: () => window.xnautOpenAgentSpace && window.xnautOpenAgentSpace() },
    { label: 'New terminal', hint: 'Your shell', run: () => createNewTab() },
    { label: 'New Project', hint: 'Create a project workspace', run: () => window.xnautSidebarNavigate && window.xnautSidebarNavigate('new-project') },
    { label: 'Claude Code · local model', hint: 'Verified against LM Studio', run: () => window.xnautOpenHarnessLocal('claude') },
    { label: 'Codex · local model', hint: 'Verified — routed via model_provider override', run: () => window.xnautOpenHarnessLocal('codex') },
  ];

  const menu = document.createElement('div');
  menu.id = 'new-tab-menu';
  const r = anchor.getBoundingClientRect();
  menu.style.cssText = `position:fixed; top:${Math.round(r.bottom + 6)}px; left:${Math.round(r.left)}px;
    z-index:10000; min-width:250px; padding:4px;
    background:var(--popover,#171717); color:var(--popover-foreground,#fafafa);
    border:1px solid var(--border,#2a2a2a); border-radius:8px;
    box-shadow:0 12px 32px rgba(0,0,0,.45); font-size:13px;`;

  items.forEach((it) => {
    const row = document.createElement('button');
    row.type = 'button';
    row.style.cssText = `display:block; width:100%; text-align:left; padding:8px 10px;
      background:transparent; border:0; border-radius:6px; color:inherit; cursor:pointer; font:inherit;`;
    row.innerHTML = `<div>${it.label}</div><div style="opacity:.6; font-size:11px; margin-top:2px;">${it.hint}</div>`;
    row.onmouseenter = () => { row.style.background = 'rgba(255,255,255,.07)'; };
    row.onmouseleave = () => { row.style.background = 'transparent'; };
    row.onclick = () => { menu.remove(); it.run(); };
    menu.appendChild(row);
  });

  document.body.appendChild(menu);
  // Defer so the click that opened the menu doesn't immediately close it.
  setTimeout(() => {
    const close = (ev) => {
      if (menu.contains(ev.target)) return;
      menu.remove();
      document.removeEventListener('mousedown', close);
    };
    document.addEventListener('mousedown', close);
  }, 0);
}

// Each harness reaches a local server its own way. Only claude is verified:
// LM Studio implements Anthropic's /v1/messages natively, so the harness is
// unchanged and only the destination differs.
//
// codex ignores OPENAI_BASE_URL entirely (proven — pointed at a dead port it
// answered normally, from the cloud), so it needs a provider override, and
// current versions reject wire_api="chat" and demand "responses". pi has no
// base-URL flag but its openai provider follows the usual SDK env convention.
// Both stay marked experimental until tested against a live server.
const LOCAL_HARNESSES = {
  claude: {
    label: 'Claude',
    cmd: () => 'exec claude',
    env: (base, model) => {
      // The key is required as *an* auth source; the server ignores its value.
      // Without the model, claude asks for a claude-* the server cannot serve.
      const e = { ANTHROPIC_BASE_URL: base, ANTHROPIC_API_KEY: 'local' };
      if (model) e.ANTHROPIC_MODEL = model;
      return e;
    },
  },
  codex: {
    label: 'Codex',
    cmd: (base, model, endpoint) => {
      const q = (s) => String(s).replace(/"/g, '\\"');
      // endpoint verbatim — whatever port or path the user configured.
      const prov = `model_providers.lms={name="Local",base_url="${q(endpoint)}",wire_api="responses"}`;
      return `exec codex -c '${prov}' -c model_provider=lms${model ? ` -c model="${q(model)}"` : ''}`;
    },
    env: () => ({}),
  },
  pi: {
    label: 'Pi',
    // pi keeps its own provider table in ~/.pi/agent/models.json (baseUrl +
    // model ids per provider) and takes --provider by name. It has no
    // base-URL flag, so pointing it anywhere means naming a provider that
    // already exists there — resolved at launch, never hardcoded.
    resolve: async (base, model, endpoint) => {
      // pi has no base-URL flag — it only takes --provider by name, resolved
      // from its own table. So we keep one entry in that table, PI_PROVIDER,
      // generated from Settings → AI Providers on every launch. The endpoint
      // and model are never written down anywhere else; change them in
      // Settings and the next launch follows. The user's own providers are
      // left exactly as they are.
      const PI_PROVIDER = 'xnaut-local';
      let home, cfg;
      try {
        home = await invoke('get_home_directory');
        const raw = await invoke('read_file', { path: `${home}/.pi/agent/models.json` });
        cfg = JSON.parse(typeof raw === 'string' ? raw : (raw?.content || '')) || {};
      } catch (_) {
        cfg = {};
      }
      cfg.providers = cfg.providers || {};
      cfg.providers[PI_PROVIDER] = {
        baseUrl: endpoint,
        apiKey: 'local',
        models: model ? [{ id: model }] : [],
      };
      try {
        await invoke('write_file', {
          path: `${home}/.pi/agent/models.json`,
          content: JSON.stringify(cfg, null, 2),
        });
      } catch (e) {
        return { error: `could not update pi's provider config: ${e}` };
      }
      return {
        cmd: `exec pi --provider ${PI_PROVIDER}${model ? ` --model ${JSON.stringify(model)}` : ''}`,
        env: {},
      };
    },
  },
};

window.xnautOpenHarnessLocal = async function (which) {
  const h = LOCAL_HARNESSES[which];
  if (!h) return;
  let s;
  try {
    s = await invoke('settings_get');
  } catch (e) {
    console.error('settings unavailable:', e);
    return;
  }
  const endpoint = (s?.llm?.endpoint || '').trim();
  if (!endpoint) {
    if (statusText) statusText.textContent = 'Set a local endpoint in Settings → AI Providers first';
    return;
  }
  // Settings stores the OpenAI-style endpoint; claude appends its own /v1, so
  // the table works from the origin and re-adds /v1 where a harness needs it.
  const base = endpoint.replace(/\/+$/, '').replace(/\/v1$/, '');
  const model = (s.llm.model || '').trim();

  // Harnesses that keep their own provider config (pi) resolve at launch;
  // the rest build a command + env from the settings endpoint.
  let cmd, env;
  if (h.resolve) {
    const r = await h.resolve(base, model, endpoint);
    if (r.error) {
      console.error(`${h.label} (local):`, r.error);
      if (statusText) statusText.textContent = `${h.label}: ${r.error}`;
      return;
    }
    ({ cmd, env } = r);
  } else {
    cmd = h.cmd(base, model, endpoint);
    env = h.env(base, model, endpoint);
  }

  // Ask the server what it actually has. This catches both ways this goes
  // wrong: an unreachable endpoint (claude would retry a refused socket in
  // silence and look hung), and a model named in Settings that the server does
  // not have loaded — which comes back as a bare HTTP 400 mid-session and
  // explains nothing. Whatever is loaded changes independently of Settings.
  {
    try {
      const available = await invoke('chat_list_models');
      if (!available || !available.length) throw new Error('no models');
      if (model && !available.includes(model)) {
        const msg = `"${model}" is not loaded on ${endpoint} — available: ${available.join(', ')}`;
        console.error(msg);
        if (statusText) statusText.textContent = msg;
        return;
      }
    } catch (e) {
      const msg = `No LLM server at ${endpoint} — check Settings → AI Providers`;
      console.error(msg, e);
      if (statusText) statusText.textContent = msg;
      return;
    }
  }

  try {
    const result = await invoke('create_command_session', {
      config: {
        // Through a LOGIN shell on purpose. claude lives in ~/.local/bin, which
        // is not on the PATH a bundled .app inherits from launchd — spawning it
        // directly works in `cargo tauri dev` and fails once installed. The env
        // below is set on the process, so exec keeps it.
        program: 'zsh',
        args: ['-lc', cmd],
        // Empty string is not a valid cwd and the backend sets it
        // unconditionally, so spawn fails outright. '~/' expands to home.
        workingDir: activeProjectPath || '~/',
        env,
      },
    });
    window.xnautAttachAgentTab(result.session_id, `${h.label} · local`);
  } catch (e) {
    console.error(`${h.label} (local) failed to start:`, e);
    if (statusText) statusText.textContent = `${h.label} (local) failed: ${e}`;
  }
};

window.xnautSetActiveProject = async function (projectId, task) {
  projectId = projectId || 'home';
  activeProjectId = projectId;
  activeProjectPath = (projectId === 'home') ? null : (task?.path || activeProjectPath);
  if (window.xnautSidebarSetActiveProject) {
    window.xnautSidebarSetActiveProject(projectId === 'home' ? null : projectId);
  }

  const own = tabs.filter(t => (t.projectId || 'home') === projectId);
  // An explicitly requested session wins over "this project already has a tab".
  // A project can have one session per agent (cl-Bucky and cx-Bucky); without
  // this, whichever tab was opened first captured every later click and the
  // chosen session was silently discarded.
  const wantSession = task && task.zellij_session;
  if (own.length && !wantSession) {
    const want = activeTabByProject[projectId];
    const target = (want && own.some(t => t.id === want)) ? want : own[own.length - 1].id;
    renderTabs();
    switchTab(target);
    if (task && task.path && window.xnautRightPaneSetRoot) window.xnautRightPaneSetRoot(task.path);
    return;
  }
  if (wantSession) {
    // Already attached to this exact session? Go back to that tab.
    const existing = own.find(t => t.zellijSession === wantSession);
    if (existing) {
      renderTabs();
      switchTab(existing.id);
      if (task && task.path && window.xnautRightPaneSetRoot) window.xnautRightPaneSetRoot(task.path);
      return;
    }
  }

  if (projectId === 'home') { renderTabs(); return; }

  // First open of this project — create its default tab in the project folder.
  if (task && task.zellij_session) {
    try {
      const esc = (s) => "'" + String(s).replace(/'/g, "'\\''") + "'";
      const result = await invoke('create_command_session', {
        // The PATH export is load-bearing: a launchd-launched app hands sh
        // /usr/bin:/bin only, where zellij does not exist, and the attach died
        // instantly leaving a black terminal with a blinking cursor while the
        // session it was meant to open ran on untouched. Same pattern as the
        // Observatory's delete and the PM panel's startShell.
        config: { program: 'sh', args: ['-c', `export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"; exec zellij attach --create ${esc(task.zellij_session)}`], workingDir: task.path || null },
      });
      window.xnautAttachAgentTab(result.session_id, task.zellij_session, task.zellij_session);
    } catch (e) {
      console.error('zellij attach failed, opening plain terminal:', e);
      openProjectTerminal(projectId, task);
    }
  } else {
    openProjectTerminal(projectId, task);
  }
  if (task && task.path && window.xnautRightPaneSetRoot) window.xnautRightPaneSetRoot(task.path);
};

// Plain terminal tab cd'd into a project's folder.
function openProjectTerminal(projectId, task) {
  const tabId = `tab-${Date.now()}`;
  const tab = {
    id: tabId,
    name: (task && task.name) || 'Terminal',
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null,
    initialCwd: (task && task.path) || null,
    projectId,
  };
  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
}

// True if a project has ≥1 open tab in this session (drives the sidebar dot).
// Real agent status per project, for the sidebar dot.
//
// The only trustworthy source is agent_sessions_list — the seven states fed by
// the agent hooks. Zellij's own last-activity timestamp is useless for this:
// the resurrection cache is rewritten about once a second whether the agent is
// working or sitting idle, so it says "alive", never "busy".
//
// Consequence: a session xNAUT did not launch has no status, and the dot must
// say so rather than inventing a spinner.
const XNAUT_AGENT_STATUS = new Map(); // pty session_id -> status string
window.xnautProjectAgentStatus = function (projectId) {
  const own = (tabs || []).filter((t) => (t.projectId || 'home') === projectId);
  if (!own.length) return null;
  const seen = own
    .map((t) => t.agentSessionId && XNAUT_AGENT_STATUS.get(t.agentSessionId))
    .filter(Boolean);
  if (!seen.length) return null;
  // Loudest wins: something needing a human beats something merely running.
  // 'unknown' sits last on purpose: any row that can actually say something
  // outranks a row that cannot. The sidebar renders it as "alive, not busy",
  // which is the same claim.
  for (const s of ['permission', 'blocked', 'waiting', 'working', 'done', 'interrupted', 'idle', 'unknown']) {
    if (seen.includes(s)) return s;
  }
  return seen[0];
};

async function pollAgentStatus() {
  try {
    const list = (await invoke('agent_sessions_list')) || [];
    XNAUT_AGENT_STATUS.clear();
    for (const s of list) {
      if (s && s.session_id) XNAUT_AGENT_STATUS.set(s.session_id, String(s.status || '').toLowerCase());
    }
    if (window.xnautSidebarRefreshDots) window.xnautSidebarRefreshDots();

    // Feed the tab-dot map from the same fetch.
    //
    // status-strip.js loads that map ONCE and then only updates it from
    // agent-status-changed events. Its retry fires on an exception, so a call
    // that SUCCEEDS with an empty list is never retried, and on a cold start
    // adoption emits its events before the webview has a listener. The result
    // seen on the rig 2026-09-02: fourteen live agent sessions in the backend,
    // nine tabs on screen, and not one status dot, because the map was size 0
    // and the dot code removes a dot it cannot resolve.
    //
    // An empty success looking exactly like a correct empty state is the same
    // failure this app keeps paying for, so the fix is to stop relying on a
    // single load: this poll already has the list every 3 seconds.
    const dotMap = window.xnautAgentSessions;
    if (dotMap) {
      dotMap.clear();
      for (const session of list) {
        if (session && session.session_id) dotMap.set(session.session_id, session);
      }
      if (window.xnautRefreshTabAgentDots) window.xnautRefreshTabAgentDots();
    }

    for (const session of list) showAgentSession(session);
  } catch (_) { /* backend not up yet — try again next tick */ }
}
setInterval(pollAgentStatus, 3000);

// An agent nobody can see is an agent nobody believes in.
//
// A woken agent launches backend-side on purpose: the wake path came from the
// mobile bridge, where a PTY must not need a pane. On the desktop that turned
// into a hole. On 2026-09-01 an agent ran for an hour on the rig with a live
// PTY, a live zellij session and a tracked "working" status, while the app in
// front of it showed two unrelated shell tabs. agent_sessions_list was only
// ever read to COLOUR DOTS on tabs that already existed, so a session no tab
// held rendered nowhere at all. "Opened xNAUT, nothing more."
//
// So every tracked agent session gets a tab. Deliberately without focus: this
// fires from a poll and from wakes the owner did not initiate, and stealing
// the active tab mid-keystroke is its own bug. A pill appears in the strip;
// clicking it is the owner's move.
// Zellij sessions already given a tab. The poll runs every 3s and attaching
// is async, so without this the same adopted session would spawn a tab per
// tick until the first one finished registering.
const SURFACED_ZELLIJ = new Set();
function showAgentSession(session) {
  // 2026-09-15 (XNAUT-402): tracked sessions no longer get a tab each. The
  // sidebar's Sessions list shows every one of them with its status and a
  // badge on the rail, and a tab opens when the owner clicks. The strip had
  // grown a tab per adopted session on every restart until the controls on
  // its right were pushed off the screen (André: "if there are more it will
  // push the top menu out of the screen"). The visibility argument above is
  // still true; the list answers it now, the strip does not have to.
  void session;
}

// Focus the tab already attached to this zellij session, if there is one.
// Without it every click on Connect spawns another PTY onto the SAME session —
// three clicks, three tabs, three status pills, one actual session.
window.xnautFocusTabForSession = function (zellijSession) {
  if (!zellijSession) return false;
  const tab = (tabs || []).find((t) => t.zellijSession === zellijSession);
  if (!tab) return false;
  switchTab(tab.id);
  return true;
};

// Close whatever tab is attached to a zellij session — used after the session
// is deleted, so its tab (and status pill) go with it instead of lingering as a
// pill pointing at something that no longer exists.
window.xnautCloseTabForSession = function (zellijSession) {
  if (!zellijSession) return false;
  const tab = (tabs || []).find((t) => t.zellijSession === zellijSession);
  if (!tab) return false;
  closeTab(tab.id);
  return true;
};

// The project the workspace is currently on, or null on Home. Read-only —
// panels that scope a setting per project need this, and the roster panel was
// the first to want it.
// The active project's local path, for anything mounted after the project was
// opened. The right pane needs it: a pane opened later used to start with a
// null root, so Git and the worktree manager said "No folder open" for a
// project the app was visibly displaying (XNAUT-259, found by the tron rig).
// The focused terminal's backend session id. get_current_directory now
// REQUIRES one, and only app.js was updated when it did (XNAUT-259b): three
// other call sites still ask with no argument and swallow the rejection.
window.xnautFocusedSessionId = function () {
  const tab = tabs.find((t) => t.id === activeTabId);
  const term = tab && tab.terminals && tab.terminals[tab.focusedPaneIndex || 0];
  return (term && term.sessionId) || null;
};

window.xnautActiveProjectPath = function () {
  return activeProjectPath || null;
};

window.xnautActiveProjectKey = function () {
  return activeProjectId && activeProjectId !== 'home' ? activeProjectId : null;
};

window.xnautProjectHasTabs = function (projectId) {
  return tabs.some(t => (t.projectId || 'home') === projectId);
};

// Enter the Home workspace context (used before attaching a global panel like
// Tasks/Automations/PM) without forcing a tab switch — the attach handles that.
window.xnautHomeContext = function () {
  activeProjectId = 'home';
  activeProjectPath = null;
  if (window.xnautSidebarSetActiveProject) window.xnautSidebarSetActiveProject(null);
};

// Switches focus to the tab carrying the given agent session, if any.
// Used by the Phase 4 status strip to make pill-clicks navigate.
window.xnautFocusAgentSession = function (sessionId) {
  const tab = tabs.find((t) => t.agentSessionId === sessionId);
  if (tab) switchTab(tab.id);
  return !!tab;
};

// Create a new tab that hosts a single diff pane (Phase 8a). Shows a
// worktree's git diff with inline annotations from <worktree>/.xnaut/notes.json.
window.xnautAttachDiffTab = async function (opts) {
  const tabId = `tab-${Date.now()}`;
  const tab = {
    id: tabId,
    name: (opts && opts.name) || 'Diff',
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null,
    isDiff: true,
    initialDiffOpts: opts || {},
  };
  tab.projectId = tab.projectId || activeProjectId;
  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
  return tabId;
};

// Create a new tab that hosts a single markdown pane (Phase 7). The pane
// is a TipTap editor instance — see markdown-pane.js. Returns the new tab id.
window.xnautAttachMarkdownTab = async function (opts) {
  const tabId = `tab-${Date.now()}`;
  const tab = {
    id: tabId,
    name: (opts && opts.filename) || 'Markdown',
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null,
    isMarkdown: true,
    initialMarkdownOpts: opts || null,
  };
  tab.projectId = tab.projectId || activeProjectId;
  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
  return tabId;
};

// Create a new tab that hosts a single browser pane (Phase 6). The pane is
// a native Tauri child webview that floats over a placeholder div — see
// browser-pane.js. Returns the new tab id.
window.xnautAttachBrowserTab = async function (initialUrl) {
  const tabId = `tab-${Date.now()}`;
  const tab = {
    id: tabId,
    name: 'Browser',
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null,
    isBrowser: true,
    initialBrowserUrl: initialUrl || null,
  };
  tab.projectId = tab.projectId || activeProjectId;
  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
  return tabId;
};

// Create a new tab hosting a single vault knowledge-graph pane (the orb).
window.xnautAttachGraphTab = async function (opts) {
  const tabId = `tab-${Date.now()}`;
  const tab = {
    id: tabId,
    name: 'Graph',
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null,
    isGraph: true,
    initialGraphOpts: opts || null,
  };
  tab.projectId = tab.projectId || activeProjectId;
  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
  return tabId;
};
function openGraphPane(opts) { return window.xnautAttachGraphTab(opts || {}); }

// Attach a new tab to an existing backend PTY session (used by the agent
// launcher, mirrors the SSH-session pattern). The tab's createTerminal
// call sees tab.agentSessionId and skips create_terminal_session.
window.xnautAttachAgentTab = function (sessionId, label, zellijSession, options) {
  // focus defaults to true so every existing caller behaves exactly as before;
  // only the automatic surfacing of a woken agent opts out.
  const focus = !options || options.focus !== false;
  // One tab per zellij session, enforced HERE rather than in each caller.
  // Every caller was expected to check first; one that forgot (or a double
  // click racing itself) produced a second pill on the same session, and the
  // strip filled up with duplicates of the same name.
  if (zellijSession) {
    const existing = (tabs || []).find((t) => t.zellijSession === zellijSession);
    if (existing) {
      if (focus) switchTab(existing.id);
      return existing.id;
    }
  }
  const tabId = `tab-${Date.now()}`;
  const tab = {
    id: tabId,
    // Which zellij session this tab is attached to, so clicking the project
    // again returns here instead of opening a second tab on the same session.
    zellijSession: zellijSession || null,
    // The one tab the sidebar's Sessions list swaps its selection into
    // (xnautShowSessionInHost). Never more than one, never a tab per session.
    sessionsHost: !!(options && options.host),
    name: (zellijSession && window.xnautSessionAlias(zellijSession)) || label || `Agent ${tabs.length + 1}`,
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null,
    agentSessionId: sessionId,
  };
  tab.projectId = tab.projectId || activeProjectId;
  tabs.push(tab);
  renderTabs();
  if (focus) switchTab(tabId);
  return tabId;
};

// A resize makes zellij reflow its scrollback and park the viewport above the
// bottom (SCROLL: 126/603 in the pane frame); the newest output looks gone
// until a key is pressed. After any resize of a zellij tab, ask zellij to
// scroll to the bottom, debounced so a drag or a zoom sends one request.
const zellijSettleTimers = new Map();
window.xnautZellijSettle = function (ptySessionId) {
  if (!ptySessionId) return;
  const tab = (tabs || []).find((t) => t.zellijSession && (t.terminals || []).some((x) => x.sessionId === ptySessionId));
  if (!tab) return;
  clearTimeout(zellijSettleTimers.get(tab.zellijSession));
  zellijSettleTimers.set(tab.zellijSession, setTimeout(() => {
    invoke('zellij_scroll_to_bottom', { name: tab.zellijSession }).catch(() => {});
  }, 250));
};

// The Sessions list is the switcher (André, 2026-09-15: "the session is
// added as tab on top, not on the left side"). One host tab in the strip
// shows whichever session the sidebar selected; selecting another detaches
// the previous (zellij keeps it running) and attaches the new one in the
// same place. No tab per session, ever.
window.xnautShowSessionInHost = async function (name) {
  const wanted = String(name || '');
  if (!wanted) return null;
  const host = (tabs || []).find((t) => t.sessionsHost);
  if (host && host.zellijSession === wanted) { switchTab(host.id); return host.id; }
  // The size the pane will have, taken from the host being replaced (or the
  // last fitted terminal). A PTY spawned at the 120x40 default and resized
  // a moment later makes zellij reflow its scrollback, and the viewport
  // came back 126 lines above the bottom: the latest output looked missing
  // (André's recording, 2026-09-16 10:57).
  const sized = host && host.terminals && host.terminals[0] && host.terminals[0].term
    ? { cols: host.terminals[0].term.cols, rows: host.terminals[0].term.rows }
    : (window.xnautLastTermSize || {});
  if (host) await closeTab(host.id);
  if (typeof window.xnautOpenZellijSession !== 'function') return null;
  return window.xnautOpenZellijSession(wanted, { focus: true, host: true, cols: sized.cols, rows: sized.rows });
};

// Return to an already attached identity-aware agent session. Agent Space uses
// this for its Terminal action and the quick pane preview uses the same source
// of truth, so neither feature creates a duplicate PTY or terminal tab.
window.xnautOpenAgentSession = function (sessionId, label) {
  const existing = (tabs || []).find((tab) => tab.agentSessionId === sessionId);
  if (existing) {
    switchTab(existing.id);
    return true;
  }
  if (!sessionId || !window.xnautAttachAgentTab) return false;
  window.xnautAttachAgentTab(sessionId, label || 'Agent terminal');
  return true;
};

window.xnautAgentSessionPreview = function (sessionId, maxLines) {
  const existing = (tabs || []).find((tab) => tab.agentSessionId === sessionId);
  const terminal = existing && existing.terminals && existing.terminals[0];
  const buffer = terminal && terminal.term && terminal.term.buffer && terminal.term.buffer.active;
  if (!buffer) return [];
  const count = Math.max(1, Math.min(Number(maxLines) || 12, 30));
  const start = Math.max(0, buffer.length - count);
  const lines = [];
  for (let index = start; index < buffer.length; index += 1) {
    const line = buffer.getLine(index);
    if (!line) continue;
    const value = line.translateToString(true).trimEnd();
    if (value) lines.push(value);
  }
  return lines.slice(-count);
};

// Push text into the ACTIVE terminal's agent (Workspace → "Push to terminal").
// Types the text into the PTY; the user presses Enter to send it (no auto-submit,
// so nothing fires into a running agent by surprise). Returns false if no terminal.
window.xnautPushToTerminal = function (text) {
  if (!text || !text.trim()) return false;
  const tab = tabs.find((t) => t.id === activeTabId);
  const term = tab && tab.terminals && tab.terminals[tab.focusedPaneIndex || 0];
  const sid = term && term.sessionId;
  if (!sid) return false;
  invoke('write_to_terminal', { sessionId: sid, data: text }).catch((e) => console.error('[push-to-terminal]', e));
  if (term.term && term.term.focus) term.term.focus();
  return true;
};

// Create a new tab hosting a generic DOM panel (Tasks Mode v1.6 — chat,
// forge tasks, automations). `factory` is the name of a window.* function
// with the (tabId, parentContainer, opts) -> entry pane contract.
window.xnautAttachPanelTab = function (name, factory, opts) {
  const tabId = `tab-${Date.now()}`;
  const tab = {
    id: tabId,
    name: name || 'Panel',
    terminals: [],
    focusedPaneIndex: 0,
    layoutType: 'single',
    colSizes: null,
    rowSizes: null,
    isPanel: true,
    panelFactory: factory,
    panelOpts: opts || null,
  };
  tab.projectId = tab.projectId || activeProjectId;
  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
  return tabId;
};

// Global product surfaces are destinations, not disposable documents. Reuse a
// matching panel in the active workspace and let its controller react to new
// options when it supports updateOptions().
window.xnautAttachSingletonPanelTab = function (name, factory, opts) {
  const workspace = activeProjectId || 'home';
  const existing = (tabs || []).find((tab) =>
    tab.isPanel && tab.panelFactory === factory && (tab.projectId || 'home') === workspace
  );
  if (existing) {
    existing.panelOpts = opts || {};
    const entry = existing.terminals && existing.terminals[0];
    if (entry && typeof entry.updateOptions === 'function') entry.updateOptions(existing.panelOpts);
    switchTab(existing.id);
    return existing.id;
  }
  return window.xnautAttachPanelTab(name, factory, opts || {});
};

const TAB_NAMES_KEY = 'xnaut.tabNames';
// Tracks last click timestamp per tab id for manual double-click detection.
// Survives renderTabs() rebuilds because it lives at module scope, not on the DOM.
const lastTabClick = {};

// A session's display name, keyed by its zellij name rather than a tab id,
// so it survives the host tab being rebuilt on every switch and shows on the
// sidebar row too (André, 2026-09-15: "I just renamed the top tab to xNaut,
// the left version stays the same, and after I clicked into another session
// and back it was again Cand0rian").
const SESSION_NAMES_KEY = 'xnaut-session-names';
function loadSessionNames() {
  try { return JSON.parse(localStorage.getItem(SESSION_NAMES_KEY) || '{}'); } catch (_) { return {}; }
}
window.xnautSessionAlias = function (session) {
  return (session && loadSessionNames()[session]) || '';
};
window.xnautRenameSession = function (session, alias) {
  if (!session) return;
  const names = loadSessionNames();
  const clean = String(alias || '').trim();
  if (clean && clean !== session) names[session] = clean; else delete names[session];
  try { localStorage.setItem(SESSION_NAMES_KEY, JSON.stringify(names)); } catch (_) {}
  for (const tab of tabs || []) {
    if (tab.zellijSession === session) tab.name = clean || session;
  }
  renderTabs();
  if (typeof window.xnautSidebarRefresh === 'function') window.xnautSidebarRefresh();
};

function loadTabNames() {
  try {
    return JSON.parse(localStorage.getItem(TAB_NAMES_KEY) || '{}');
  } catch (_) {
    return {};
  }
}

function saveTabName(tabId, name) {
  const names = loadTabNames();
  if (name && name !== `Terminal ${tabs.findIndex(t => t.id === tabId) + 1}`) {
    names[tabId] = name;
  } else {
    delete names[tabId];
  }
  localStorage.setItem(TAB_NAMES_KEY, JSON.stringify(names));
}

function startTabRename(tabEl, tab) {
  const span = tabEl.querySelector('.tab-name');
  if (!span || span.dataset.editing === '1') return;
  span.dataset.editing = '1';

  const input = document.createElement('input');
  input.type = 'text';
  input.value = tab.name;
  input.className = 'tab-name-input';
  input.style.cssText = 'background:transparent; border:none; outline:none; color:inherit; font:inherit; width:120px; padding:0;';
  span.replaceWith(input);
  input.focus();
  input.select();

  const commit = () => {
    const newName = input.value.trim() || tab.name;
    tab.name = newName;
    // A session tab's name belongs to the session, not to this tab's id.
    if (tab.zellijSession) { window.xnautRenameSession(tab.zellijSession, newName); return; }
    saveTabName(tab.id, newName);
    renderTabs();
  };
  const cancel = () => renderTabs();

  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); commit(); }
    else if (e.key === 'Escape') { e.preventDefault(); cancel(); }
  });
  input.addEventListener('blur', commit);
}

function renderTabs() {
  tabsContainer.innerHTML = '';
  const savedNames = loadTabNames();
  // Only show tabs belonging to the active project workspace (Orca/CMUX model).
  tabs.filter(t => (t.projectId || 'home') === activeProjectId).forEach(tab => {
    if (savedNames[tab.id]) tab.name = savedNames[tab.id];

    const tabEl = document.createElement('div');
    tabEl.className = `tab ${tab.id === activeTabId ? 'active' : ''}`;
    tabEl.dataset.sessionId = tab.id;
    // Who drives the tab: an agent xNAUT launched or adopted (blue), or a
    // session the owner opened himself (yellow). André, 2026-09-15: "so we
    // know they are xNaut driven agents".
    // A zellij tab is judged by the session's name (his own are attached
    // through the same PTY path, so agentSessionId alone says nothing).
    if (tab.zellijSession) tabEl.classList.add(/^xnaut-/.test(tab.zellijSession) ? 'tab-auto' : 'tab-manual');
    else if (tab.agentSessionId) tabEl.classList.add('tab-auto');

    // Add backendSessionId if available
    if (tab.terminals && tab.terminals.length > 0) {
      tabEl.dataset.backendSessionId = tab.terminals[0].sessionId;
    }
    // Agent tabs carry their session id so terminal-agent-status.js can show a
    // provider mark + a working/done status dot on the tab.
    if (tab.agentSessionId) tabEl.dataset.agentSessionId = tab.agentSessionId;
    // An agent can have TWO rows in the status map, and only one of them is
    // alive. The `zellij attach` shell we open for an adopted session registers
    // its own PTY row, keyed by a uuid and carrying the real status; the
    // original adopted row stays keyed by the zellij session NAME (status.rs
    // adopt_orphans) and goes stale. Binding the tab to the NAME was my first
    // fix and it was backwards: every tab then read `idle` off the stale row
    // while /api/sessions said `working` off the live one. So prefer the PTY
    // row, and fall back to the name only when the map does not know the PTY.
    const agentRows = window.xnautAgentSessions;
    if (agentRows && !agentRows.has(tab.agentSessionId) && tab.zellijSession
        && agentRows.has(tab.zellijSession)) {
      tabEl.dataset.agentSessionId = tab.zellijSession;
    }

    const nameSpan = document.createElement('span');
    nameSpan.className = 'tab-name';
    nameSpan.textContent = tab.name;
    nameSpan.title = 'Auto-named from cwd / git branch · click twice on the active tab to rename';

    const closeBtn = document.createElement('button');
    closeBtn.className = 'tab-close';
    closeBtn.dataset.tabId = tab.id;
    closeBtn.textContent = '×';

    tabEl.appendChild(nameSpan);
    tabEl.appendChild(closeBtn);

    // Handle tab switching + manual double-click detection.
    // Browser dblclick doesn't work because switchTab() rebuilds the DOM,
    // so the second click lands on a fresh element.
    tabEl.addEventListener('click', (e) => {
      if (e.target.classList.contains('tab-close') ||
          e.target.classList.contains('tab-name-input')) return;

      const isNameClick = e.target.classList.contains('tab-name');
      const isActive = tab.id === activeTabId;
      const now = Date.now();
      const lastClick = lastTabClick[tab.id] || 0;
      lastTabClick[tab.id] = now;

      // Two clicks within 400ms on the focused tab's name → rename
      if (isNameClick && isActive && (now - lastClick) < 400) {
        startTabRename(tabEl, tab);
        return;
      }
      switchTab(tab.id);
    });

    // Handle close button
    closeBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeTab(tab.id);
    });

    tabsContainer.appendChild(tabEl);
  });
  if (window.xnautRefreshTabAgentDots) window.xnautRefreshTabAgentDots();
}

window.xnautSwitchTab = (id) => switchTab(id); // browser-pane.js focuses an existing browser tab (XNAUT-149)
async function switchTab(tabId) {
  activeTabId = tabId;
  // Remember the active tab per workspace so re-selecting a project restores it.
  const _st = tabs.find(t => t.id === tabId);
  if (_st) activeTabByProject[_st.projectId || 'home'] = tabId;
  // Clear container safely (only our own pane elements)
  while (terminalContainer.firstChild) {
    terminalContainer.removeChild(terminalContainer.firstChild);
  }
  removeSplitDividers();
  // Reset container to flex (applyLayout will set grid if needed)
  terminalContainer.classList.remove('grid-mode');
  terminalContainer.style.display = 'flex';
  terminalContainer.style.gridTemplateColumns = '';
  terminalContainer.style.gridTemplateRows = '';
  terminalContainer.style.gridTemplateAreas = '';

  const tab = tabs.find(t => t.id === tabId);
  if (tab) {
    if (tab.terminals.length === 0) {
      // Generic panel tab (Tasks Mode v1.6) — factory name lives on the tab.
      if (tab.isPanel && typeof window[tab.panelFactory] === 'function') {
        try {
          const entry = await window[tab.panelFactory](tabId, terminalContainer, tab.panelOpts || {});
          if (entry) tab.terminals.push(entry);
        } catch (e) {
          console.error('Failed to create panel pane:', e);
        }
      }
      // Markdown tab — first pane is a TipTap editor.
      else if (tab.isMarkdown && typeof window.xnautCreateMarkdownPane === 'function') {
        try {
          const entry = await window.xnautCreateMarkdownPane(tabId, terminalContainer, tab.initialMarkdownOpts || {});
          if (entry) tab.terminals.push(entry);
        } catch (e) {
          console.error('Failed to create markdown pane:', e);
        }
      }
      // Graph tab — vault knowledge-graph orb.
      else if (tab.isGraph && typeof window.xnautCreateGraphPane === 'function') {
        try {
          const entry = await window.xnautCreateGraphPane(tabId, terminalContainer, tab.initialGraphOpts || {});
          if (entry) tab.terminals.push(entry);
        } catch (e) {
          console.error('Failed to create graph pane:', e);
        }
      }
      // Diff tab — git diff viewer with inline notes.
      else if (tab.isDiff && typeof window.xnautCreateDiffPane === 'function') {
        try {
          const entry = await window.xnautCreateDiffPane(tabId, terminalContainer, tab.initialDiffOpts || {});
          if (entry) tab.terminals.push(entry);
        } catch (e) {
          console.error('Failed to create diff pane:', e);
        }
      }
      // Browser tab — first pane is a browser, not a terminal.
      else if (tab.isBrowser && typeof window.xnautCreateBrowserPane === 'function') {
        try {
          const entry = await window.xnautCreateBrowserPane(tabId, terminalContainer, tab.initialBrowserUrl);
          tab.terminals.push(entry);
        } catch (e) {
          console.error('Failed to create browser pane:', e);
        }
      }
      // Check if this is an SSH tab that needs a terminal UI
      else if (tab.isSSH && tab.sshSessionId) {
        console.log('📡 Creating terminal UI for SSH session:', tab.sshSessionId);
        await createSSHTerminal(tabId, tab.sshSessionId);
      } else {
        await createTerminal(tabId, undefined, undefined, tab.initialCwd);
      }
    } else {
      tab.terminals.forEach(terminal => {
        terminalContainer.appendChild(terminal.pane);
      });

      // Apply the saved layout (handles grid, fit, dividers, focus)
      applyLayout(tab);

      // Focus the previously focused pane
      const focusedTerminal = tab.terminals[tab.focusedPaneIndex || 0];
      if (focusedTerminal) {
        if (focusedTerminal.term) focusedTerminal.term.focus();
        // Refresh shared status bar with the focused pane's cwd/git info
        const fid = focusedTerminal.frontendSessionId || (focusedTerminal.pane && focusedTerminal.pane.dataset && focusedTerminal.pane.dataset.sessionId);
        if (fid) updateSharedStatusBar(fid);
      }
    }
  }

  renderTabs();
  // Phase 6: let browser-pane.js know which webviews to show/hide.
  if (typeof window.xnautOnTabSwitched === 'function') {
    window.xnautOnTabSwitched(tabId);
  }
}

// Zellij session name for a tab. Derived from the tab id, which is stable for
// the life of the tab, so reattaching is just asking for the same name again.
// Kept to [a-z0-9-] because zellij session names end up in shell commands.
function zellijNameForTab(tabId) {
  return 'xnaut-' + String(tabId || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
}

async function closeTab(tabId) {
  console.log('🗑️ Closing tab:', tabId);
  const tabIndex = tabs.findIndex(t => t.id === tabId);

  if (tabIndex === -1) {
    console.error('❌ Tab not found:', tabId);
    return;
  }

  const tab = tabs[tabIndex];
  console.log('✅ Found tab to close:', tab.name, 'with', tab.terminals.length, 'terminals');

  // Close all terminals in tab
  for (const terminal of tab.terminals) {
    try {
      if (tab.isPanel && terminal && !terminal.sessionId) {
        if (typeof terminal.dispose === 'function') terminal.dispose();
        continue;
      }
      // Phase 6: browser panes don't have a PTY — destroy via browser API.
      if (terminal && terminal.kind === 'browser' && terminal.label) {
        if (typeof window.xnautDestroyBrowserPane === 'function') {
          await window.xnautDestroyBrowserPane(terminal.label);
        }
        continue;
      }
      // Phase 7: markdown panes — destroy via markdown API.
      if (terminal && terminal.kind === 'markdown' && terminal.label) {
        if (typeof window.xnautDestroyMarkdownPane === 'function') {
          await window.xnautDestroyMarkdownPane(terminal.label);
        }
        continue;
      }
      // Vault graph panes — destroy via graph API.
      if (terminal && terminal.kind === 'graph' && terminal.label) {
        if (typeof window.xnautDestroyGraphPane === 'function') {
          window.xnautDestroyGraphPane(terminal.label);
        }
        continue;
      }
      // Phase 8a: diff panes — destroy via diff API.
      if (terminal && terminal.kind === 'diff' && terminal.label) {
        if (typeof window.xnautDestroyDiffPane === 'function') {
          await window.xnautDestroyDiffPane(terminal.label);
        }
        continue;
      }
      if (terminal && terminal.kind === 'agents') {
        continue;
      }
      console.log('  🔌 Closing terminal session:', terminal.sessionId);
      await invoke('close_terminal', { sessionId: terminal.sessionId });
      window.removeEventListener('resize', terminal.handleResize);
    } catch (error) {
      console.error('Error closing terminal:', error);
    }
  }

  tabs.splice(tabIndex, 1);
  // Clean up persisted custom name for this tab
  try {
    const names = loadTabNames();
    if (names[tabId]) {
      delete names[tabId];
      localStorage.setItem(TAB_NAMES_KEY, JSON.stringify(names));
    }
  } catch (_) { /* ignore */ }
  console.log('📊 Remaining tabs:', tabs.length);

  if (tabs.length === 0) {
    console.log('🆕 No tabs left, creating new tab');
    activeProjectId = 'home';
    activeProjectPath = null;
    createNewTab();
  } else if (activeTabId === tabId) {
    // Switch within the active project; if it's now empty, fall back to Home.
    const siblings = tabs.filter(t => (t.projectId || 'home') === activeProjectId);
    if (siblings.length) {
      switchTab(siblings[siblings.length - 1].id);
    } else {
      window.xnautSetActiveProject('home');
    }
  } else {
    console.log('🔄 Closed inactive tab, re-rendering tabs');
    renderTabs();
  }
}

// Find terminal by session ID
function findTerminalBySession(sessionId) {
  for (const tab of tabs) {
    const terminal = tab.terminals.find(t => t.sessionId === sessionId);
    if (terminal) return terminal;
  }
  return null;
}

function buildTerminalTheme(bgColor, textColor, cursorColor) {
  const preset = settings.activeTheme ? THEME_PRESETS[settings.activeTheme] : THEME_PRESETS['Default Dark'];
  return {
    background: bgColor,
    foreground: textColor,
    cursor: cursorColor,
    cursorAccent: cursorColor,
    selectionBackground: preset?.selection || 'rgba(255,255,255,0.2)',
    black: preset?.black || '#282c34',
    red: preset?.red || '#ff6b6b',
    green: preset?.green || '#51cf66',
    yellow: preset?.yellow || '#ffd93d',
    blue: preset?.blue || '#6bcfff',
    magenta: preset?.magenta || '#ff6ac1',
    cyan: preset?.cyan || '#4adfdf',
    white: preset?.white || '#abb2bf',
    brightBlack: preset?.brightBlack || '#5c6370',
    brightRed: preset?.brightRed || '#ff9999',
    brightGreen: preset?.brightGreen || '#85e89d',
    brightYellow: preset?.brightYellow || '#ffea7f',
    brightBlue: preset?.brightBlue || '#8cc8ff',
    brightMagenta: preset?.brightMagenta || '#ff99d6',
    brightCyan: preset?.brightCyan || '#7ce9e9',
    brightWhite: preset?.brightWhite || '#ffffff',
  };
}

// ==================== Theme Presets ====================
const DEFAULT_THEME_NAMES = ['xNAUT Instrument', 'Jellybeans', 'Default Dark', 'Dracula', 'Solarized Light', 'Monokai'];

const THEME_PRESETS = {
  'xNAUT Instrument': {
    bg: '#101214', fg: '#F2F4F5', cursor: '#F4B942', chrome: '#141719', selection: 'rgba(244,185,66,0.22)',
    accentText: '#101214',
    black: '#101214', red: '#E06C75', green: '#7FAF98', yellow: '#F4B942', blue: '#F4B942', magenta: '#B29ACB', cyan: '#8FAFB7', white: '#C4CACE',
    brightBlack: '#596168', brightRed: '#F08A91', brightGreen: '#9BC5B0', brightYellow: '#FFD36A', brightBlue: '#FFD36A', brightMagenta: '#CDB8DE', brightCyan: '#B2CED4', brightWhite: '#FFFFFF',
  },
  'Default Dark': {
    bg: '#1e1e1e', fg: '#ffffff', cursor: '#3b82f6', chrome: '#1a1a1f', selection: 'rgba(255,255,255,0.2)',
    black: '#282c34', red: '#ff6b6b', green: '#51cf66', yellow: '#ffd93d', blue: '#6bcfff', magenta: '#ff6ac1', cyan: '#4adfdf', white: '#abb2bf',
    brightBlack: '#5c6370', brightRed: '#ff9999', brightGreen: '#85e89d', brightYellow: '#ffea7f', brightBlue: '#8cc8ff', brightMagenta: '#ff99d6', brightCyan: '#7ce9e9', brightWhite: '#ffffff',
  },
  'Dracula': {
    bg: '#282a36', fg: '#f8f8f2', cursor: '#ff79c6', chrome: '#21222c', selection: 'rgba(68,71,90,0.7)',
    black: '#21222c', red: '#ff5555', green: '#50fa7b', yellow: '#f1fa8c', blue: '#bd93f9', magenta: '#ff79c6', cyan: '#8be9fd', white: '#f8f8f2',
    brightBlack: '#6272a4', brightRed: '#ff6e6e', brightGreen: '#69ff94', brightYellow: '#ffffa5', brightBlue: '#d6acff', brightMagenta: '#ff92df', brightCyan: '#a4ffff', brightWhite: '#ffffff',
  },
  'Monokai': {
    bg: '#272822', fg: '#f8f8f2', cursor: '#f92672', chrome: '#1e1f1c', selection: 'rgba(73,72,62,0.6)',
    black: '#272822', red: '#f92672', green: '#a6e22e', yellow: '#f4bf75', blue: '#66d9ef', magenta: '#ae81ff', cyan: '#a1efe4', white: '#f8f8f2',
    brightBlack: '#75715e', brightRed: '#f92672', brightGreen: '#a6e22e', brightYellow: '#f4bf75', brightBlue: '#66d9ef', brightMagenta: '#ae81ff', brightCyan: '#a1efe4', brightWhite: '#f9f8f5',
  },
  'Solarized Light': {
    bg: '#fdf6e3', fg: '#657b83', cursor: '#268bd2', chrome: '#eee8d5', selection: 'rgba(7,54,66,0.15)',
    black: '#073642', red: '#dc322f', green: '#859900', yellow: '#b58900', blue: '#268bd2', magenta: '#d33682', cyan: '#2aa198', white: '#eee8d5',
    brightBlack: '#586e75', brightRed: '#cb4b16', brightGreen: '#859900', brightYellow: '#b58900', brightBlue: '#268bd2', brightMagenta: '#6c71c4', brightCyan: '#2aa198', brightWhite: '#fdf6e3',
  },
  'Jellybeans': {
    bg: '#121212', fg: '#dedede', cursor: '#e1c0fa', chrome: '#0d0d0d', selection: 'rgba(255,255,255,0.2)',
    black: '#929292', red: '#e27373', green: '#94b979', yellow: '#ffba7b', blue: '#97bedc', magenta: '#e1c0fa', cyan: '#00988e', white: '#dedede',
    brightBlack: '#929292', brightRed: '#ffa1a1', brightGreen: '#94b979', brightYellow: '#ffdca0', brightBlue: '#97bedc', brightMagenta: '#e1c0fa', brightCyan: '#00988e', brightWhite: '#ffffff',
  },
};

// Load custom themes (imported + AI generated) from localStorage
function loadCustomThemes() {
  try {
    const custom = JSON.parse(localStorage.getItem('xnaut-custom-themes') || '{}');
    Object.assign(THEME_PRESETS, custom);
  } catch (e) {}
}
loadCustomThemes();

// Merge bundled Warp/iTerm2 themes (warp-themes.js) — customs and defaults win on name clash
const BUNDLED_THEME_NAMES = [];
if (typeof WARP_THEMES !== 'undefined') {
  for (const [name, colors] of Object.entries(WARP_THEMES)) {
    if (!THEME_PRESETS[name]) {
      THEME_PRESETS[name] = colors;
      BUNDLED_THEME_NAMES.push(name);
    }
  }
}

// Delete a custom theme (not defaults)
window.deleteCustomTheme = function(name) {
  if (DEFAULT_THEME_NAMES.includes(name)) return;
  delete THEME_PRESETS[name];
  const custom = JSON.parse(localStorage.getItem('xnaut-custom-themes') || '{}');
  delete custom[name];
  localStorage.setItem('xnaut-custom-themes', JSON.stringify(custom));
  if (settings.activeTheme === name) {
    applyThemeFromSettings('Default Dark');
  }
  loadSettingsSection('appearance');
};

// Render theme preset buttons in settings modal
function renderThemePresets() {
  const container = document.getElementById('theme-presets');
  if (!container) return;
  // Clear existing
  while (container.firstChild) container.removeChild(container.firstChild);

  for (const [name, colors] of Object.entries(THEME_PRESETS)) {
    const btn = document.createElement('div');
    btn.className = 'theme-preset';
    btn.title = name;

    const dot = document.createElement('div');
    dot.className = 'theme-preview-dot';
    dot.style.background = colors.bg;
    btn.appendChild(dot);

    const label = document.createTextNode(name);
    btn.appendChild(label);

    btn.addEventListener('click', () => {
      document.getElementById('terminal-bg-color').value = colors.bg;
      document.getElementById('terminal-text-color').value = colors.fg;
      document.getElementById('terminal-cursor-color').value = colors.cursor;
      document.getElementById('app-chrome-color').value = colors.chrome;

      settings.activeTheme = name;

      container.querySelectorAll('.theme-preset').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
    });

    container.appendChild(btn);
  }
}

// Apply app chrome color (top bar, panels)
function hexToRgb(hex) {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return [r, g, b];
}

function shiftColor(hex, amount) {
  const [r, g, b] = hexToRgb(hex);
  return '#' + [r, g, b].map(c => Math.min(255, Math.max(0, c + amount)).toString(16).padStart(2, '0')).join('');
}

/// Blend two hex colours. t=0 returns `a`, t=1 returns `b`.
function mixColor(a, b, t) {
  const [r1, g1, b1] = hexToRgb(a);
  const [r2, g2, b2] = hexToRgb(b);
  const m = (x, y) => Math.round(x + (y - x) * t).toString(16).padStart(2, '0');
  return '#' + m(r1, r2) + m(g1, g2) + m(b1, b2);
}

function applyAppChrome(chromeColor) {
  if (!chromeColor) return;
  const preset = settings.activeTheme ? THEME_PRESETS[settings.activeTheme] : null;
  const root = document.documentElement.style;

  // Background tiers
  root.setProperty('--bg-primary', preset?.bg || chromeColor);
  root.setProperty('--bg-secondary', chromeColor);
  root.setProperty('--bg-tertiary', shiftColor(chromeColor, 16));

  // Everything below is derived by blending TOWARD the opposite end rather than
  // shifting by a fixed signed amount. shiftColor(fg, -40) always darkens: right
  // on a light theme, backwards on a dark one, where it pushes secondary text
  // away from the background and makes it louder instead of quieter. Blending is
  // luminance-agnostic — the same ratios read correctly on Gruvbox Light and on
  // Synthwave 84.
  if (preset) {
    const fg = preset.fg;
    const bg = chromeColor;
    root.setProperty('--text-primary', fg);
    // Text recedes toward the background.
    root.setProperty('--text-secondary', mixColor(fg, bg, 0.30));
    root.setProperty('--text-muted', mixColor(fg, bg, 0.52));
    // Surfaces lift toward the foreground.
    root.setProperty('--border', mixColor(bg, fg, 0.18));
    root.setProperty('--border-color', mixColor(bg, fg, 0.18));
    root.setProperty('--hover-bg', mixColor(bg, fg, 0.07));
    root.setProperty('--active-bg', mixColor(bg, fg, 0.13));
    root.setProperty('--chip-bg', mixColor(bg, fg, 0.10));
    root.setProperty('--menu-bg', mixColor(bg, fg, 0.06));
    root.setProperty('--editor-surface', bg);
    // Status dots stay semantic — green means running on any theme — but the
    // off state is only meant to be a shape, so it tracks the background.
    root.setProperty('--dot-on', preset.green || '#3fb950');
    root.setProperty('--dot-off', mixColor(bg, fg, 0.32));
    root.setProperty('--accent', preset.blue || '#3b82f6');
    root.setProperty('--accent-hover', shiftColor(preset.blue || '#3b82f6', -20));
    root.setProperty('--accent-foreground', preset.accentText || '#fff');
    root.setProperty('--primary-foreground', preset.accentText || '#fff');
  }
}

// Apply appearance settings to all existing terminals
function applyAppearanceToAllTerminals() {
  const bgColor = settings.terminalBgColor || '#1e1e1e';
  const textColor = settings.terminalTextColor || '#ffffff';
  const cursorColor = settings.terminalCursorColor || '#3b82f6';
  const opacity = settings.terminalOpacity !== undefined ? settings.terminalOpacity : 100;
  const fontFamily = settings.terminalFontFamily || 'default';
  const chromeColor = settings.appChromeColor;

  const fontStack = fontFamily === 'default'
    ? '"SF Mono", Menlo, "JetBrains Mono", "DejaVu Sans Mono", "Fira Code", monospace'
    : `"${fontFamily}", "SF Mono", Menlo, monospace`;

  // Apply app chrome color
  if (chromeColor) {
    applyAppChrome(chromeColor);
  }

  for (const tab of tabs) {
    for (const terminal of tab.terminals) {
      if (terminal.term && terminal.pane) {
        terminal.term.options.theme = buildTerminalTheme(bgColor, textColor, cursorColor);

        // Update font family
        terminal.term.options.fontFamily = fontStack;

        // Make pane background transparent for see-through effect
        terminal.pane.style.background = 'transparent';

        // Find the terminal output div and apply opacity
        const terminalDiv = terminal.pane.querySelector('.terminal-output');
        if (terminalDiv) {
          terminalDiv.style.opacity = opacity / 100;
        }

        // Refresh terminal to apply changes
        terminal.term.refresh(0, terminal.term.rows - 1);
      }
    }
  }
}

// Reset appearance to defaults
function resetAppearanceToDefaults() {
  // Set default values
  document.getElementById('terminal-opacity').value = 100;
  document.getElementById('opacity-value').textContent = 100;
  document.getElementById('terminal-bg-color').value = '#1e1e1e';
  document.getElementById('terminal-text-color').value = '#ffffff';
  document.getElementById('terminal-cursor-color').value = '#3b82f6';
  document.getElementById('app-chrome-color').value = '#1a1a1f';
  document.getElementById('terminal-font-family').value = 'default';

  // Clear active preset highlight
  const presets = document.getElementById('theme-presets');
  if (presets) presets.querySelectorAll('.theme-preset').forEach(b => b.classList.remove('active'));

  // Update settings object
  settings.terminalOpacity = 100;
  settings.terminalBgColor = '#1e1e1e';
  settings.terminalTextColor = '#ffffff';
  settings.terminalCursorColor = '#3b82f6';
  settings.appChromeColor = '#1a1a1f';
  settings.terminalFontFamily = 'default';

  // Save to localStorage
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));

  // Apply to all terminals
  applyAppearanceToAllTerminals();

  updateStatus('Appearance reset to defaults');
  setTimeout(() => updateStatus('Ready'), 2000);
}

// Settings Management
async function loadSettings() {
  try {
    const saved = localStorage.getItem('xnaut-settings');
    if (saved) {
      settings = JSON.parse(saved);

      // Apply settings to UI
      if (settings.apiKeyAnthropic) {
        document.getElementById('api-key-anthropic').value = settings.apiKeyAnthropic;
      }
      if (settings.apiKeyOpenAI) {
        document.getElementById('api-key-openai').value = settings.apiKeyOpenAI;
      }
      if (settings.apiKeyOpenRouter) {
        document.getElementById('api-key-openrouter').value = settings.apiKeyOpenRouter;
      }
      if (settings.apiKeyPerplexity) {
        document.getElementById('api-key-perplexity').value = settings.apiKeyPerplexity;
      }
      if (settings.fontSize) {
        document.getElementById('font-size').value = settings.fontSize;
      }
      if (settings.theme) {
        document.getElementById('theme').value = settings.theme;
      }
      // LLM provider/model now handled by cascading dropdown (no input elements to set)
      if (settings.shellType) {
        document.getElementById('shell-type').value = settings.shellType;
        if (settings.shellType === 'custom' && settings.customShell) {
          document.getElementById('custom-shell-group').style.display = 'block';
          document.getElementById('custom-shell').value = settings.customShell;
        }
      }

      // Terminal appearance settings
      if (settings.terminalOpacity !== undefined) {
        document.getElementById('terminal-opacity').value = settings.terminalOpacity;
        document.getElementById('opacity-value').textContent = settings.terminalOpacity;
      }
      if (settings.terminalBgColor) {
        document.getElementById('terminal-bg-color').value = settings.terminalBgColor;
      }
      if (settings.terminalTextColor) {
        document.getElementById('terminal-text-color').value = settings.terminalTextColor;
      }
      if (settings.terminalCursorColor) {
        document.getElementById('terminal-cursor-color').value = settings.terminalCursorColor;
      }
      if (settings.appChromeColor) {
        document.getElementById('app-chrome-color').value = settings.appChromeColor;
        applyAppChrome(settings.appChromeColor);
      }
      if (settings.terminalFontFamily) {
        document.getElementById('terminal-font-family').value = settings.terminalFontFamily;
      }
    }

    // Provider credentials historically lived only in WebKit localStorage,
    // while chat and agents read the Rust settings store. Hydrate missing UI
    // values from the durable registry, then migrate the visible Settings-page
    // values back before any conversation surface is mounted.
    if (window.__TAURI__?.core?.invoke) {
      const durable = await invoke('settings_get').catch(() => null);
      const providers = durable?.llm_providers || [];
      const nautgate = providers.find((item) => String(item?.name || '').toLowerCase() === 'nautgate')
        || (String(durable?.llm?.provider || '').toLowerCase() === 'nautgate' ? durable.llm : null);
      if (nautgate) {
        if (!settings.nautgateUrl && nautgate.endpoint) settings.nautgateUrl = nautgate.endpoint;
        if (!settings.apiKeyNautGate && nautgate.api_key) settings.apiKeyNautGate = nautgate.api_key;
      }
      // The local providers need the same hydration. Their URL lived only in
      // localStorage and defaulted to the vendor's stock port, so an install on
      // a non-default port (LM Studio on 1238) probed a dead 1234, rendered
      // "not reachable", and left an unchangeable model dropdown, while the
      // request itself went to the configured endpoint and failed on a model id
      // nobody could see was stale. The dropdown appends /v1 itself.
      const originOf = (url) => String(url || '').replace(/\/+$/, '').replace(/\/v1$/i, '');
      const byName = (n) => providers.find((item) => String(item?.name || '').toLowerCase() === n);
      const lmstudio = byName('lmstudio');
      if (!settings.lmstudioUrl && lmstudio?.endpoint) settings.lmstudioUrl = originOf(lmstudio.endpoint);
      const ollama = byName('ollama');
      if (!settings.ollamaUrl && ollama?.endpoint) settings.ollamaUrl = originOf(ollama.endpoint);
      localStorage.setItem('xnaut-settings', JSON.stringify(settings));
      await window.xnautSyncChatSettingsFromAiSettings?.().catch(() => false);
    }

    // Render theme presets in settings modal
    renderThemePresets();
  } catch (e) {
    console.error('Failed to load settings:', e);
  }
}

function saveSettings() {
  const shellType = document.getElementById('shell-type').value;
  settings = {
    // This legacy modal owns only the fields below. Preserve credentials and
    // provider URLs owned by the AI Settings page.
    ...settings,
    apiKeyAnthropic: document.getElementById('api-key-anthropic').value,
    apiKeyOpenAI: document.getElementById('api-key-openai').value,
    apiKeyOpenRouter: document.getElementById('api-key-openrouter').value,
    apiKeyPerplexity: document.getElementById('api-key-perplexity').value,
    fontSize: parseInt(document.getElementById('font-size').value),
    theme: document.getElementById('theme').value,
    llmProvider: settings.llmProvider || 'anthropic',
    llmModel: settings.llmModel || 'claude-3-5-sonnet-20241022',
    shellType: shellType,
    customShell: shellType === 'custom' ? document.getElementById('custom-shell').value : null,
    // Terminal appearance settings
    terminalOpacity: parseInt(document.getElementById('terminal-opacity').value),
    terminalBgColor: document.getElementById('terminal-bg-color').value,
    terminalTextColor: document.getElementById('terminal-text-color').value,
    terminalCursorColor: document.getElementById('terminal-cursor-color').value,
    appChromeColor: document.getElementById('app-chrome-color').value,
    terminalFontFamily: document.getElementById('terminal-font-family').value,
    activeTheme: settings.activeTheme
  };

  localStorage.setItem('xnaut-settings', JSON.stringify(settings));
  saveKeybindings();
  closeModal('settings-modal');

  // Apply appearance to all existing terminals
  applyAppearanceToAllTerminals();

  // Show success message
  updateStatus('Settings saved!');
  setTimeout(() => updateStatus('Ready'), 2000);
}

// Command History
function loadCommandHistory() {
  try {
    const saved = localStorage.getItem('xnaut-history');
    if (saved) {
      commandHistory = JSON.parse(saved);
    }
  } catch (e) {
    console.error('Failed to load command history:', e);
  }
}

function searchCommandHistory(query) {
  if (!query) return commandHistory.slice(-50).reverse();

  const lowerQuery = query.toLowerCase();
  return commandHistory
    .filter(item => item.command.toLowerCase().includes(lowerQuery))
    .slice(-50)
    .reverse();
}

function showCommandHistory() {
  showModal('history-modal');
  renderHistoryResults('');
  document.getElementById('history-search').focus();
}

// ==================== Autocomplete Engine ====================
const autocompleteState = {
  currentInput: '',
  suggestion: null,
  visible: false,
  activeTerminal: null,
  debounceTimer: null,
};

function getAutocompleteSuggestions(input) {
  if (!input || input.length < 2) return [];
  const lower = input.toLowerCase();
  const seen = new Set();
  return commandHistory
    .filter(item => {
      const cmd = item.command || item;
      if (seen.has(cmd)) return false;
      seen.add(cmd);
      return cmd.toLowerCase().startsWith(lower) && cmd !== input;
    })
    .slice(-5)
    .reverse()
    .map(item => item.command || item);
}

function showAutocompleteSuggestion(term, sessionId, backendSessionId) {
  const suggestions = getAutocompleteSuggestions(autocompleteState.currentInput);
  const dropdown = document.getElementById('autocomplete-dropdown');
  const suggestionsDiv = document.getElementById('autocomplete-suggestions');
  if (!dropdown || !suggestionsDiv) return;

  if (suggestions.length === 0) {
    hideAutocomplete();
    return;
  }

  autocompleteState.suggestion = suggestions[0];
  autocompleteState.visible = true;
  autocompleteState.activeTerminal = { term, sessionId, backendSessionId };

  suggestionsDiv.innerHTML = suggestions.map((s, i) =>
    `<div class="autocomplete-item${i === 0 ? ' active' : ''}" data-cmd="${s.replace(/"/g, '&quot;')}">${escapeHtml(s)}</div>`
  ).join('');

  // Click to accept suggestion
  suggestionsDiv.querySelectorAll('.autocomplete-item').forEach(item => {
    item.addEventListener('click', () => {
      acceptSuggestion(item.dataset.cmd);
    });
  });

  dropdown.style.display = 'block';
}

function hideAutocomplete() {
  const dropdown = document.getElementById('autocomplete-dropdown');
  if (dropdown) dropdown.style.display = 'none';
  autocompleteState.visible = false;
  autocompleteState.suggestion = null;
}

async function acceptSuggestion(suggestion) {
  if (!suggestion || !autocompleteState.activeTerminal) return;
  const { backendSessionId } = autocompleteState.activeTerminal;
  const remaining = suggestion.slice(autocompleteState.currentInput.length);
  if (remaining) {
    try {
      // Send the remaining characters + a way to place cursor
      // Use Ctrl+U to clear line, then send full command
      await invoke('write_to_terminal', {
        sessionId: backendSessionId,
        data: '\x15' + suggestion // Ctrl+U clears line, then type full command
      });
    } catch (e) {
      console.error('Autocomplete accept error:', e);
    }
  }
  hideAutocomplete();
  autocompleteState.currentInput = '';
}

function escapeHtml(str) {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function handleAutocompleteInput(data, term, sessionId, backendSessionId) {
  // Enter key — reset input tracking, save to history, log to worklog
  if (data === '\r' || data === '\n') {
    if (autocompleteState.currentInput.trim()) {
      const cmd = autocompleteState.currentInput.trim();
      commandHistory.push({ command: cmd, timestamp: Date.now() });
      if (commandHistory.length > 1000) commandHistory = commandHistory.slice(-1000);
      localStorage.setItem('xnaut-history', JSON.stringify(commandHistory));
      // Auto-log to work session if active
      const sharedPath = document.getElementById('shared-status-path');
      worklogAutoLog(cmd, sharedPath?.textContent || '~');
    }
    autocompleteState.currentInput = '';
    hideAutocomplete();
    return;
  }

  // Tab key — accept suggestion
  if (data === '\t' && autocompleteState.visible && autocompleteState.suggestion) {
    acceptSuggestion(autocompleteState.suggestion);
    return 'consumed'; // Don't send tab to PTY
  }

  // Escape — dismiss
  if (data === '\x1b' && autocompleteState.visible) {
    hideAutocomplete();
    return;
  }

  // Backspace
  if (data === '\x7f' || data === '\b') {
    autocompleteState.currentInput = autocompleteState.currentInput.slice(0, -1);
  }
  // Ctrl+C / Ctrl+U — reset
  else if (data === '\x03' || data === '\x15') {
    autocompleteState.currentInput = '';
    hideAutocomplete();
    return;
  }
  // Printable characters
  else if (data.length === 1 && data.charCodeAt(0) >= 32) {
    autocompleteState.currentInput += data;
  }
  // Arrow keys, escape sequences — ignore for autocomplete
  else if (data.startsWith('\x1b')) {
    return;
  }

  // Debounced suggestion lookup
  clearTimeout(autocompleteState.debounceTimer);
  autocompleteState.debounceTimer = setTimeout(() => {
    showAutocompleteSuggestion(term, sessionId, backendSessionId);
  }, 150);
}

function renderHistoryResults(query) {
  const results = searchCommandHistory(query);
  const resultsContainer = document.getElementById('history-results');

  if (results.length === 0) {
    resultsContainer.innerHTML = '<div class="history-empty">No commands found</div>';
    return;
  }

  resultsContainer.innerHTML = results.map((item, i) => `
    <div class="history-item" data-history-index="${i}">
      <div class="history-timestamp">${new Date(item.timestamp).toLocaleString()}</div>
      <div class="history-command">${escapeHtml(item.command)}</div>
    </div>
  `).join('');
  // CSP forbids inline handlers in the bundled app — bind from JS.
  resultsContainer.querySelectorAll('.history-item[data-history-index]').forEach((el) => {
    el.onclick = () => executeHistoryCommand(results[Number(el.dataset.historyIndex)].command);
  });
}

async function executeHistoryCommand(command) {
  closeModal('history-modal');

  // Get active terminal
  const tab = tabs.find(t => t.id === activeTabId);
  if (tab && tab.terminals.length > 0) {
    const terminal = tab.terminals[0];
    try {
      await invoke('write_to_terminal', {
        sessionId: terminal.sessionId,
        data: command + '\n'
      });
    } catch (error) {
      console.error('Error executing command:', error);
    }
  }
}

// Chat Session Management
let chatSessions = [];
let activeChatSessionId = null;

function initChatSessions() {
  try {
    const saved = localStorage.getItem('xnaut-chat-sessions');
    if (saved) {
      chatSessions = JSON.parse(saved);
    }
  } catch (e) {
    console.error('Failed to load chat sessions:', e);
  }

  // Create default session if none exist
  if (chatSessions.length === 0) {
    createNewChatSession('New Chat');
  } else {
    activeChatSessionId = chatSessions[0].id;
  }
}

function createNewChatSession(title = 'New Chat') {
  const session = {
    id: Date.now().toString(),
    title: title,
    messages: [],
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString()
  };

  chatSessions.unshift(session);
  activeChatSessionId = session.id;
  saveChatSessions();
  renderChatSessions();
  clearChatDisplay();
  return session;
}

function saveChatSessions() {
  try {
    localStorage.setItem('xnaut-chat-sessions', JSON.stringify(chatSessions));
  } catch (e) {
    console.error('Failed to save chat sessions:', e);
  }
}

function getActiveSession() {
  return chatSessions.find(s => s.id === activeChatSessionId);
}

function switchChatSession(sessionId) {
  activeChatSessionId = sessionId;
  renderChatSessions();
  loadSessionMessages();
}

function loadSessionMessages() {
  const session = getActiveSession();
  if (!session) return;

  clearChatDisplay();
  session.messages.forEach(msg => {
    addChatMessageToDOM(msg.role, msg.content);
  });
}

function clearChatDisplay() {
  // The chat pane is built lazily, so #chat-messages is absent at startup.
  // Without this guard a fresh profile (no saved sessions -> createNewChatSession)
  // threw here and took the whole of init() down with it. XNAUT-74.
  const el = document.getElementById('chat-messages');
  if (!el) return;
  el.innerHTML = '';
}

function renderChatSessions() {
  const container = document.getElementById('chat-sessions-list');
  if (!container) return; // Handle if sidebar not visible

  container.innerHTML = chatSessions.map(session => {
    const timeAgo = getTimeAgo(new Date(session.updatedAt));
    const isActive = session.id === activeChatSessionId;

    return `
      <div class="chat-session-item ${isActive ? 'active' : ''}" data-session-id="${session.id}">
        <span class="session-icon">💬</span>
        <div class="chat-session-info">
          <div class="chat-session-title">${escapeHtml(session.title)}</div>
          <div class="chat-session-time">${timeAgo}</div>
        </div>
        <button class="session-delete-btn" data-session-id="${session.id}" title="Delete session">×</button>
      </div>
    `;
  }).join('');

  // Attach click handlers for session switching
  container.querySelectorAll('.chat-session-item').forEach(item => {
    item.addEventListener('click', (e) => {
      // Don't switch if clicking delete button
      if (e.target.classList.contains('session-delete-btn')) return;

      const sessionId = item.dataset.sessionId;
      switchChatSession(sessionId);
    });
  });

  // Attach delete button handlers
  container.querySelectorAll('.session-delete-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const sessionId = btn.dataset.sessionId;
      deleteChatSession(sessionId);
    });
  });
}

function deleteChatSession(sessionId) {
  if (chatSessions.length === 1) {
    // Don't delete the last session, just clear it
    const session = chatSessions[0];
    session.messages = [];
    session.title = 'New Chat';
    clearChatDisplay();
    saveChatSessions();
    renderChatSessions();
    return;
  }

  chatSessions = chatSessions.filter(s => s.id !== sessionId);

  // If we deleted the active session, switch to first available
  if (activeChatSessionId === sessionId) {
    activeChatSessionId = chatSessions[0].id;
    loadSessionMessages();
  }

  saveChatSessions();
  renderChatSessions();
}

function getTimeAgo(date) {
  const seconds = Math.floor((new Date() - date) / 1000);
  if (seconds < 60) return 'just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

// AI Chat with Streaming
async function sendChatMessage() {
  const input = document.getElementById('chat-input');
  const message = input.value.trim();

  if (!message) return;

  // Add user message
  addChatMessage('user', message);
  input.value = '';

  // Get AI response with streaming
  try {
    const apiKey = getAPIKey();
    const provider = settings.llmProvider || 'anthropic';
    const localProviders = ['ollama', 'lmstudio', 'nautgate'];
    if (!apiKey && !localProviders.includes(provider)) {
      addChatMessage('assistant', 'Please set your API key in Settings first.');
      return;
    }

    const context = terminalOutputBuffer.slice(-2000); // Last 2000 chars
    const model = settings.llmModel || 'claude-sonnet-4-5-20250929';

    console.log('🤖 Sending AI request:', { provider, model, promptLength: message.length, contextLength: context.length });

    let response;
    if (provider === 'ollama') {
      const url = settings.ollamaUrl || 'http://localhost:11434';
      const data = await invoke('net_fetch_json', {
        url: url.replace(/\/$/, '') + '/api/chat',
        method: 'POST',
        body: {
          model: model,
          messages: [
            ...(context ? [{ role: 'system', content: 'Terminal context:\n' + context }] : []),
            { role: 'user', content: message }
          ],
          stream: false
        }
      });
      response = data.message?.content || data.response || 'No response';
    } else if (provider === 'lmstudio') {
      const url = settings.lmstudioUrl || 'http://localhost:1234';
      const data = await invoke('net_fetch_json', {
        url: url.replace(/\/$/, '') + '/v1/chat/completions',
        method: 'POST',
        body: {
          model: model,
          messages: [
            ...(context ? [{ role: 'system', content: 'Terminal context:\n' + context }] : []),
            { role: 'user', content: message }
          ]
        }
      });
      response = data.choices?.[0]?.message?.content || 'No response';
    } else {
      response = await invoke('ask_ai', {
        prompt: message, context: context,
        provider: provider, apiKey: apiKey, model: model
      });
    }

    console.log('✅ AI response received:', response.substring(0, 100) + '...');

    // Stream the response
    await addStreamingChatMessage('assistant', response);
  } catch (error) {
    console.error('❌ AI Error:', error);
    addChatMessage('assistant', `Error: ${error}`);
  }
}

// Streaming response animation
async function addStreamingChatMessage(role, content) {
  const session = getActiveSession();
  if (!session) return;

  const messagesContainer = document.getElementById('chat-messages');
  const messageEl = document.createElement('div');
  messageEl.className = `chat-message ${role} streaming`;

  const contentEl = document.createElement('div');
  contentEl.className = 'chat-message-content';
  messageEl.appendChild(contentEl);
  messagesContainer.appendChild(messageEl);

  // Stream word by word for smooth effect
  const words = content.split(' ');
  let currentText = '';

  for (let i = 0; i < words.length; i++) {
    currentText += (i > 0 ? ' ' : '') + words[i];
    contentEl.textContent = currentText;
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
    await new Promise(r => setTimeout(r, 30)); // 30ms per word
  }

  // Remove streaming class and apply markdown rendering
  messageEl.classList.remove('streaming');

  // Now render with markdown
  if (typeof marked !== 'undefined') {
    const renderer = new marked.Renderer();

    renderer.code = function(code, language) {
      const validLanguage = language && hljs.getLanguage(language) ? language : 'plaintext';
      const highlighted = hljs.highlight(code, { language: validLanguage }).value;

      return `
        <div class="code-block-wrapper">
          <div class="code-block-header">
            <span class="code-language">${validLanguage}</span>
            <div class="code-actions">
              <button class="code-btn copy-code" data-code="${escapeHtml(code)}" title="Copy code">📋 Copy</button>
              <button class="code-btn run-code" data-code="${escapeHtml(code)}" title="Run in terminal">▶️ Run</button>
            </div>
          </div>
          <pre><code class="hljs language-${validLanguage}">${highlighted}</code></pre>
        </div>
      `;
    };

    marked.setOptions({ renderer });
    contentEl.innerHTML = marked.parse(content);

    // Apply syntax highlighting
    contentEl.querySelectorAll('pre code:not(.hljs)').forEach((block) => {
      hljs.highlightElement(block);
    });

    // Add event listeners to copy buttons
    contentEl.querySelectorAll('.copy-code').forEach(btn => {
      btn.onclick = () => {
        const code = btn.dataset.code;
        navigator.clipboard.writeText(code);
        btn.textContent = '✅ Copied';
        setTimeout(() => btn.textContent = '📋 Copy', 2000);
      };
    });

    // Add event listeners to run buttons
    contentEl.querySelectorAll('.run-code').forEach(btn => {
      btn.onclick = async () => {
        const code = btn.dataset.code;
        await executeCodeInTerminal(code, btn);
      };
    });
  }

  // Add copy button for entire message
  const copyBtn = document.createElement('button');
  copyBtn.className = 'chat-copy-btn';
  copyBtn.textContent = '📋';
  copyBtn.title = 'Copy entire message';
  copyBtn.onclick = () => {
    navigator.clipboard.writeText(content);
    copyBtn.textContent = '✅';
    copyBtn.classList.add('copied');
    setTimeout(() => {
      copyBtn.textContent = '📋';
      copyBtn.classList.remove('copied');
    }, 2000);
  };
  messageEl.appendChild(copyBtn);

  // Save to session history
  session.messages.push({ role, content, timestamp: new Date().toISOString() });
  session.updatedAt = new Date().toISOString();

  // Auto-generate title from first message
  if (session.messages.length === 2 && session.title === 'New Chat') {
    session.title = content.substring(0, 30) + (content.length > 30 ? '...' : '');
    renderChatSessions();
  }

  saveChatSessions();
  messagesContainer.scrollTop = messagesContainer.scrollHeight;
}

// Execute code in terminal with visual feedback
async function executeCodeInTerminal(code, buttonEl) {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || tab.terminals.length === 0) {
    alert('No active terminal to run command');
    return;
  }

  const terminal = tab.terminals[0];

  try {
    buttonEl.textContent = '⏳ Running...';
    buttonEl.disabled = true;

    await invoke('write_to_terminal', {
      sessionId: terminal.sessionId,
      data: code + '\n'
    });

    buttonEl.textContent = '✅ Sent';
    setTimeout(() => {
      buttonEl.textContent = '▶️ Run';
      buttonEl.disabled = false;
    }, 2000);
  } catch (error) {
    console.error('Failed to run code:', error);
    buttonEl.textContent = '❌ Failed';
    setTimeout(() => {
      buttonEl.textContent = '▶️ Run';
      buttonEl.disabled = false;
    }, 2000);
  }
}

function addChatMessageToDOM(role, content) {
  const messagesContainer = document.getElementById('chat-messages');

  const messageEl = document.createElement('div');
  messageEl.className = `chat-message ${role}`;

  const contentEl = document.createElement('div');
  contentEl.className = 'chat-message-content';

  // Render markdown for assistant messages
  if (role === 'assistant' && typeof marked !== 'undefined') {
    // Configure marked for code block rendering
    const renderer = new marked.Renderer();
    const originalCode = renderer.code.bind(renderer);

    renderer.code = function(code, language) {
      const validLanguage = language && hljs.getLanguage(language) ? language : 'plaintext';
      const highlighted = hljs.highlight(code, { language: validLanguage }).value;

      return `
        <div class="code-block-wrapper">
          <div class="code-block-header">
            <span class="code-language">${validLanguage}</span>
            <div class="code-actions">
              <button class="code-btn copy-code" data-code="${escapeHtml(code)}" title="Copy code">📋 Copy</button>
              <button class="code-btn run-code" data-code="${escapeHtml(code)}" title="Run in terminal">▶️ Run</button>
            </div>
          </div>
          <pre><code class="hljs language-${validLanguage}">${highlighted}</code></pre>
        </div>
      `;
    };

    marked.setOptions({ renderer });
    contentEl.innerHTML = marked.parse(content);

    // Apply syntax highlighting to inline code
    contentEl.querySelectorAll('pre code:not(.hljs)').forEach((block) => {
      hljs.highlightElement(block);
    });

    // Add event listeners to copy buttons
    contentEl.querySelectorAll('.copy-code').forEach(btn => {
      btn.onclick = () => {
        const code = btn.dataset.code;
        navigator.clipboard.writeText(code);
        btn.textContent = '✅ Copied';
        setTimeout(() => {
          btn.textContent = '📋 Copy';
        }, 2000);
      };
    });

    // Add event listeners to run buttons
    contentEl.querySelectorAll('.run-code').forEach(btn => {
      btn.onclick = async () => {
        const code = btn.dataset.code;
        const tab = tabs.find(t => t.id === activeTabId);
        if (tab && tab.terminals.length > 0) {
          const terminal = tab.terminals[0];
          try {
            await invoke('write_to_terminal', {
              sessionId: terminal.sessionId,
              data: code + '\n'
            });
            btn.textContent = '✅ Sent';
            setTimeout(() => {
              btn.textContent = '▶️ Run';
            }, 2000);
          } catch (error) {
            console.error('Failed to run code:', error);
            btn.textContent = '❌ Failed';
            setTimeout(() => {
              btn.textContent = '▶️ Run';
            }, 2000);
          }
        } else {
          alert('No active terminal to run command');
        }
      };
    });
  } else {
    // Plain text for user messages
    contentEl.textContent = content;
  }

  // Message-level copy button (copies entire message)
  const copyBtn = document.createElement('button');
  copyBtn.className = 'chat-copy-btn';
  copyBtn.textContent = '📋';
  copyBtn.title = 'Copy entire message';
  copyBtn.onclick = () => {
    navigator.clipboard.writeText(content);
    copyBtn.textContent = '✅';
    copyBtn.classList.add('copied');
    setTimeout(() => {
      copyBtn.textContent = '📋';
      copyBtn.classList.remove('copied');
    }, 2000);
  };

  messageEl.appendChild(contentEl);
  messageEl.appendChild(copyBtn);
  messagesContainer.appendChild(messageEl);

  // Scroll to bottom
  messagesContainer.scrollTop = messagesContainer.scrollHeight;
}

// Wrapper that saves to session history
function addChatMessage(role, content) {
  const session = getActiveSession();
  if (session) {
    session.messages.push({ role, content, timestamp: new Date().toISOString() });
    session.updatedAt = new Date().toISOString();
    saveChatSessions();
    renderChatSessions();
  }

  addChatMessageToDOM(role, content);

  // Also save to old chat history for backwards compatibility
  chatHistory.push({ role, content, timestamp: new Date().toISOString() });
}

async function analyzeTerminalOutput() {
  const context = terminalOutputBuffer.slice(-2000);

  if (!context.trim()) {
    addChatMessage('assistant', 'No terminal output to analyze yet. Run some commands first!');
    return;
  }

  try {
    // analyze_output is the registered name and takes the text alone.
    const response = await invoke('analyze_output', { output: context });

    addChatMessage('assistant', response);
  } catch (error) {
    console.error('AI Error:', error);
    addChatMessage('assistant', `Error: ${error.message || 'Failed to analyze output'}`);
  }
}

function clearChat() {
  document.getElementById('chat-messages').innerHTML = '';
  chatHistory = [];
}

function getAPIKey() {
  const provider = settings.llmProvider || 'anthropic';

  switch (provider) {
    case 'anthropic':
      return settings.apiKeyAnthropic;
    case 'openai':
      return settings.apiKeyOpenAI;
    case 'openrouter':
      return settings.apiKeyOpenRouter;
    case 'perplexity':
      return settings.apiKeyPerplexity;
    default:
      return null;
  }
}

// SSH Management
function loadSSHProfiles() {
  console.log('🔐 Loading SSH profiles from localStorage...');
  try {
    const saved = localStorage.getItem('xnaut-ssh-profiles');
    if (saved) {
      sshProfiles = JSON.parse(saved);
      console.log('✅ Loaded', sshProfiles.length, 'SSH profiles');
    } else {
      console.log('ℹ️ No saved SSH profiles found');
    }
  } catch (e) {
    console.error('❌ Failed to load SSH profiles:', e);
  }
}

function saveSSHProfiles() {
  console.log('💾 Saving', sshProfiles.length, 'SSH profiles');
  localStorage.setItem('xnaut-ssh-profiles', JSON.stringify(sshProfiles));
}

function showSSHModal() {
  console.log('🔐 Opening SSH modal, current profiles:', sshProfiles.length);
  renderSSHProfiles();
  showModal('ssh-modal');
  console.log('✅ SSH modal should be visible now');
}

function renderSSHProfiles(searchQuery = '') {
  console.log('🎨 Rendering SSH profiles, total:', sshProfiles.length);
  const container = document.getElementById('ssh-profiles-list');

  if (!container) {
    console.error('❌ SSH profiles container not found!');
    return;
  }

  let filtered = sshProfiles;
  if (searchQuery) {
    const lower = searchQuery.toLowerCase();
    filtered = sshProfiles.filter(p =>
      p.name.toLowerCase().includes(lower) ||
      p.host.toLowerCase().includes(lower)
    );
  }

  console.log('📋 Filtered profiles:', filtered.length);

  if (filtered.length === 0) {
    container.innerHTML = '<div class="ssh-empty">No SSH profiles yet. Create one to get started!</div>';
    return;
  }

  container.innerHTML = filtered.map(profile => {
    const isConnected = activeSSHConnections.has(profile.id);
    return `
      <div class="ssh-profile-item" data-profile-id="${profile.id}">
        <div class="ssh-profile-header">
          <div class="ssh-profile-name">
            <span class="ssh-profile-status ${isConnected ? 'connected' : ''}"></span>
            ${escapeHtml(profile.name)}
          </div>
          <div class="ssh-profile-actions">
            ${isConnected ?
              `<button class="ssh-action-btn danger" data-action="disconnect" data-profile-id="${profile.id}">Disconnect</button>` :
              `<button class="ssh-action-btn success" data-action="connect" data-profile-id="${profile.id}">Connect</button>`
            }
            <button class="ssh-action-btn" data-action="edit" data-profile-id="${profile.id}">Edit</button>
            <button class="ssh-action-btn danger" data-action="delete" data-profile-id="${profile.id}">Delete</button>
          </div>
        </div>
        <div class="ssh-profile-details">${escapeHtml(profile.username)}@${escapeHtml(profile.host)}:${profile.port}</div>
      </div>
    `;
  }).join('');

  console.log('✅ SSH profiles rendered');
}

function showNewSSHProfile() {
  editingSSHProfileId = null;
  document.getElementById('ssh-profile-title').textContent = 'New SSH Profile';
  document.getElementById('ssh-profile-name').value = '';
  document.getElementById('ssh-host').value = '';
  document.getElementById('ssh-port').value = '22';
  document.getElementById('ssh-username').value = '';
  document.getElementById('ssh-password').value = '';
  document.getElementById('ssh-key-path').value = '';
  document.getElementById('ssh-auth-method').value = 'password';
  toggleSSHAuthMethod();
  loadSshConfigHosts();
  showModal('ssh-profile-modal');
}

// Load SSH config hosts when modal opens
async function loadSshConfigHosts() {
  try {
    console.log('🔄 Loading SSH config hosts...');
    const hosts = await invoke('get_ssh_config_hosts');
    console.log('✅ Loaded SSH config hosts:', hosts);
    const select = document.getElementById('ssh-config-select');

    if (!select) {
      console.error('❌ SSH config select element not found!');
      return;
    }

    // Clear existing options except first
    select.innerHTML = '<option value="">-- Select a saved SSH host --</option>';

    // Add hosts from config
    if (hosts && hosts.length > 0) {
      console.log(`📋 Adding ${hosts.length} hosts to dropdown`);
      hosts.forEach(host => {
        const option = document.createElement('option');
        option.value = JSON.stringify(host);
        const displayName = host.name + (host.hostname ? ` (${host.hostname})` : '');
        option.textContent = displayName;
        select.appendChild(option);
        console.log(`  ✓ Added: ${displayName}`);
      });
    } else {
      console.log('ℹ️ No SSH hosts found in config');
    }

    // Add change listener (remove any existing ones first)
    const newSelect = select.cloneNode(true);
    select.parentNode.replaceChild(newSelect, select);

    newSelect.addEventListener('change', (e) => {
      if (e.target.value) {
        const config = JSON.parse(e.target.value);

        // Auto-fill form fields
        document.getElementById('ssh-profile-name').value = config.name;
        document.getElementById('ssh-host').value = config.hostname || config.name;
        document.getElementById('ssh-port').value = config.port || 22;
        if (config.user) {
          document.getElementById('ssh-username').value = config.user;
        }

        // Set auth method based on identity file
        if (config.identity_file) {
          document.getElementById('ssh-auth-method').value = 'privateKey';
          const keyPathElement = document.getElementById('ssh-key-path');
          if (keyPathElement) {
            keyPathElement.value = config.identity_file;
          }
          // Trigger auth method change
          toggleSSHAuthMethod();
        } else {
          document.getElementById('ssh-auth-method').value = 'password';
          toggleSSHAuthMethod();
        }
      }
    });
  } catch (error) {
    console.error('Failed to load SSH config hosts:', error);
    // Silently fail - not critical if config file doesn't exist
  }
}


function editSSHProfile(profileId) {
  const profile = sshProfiles.find(p => p.id === profileId);
  if (!profile) return;

  editingSSHProfileId = profileId;
  document.getElementById('ssh-profile-title').textContent = 'Edit SSH Profile';
  document.getElementById('ssh-profile-name').value = profile.name;
  document.getElementById('ssh-host').value = profile.host;
  document.getElementById('ssh-port').value = profile.port;
  document.getElementById('ssh-username').value = profile.username;
  document.getElementById('ssh-password').value = profile.password || '';
  document.getElementById('ssh-key-path').value = profile.keyPath || '';
  document.getElementById('ssh-auth-method').value = profile.authMethod || 'password';
  toggleSSHAuthMethod();
  showModal('ssh-profile-modal');
}

function saveSSHProfile() {
  const name = document.getElementById('ssh-profile-name').value.trim();
  const host = document.getElementById('ssh-host').value.trim();
  const port = parseInt(document.getElementById('ssh-port').value);
  const username = document.getElementById('ssh-username').value.trim();
  const authMethod = document.getElementById('ssh-auth-method').value;
  const password = document.getElementById('ssh-password').value;
  // The key field is a PATH, and it is #ssh-key-path: the old code read a
  // textarea that nothing ever showed, so every key profile saved empty.
  const keyPath = document.getElementById('ssh-key-path').value.trim();

  if (!name || !host || !username) {
    alert('Please fill in all required fields');
    return;
  }

  const profile = {
    id: editingSSHProfileId || `ssh-${Date.now()}`,
    name,
    host,
    port,
    username,
    authMethod,
    password: authMethod === 'password' ? password : undefined,
    keyPath: authMethod === 'privateKey' ? keyPath : undefined
  };

  if (editingSSHProfileId) {
    const index = sshProfiles.findIndex(p => p.id === editingSSHProfileId);
    sshProfiles[index] = profile;
  } else {
    sshProfiles.push(profile);
  }

  saveSSHProfiles();
  closeModal('ssh-profile-modal');
  renderSSHProfiles();
}

async function deleteSSHProfile(profileId) {
  if (!await window.xnautConfirmDialog('Delete this SSH profile?', 'Delete',
    'The stored host, user and key path go. Nothing on the remote machine changes.')) return;

  sshProfiles = sshProfiles.filter(p => p.id !== profileId);
  saveSSHProfiles();
  renderSSHProfiles();
}

async function connectSSH(profileId) {
  console.log('🔄 Connecting to SSH profile:', profileId);
  const profile = sshProfiles.find(p => p.id === profileId);

  if (!profile) {
    console.error('❌ SSH profile not found:', profileId);
    alert('SSH profile not found');
    return;
  }

  console.log('✅ Found profile:', profile);

  try {
    const config = {
      host: profile.host,
      port: profile.port,
      username: profile.username,
      password: profile.authMethod === 'password' ? profile.password : undefined,
      keyPath: profile.authMethod === 'privateKey' ? profile.keyPath : undefined
    };

    console.log('📡 Invoking create_ssh_session with config:', { ...config, password: config.password ? '***' : undefined });
    const result = await invoke('create_ssh_session', { config });
    console.log('✅ SSH session created:', result);
    const sessionId = result.session_id;

    activeSSHConnections.set(profile.id, sessionId);

    // Create new tab for SSH session
    const tabId = `tab-${Date.now()}`;
    const tab = {
      id: tabId,
      name: `SSH: ${profile.name}`,
      terminals: [],
      isSSH: true,
      sshSessionId: sessionId
    };

    tab.projectId = tab.projectId || activeProjectId;
    tabs.push(tab);
    renderTabs();
    switchTab(tabId);

    // Output is wired up by createSSHTerminal, which owns the xterm instance.
    // A second listener here wrote every chunk twice.
    await listen(`ssh-closed-${sessionId}`, () => {
      activeSSHConnections.delete(profile.id);
      renderSSHProfiles();
      updateStatus(`SSH disconnected: ${profile.name}`);
    });

    closeModal('ssh-modal');
    updateStatus(`Connected to ${profile.name}`);
    console.log('✅ SSH connection complete');
  } catch (error) {
    console.error('❌ SSH connection error:', error);
    alert(`Failed to connect: ${error}`);
  }
}

async function disconnectSSH(profileId) {
  const sessionId = activeSSHConnections.get(profileId);
  if (!sessionId) return;

  try {
    await invoke('close_ssh_session', { sessionId });
    activeSSHConnections.delete(profileId);
    renderSSHProfiles();
    updateStatus('SSH disconnected');
  } catch (error) {
    console.error('SSH disconnect error:', error);
  }
}

async function testSSHConnection() {
  console.log('🧪 Testing SSH connection');

  // Get values from form
  const host = document.getElementById('ssh-host').value;
  const port = document.getElementById('ssh-port').value;
  const username = document.getElementById('ssh-username').value;
  const authMethod = document.getElementById('ssh-auth-method').value;
  const password = authMethod === 'password' ? document.getElementById('ssh-password').value : undefined;
  const keyPath = authMethod === 'privateKey' ? document.getElementById('ssh-key-path').value.trim() : undefined;

  if (!host || !username) {
    alert('Please enter host and username');
    return;
  }

  if (authMethod === 'password' && !password) {
    alert('Please enter password');
    return;
  }

  if (authMethod === 'privateKey' && !keyPath) {
    alert('Please enter the path to your private key');
    return;
  }

  const btn = document.getElementById('btn-test-ssh');
  const originalText = btn.textContent;
  btn.textContent = 'Testing...';
  btn.disabled = true;

  try {
    const config = {
      host,
      port: parseInt(port),
      username,
      password,
      keyPath
    };

    console.log('📡 Testing SSH connection with:', { host, port, username });
    // A test opens a real shell, so it has to close one too, or every press of
    // this button leaves a live connection behind.
    const test = await invoke('create_ssh_session', { config });
    await invoke('close_ssh_session', { sessionId: test.session_id }).catch(() => {});

    alert('✅ Connection successful!');
    btn.textContent = '✅ Success';
    setTimeout(() => {
      btn.textContent = originalText;
    }, 2000);
  } catch (error) {
    console.error('❌ SSH test failed:', error);
    alert(`❌ Connection failed:\n\n${error}`);
    btn.textContent = '❌ Failed';
    setTimeout(() => {
      btn.textContent = originalText;
    }, 2000);
  } finally {
    btn.disabled = false;
  }
}

function toggleSSHAuthMethod() {
  const method = document.getElementById('ssh-auth-method').value;
  document.getElementById('ssh-password-group').style.display =
    method === 'password' ? 'block' : 'none';
  document.getElementById('ssh-key-group').style.display =
    method === 'privateKey' ? 'block' : 'none';
}

// Workflows
function loadWorkflows() {
  try {
    const saved = localStorage.getItem('xnaut-workflows');
    if (saved) {
      workflows = JSON.parse(saved);
    }
  } catch (e) {
    console.error('Failed to load workflows:', e);
  }
}

function saveWorkflows() {
  localStorage.setItem('xnaut-workflows', JSON.stringify(workflows));
}

function showWorkflowsModal() {
  renderWorkflows();
  showModal('workflows-modal');
}

function renderWorkflows(searchQuery = '') {
  const container = document.getElementById('workflows-list');

  let filtered = workflows;
  if (searchQuery) {
    const lower = searchQuery.toLowerCase();
    filtered = workflows.filter(w =>
      w.name.toLowerCase().includes(lower) ||
      w.description.toLowerCase().includes(lower)
    );
  }

  if (filtered.length === 0) {
    container.innerHTML = '<div class="workflow-empty">No workflows yet. Create one to get started!</div>';
    return;
  }

  container.innerHTML = filtered.map(workflow => `
    <div class="workflow-item" data-workflow-id="${workflow.id}">
      <div class="workflow-item-header">
        <div class="workflow-item-title">${escapeHtml(workflow.name)}</div>
        <div class="workflow-item-actions">
          <button class="workflow-action-btn workflow-run" data-workflow-id="${workflow.id}">▶ Run</button>
          <button class="workflow-action-btn workflow-edit" data-workflow-id="${workflow.id}">Edit</button>
          <button class="workflow-action-btn danger workflow-delete" data-workflow-id="${workflow.id}">Delete</button>
        </div>
      </div>
      <div class="workflow-item-desc">${escapeHtml(workflow.description || 'No description')}</div>
      <div class="workflow-item-meta">
        <div class="workflow-item-commands">📝 ${workflow.commands.length} commands</div>
      </div>
    </div>
  `).join('');

  // Add event listeners using event delegation
  container.querySelectorAll('.workflow-run').forEach(btn => {
    btn.addEventListener('click', () => {
      const workflowId = btn.dataset.workflowId;
      console.log('▶️ Run workflow button clicked for:', workflowId);
      executeWorkflow(workflowId);
    });
  });

  container.querySelectorAll('.workflow-edit').forEach(btn => {
    btn.addEventListener('click', () => {
      const workflowId = btn.dataset.workflowId;
      console.log('✏️ Edit workflow button clicked for:', workflowId);
      editWorkflow(workflowId);
    });
  });

  container.querySelectorAll('.workflow-delete').forEach(btn => {
    btn.addEventListener('click', () => {
      const workflowId = btn.dataset.workflowId;
      console.log('🗑️ Delete workflow button clicked for:', workflowId);
      deleteWorkflow(workflowId);
    });
  });
}

function showNewWorkflow() {
  editingWorkflowId = null;
  document.getElementById('workflow-edit-title').textContent = 'Create Workflow';
  document.getElementById('workflow-name').value = '';
  document.getElementById('workflow-desc').value = '';
  document.getElementById('workflow-commands').value = '';
  showModal('workflow-edit-modal');
}

function editWorkflow(workflowId) {
  const workflow = workflows.find(w => w.id === workflowId);
  if (!workflow) return;

  editingWorkflowId = workflowId;
  document.getElementById('workflow-edit-title').textContent = 'Edit Workflow';
  document.getElementById('workflow-name').value = workflow.name;
  document.getElementById('workflow-desc').value = workflow.description || '';
  document.getElementById('workflow-commands').value = workflow.commands.join('\n');
  showModal('workflow-edit-modal');
}

function saveWorkflow() {
  const name = document.getElementById('workflow-name').value.trim();
  const description = document.getElementById('workflow-desc').value.trim();
  const commandsText = document.getElementById('workflow-commands').value.trim();

  if (!name || !commandsText) {
    alert('Please provide a name and at least one command');
    return;
  }

  const commands = commandsText.split('\n').filter(c => c.trim());

  const workflow = {
    id: editingWorkflowId || `workflow-${Date.now()}`,
    name,
    description,
    commands
  };

  if (editingWorkflowId) {
    const index = workflows.findIndex(w => w.id === editingWorkflowId);
    workflows[index] = workflow;
  } else {
    workflows.push(workflow);
  }

  saveWorkflows();
  closeModal('workflow-edit-modal');
  renderWorkflows();
}

async function deleteWorkflow(workflowId) {
  if (!await window.xnautConfirmDialog('Delete this workflow?', 'Delete')) return;

  workflows = workflows.filter(w => w.id !== workflowId);
  saveWorkflows();
  renderWorkflows();
}

async function executeWorkflow(workflowId) {
  const workflow = workflows.find(w => w.id === workflowId);
  if (!workflow) return;

  closeModal('workflows-modal');

  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || tab.terminals.length === 0) return;

  const terminal = tab.terminals[0];
  const sessionId = terminal.sessionId;

  console.log(`🎬 Starting workflow: ${workflow.name} with ${workflow.commands.length} commands`);
  updateStatus(`Running workflow: ${workflow.name}...`);

  for (let i = 0; i < workflow.commands.length; i++) {
    const command = workflow.commands[i];
    console.log(`  ⏳ Step ${i + 1}/${workflow.commands.length}: ${command}`);
    updateStatus(`Workflow [${i + 1}/${workflow.commands.length}]: ${command.substring(0, 30)}...`);

    try {
      // Wait for command to complete
      await executeCommandAndWait(sessionId, command);
      console.log(`  ✅ Step ${i + 1} completed`);

      // Brief pause between commands for readability
      await new Promise(resolve => setTimeout(resolve, 300));
    } catch (error) {
      console.error(`❌ Workflow failed at step ${i + 1}:`, error);
      updateStatus(`Workflow failed at step ${i + 1}`);
      return;
    }
  }

  console.log('✅ Workflow completed successfully');
  updateStatus('Workflow completed!');
  setTimeout(() => updateStatus('Ready'), 2000);
}

// Execute a command and wait for it to complete using marker-based detection
async function executeCommandAndWait(sessionId, command) {
  return new Promise(async (resolve, reject) => {
    const markerId = `XNAUT_CMD_DONE_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    let unlisten = null;
    let maxWaitTimer = null;
    let outputAccumulator = ''; // Accumulate output to catch marker across chunks

    console.log(`🔧 Executing command with marker: ${markerId}`);
    console.log(`   Command: ${command}`);

    // Cleanup function
    const cleanup = () => {
      console.log('🧹 Cleaning up command listener');
      if (maxWaitTimer) clearTimeout(maxWaitTimer);
      if (unlisten) unlisten();
    };

    try {
      // Set up listener FIRST to catch the marker
      unlisten = await listen(`terminal-output-${sessionId}`, (event) => {
        try {
          const base64Data = event.payload;
          const binaryStr = atob(base64Data);
          const decodedBytes = Uint8Array.from(binaryStr, c => c.charCodeAt(0));
          const decodedData = new TextDecoder('utf-8').decode(decodedBytes);

          // Accumulate output (marker might be split across chunks)
          outputAccumulator += decodedData;

          // Keep only last 500 chars to avoid memory bloat
          if (outputAccumulator.length > 500) {
            outputAccumulator = outputAccumulator.slice(-500);
          }

          // Check if our completion marker appears
          if (outputAccumulator.includes(markerId)) {
            console.log('✅ Completion marker detected, command finished!');
            // Small delay to ensure marker is fully processed
            setTimeout(() => {
              cleanup();
              resolve();
            }, 100);
          }
        } catch (error) {
          console.error('❌ Error processing output:', error);
        }
      });

      console.log('✅ Listener set up');

      // Send command followed by marker echo
      // The marker will only execute AFTER the command completes
      const commandWithMarker = `${command} ; echo "${markerId}"`;

      await invoke('write_to_terminal', {
        sessionId: sessionId,
        data: commandWithMarker + '\n'
      });

      console.log('📤 Command + marker sent, waiting...');

      // Maximum wait time (10 minutes) for very long-running commands
      maxWaitTimer = setTimeout(() => {
        console.log('⏱️ Maximum wait time (10min) reached, proceeding');
        cleanup();
        resolve();
      }, 600000);

    } catch (error) {
      console.error('❌ Command execution error:', error);
      cleanup();
      reject(error);
    }
  });
}

function toggleWorkflowRecording() {
  if (isRecordingWorkflow) {
    // Stop recording
    isRecordingWorkflow = false;
    recordedCommands = [];
    updateStatus('Recording stopped');
  } else {
    // Start recording
    isRecordingWorkflow = true;
    recordedCommands = [];
    updateStatus('Recording commands...');
  }
}

// Triggers
function loadTriggers() {
  try {
    const saved = localStorage.getItem('xnaut-triggers');
    if (saved) {
      triggers = JSON.parse(saved);
    }
  } catch (e) {
    console.error('Failed to load triggers:', e);
  }
}

function saveTriggers() {
  localStorage.setItem('xnaut-triggers', JSON.stringify(triggers));
}

function showTriggersModal() {
  renderTriggers();
  showModal('triggers-modal');
}

function renderTriggers(searchQuery = '') {
  const container = document.getElementById('triggers-list');

  let filtered = triggers;
  if (searchQuery) {
    const lower = searchQuery.toLowerCase();
    filtered = triggers.filter(t =>
      t.name.toLowerCase().includes(lower) ||
      t.pattern.toLowerCase().includes(lower)
    );
  }

  if (filtered.length === 0) {
    container.innerHTML = '<div class="workflow-empty">No triggers yet. Create one to get started!</div>';
    return;
  }

  container.innerHTML = filtered.map(trigger => `
    <div class="workflow-item" data-trigger-id="${trigger.id}">
      <div class="workflow-item-header">
        <div class="workflow-item-title">${escapeHtml(trigger.name)}</div>
        <div class="workflow-item-actions">
          <button class="workflow-action-btn trigger-toggle" data-trigger-id="${trigger.id}">${trigger.enabled ? 'Disable' : 'Enable'}</button>
          <button class="workflow-action-btn trigger-edit" data-trigger-id="${trigger.id}">Edit</button>
          <button class="workflow-action-btn danger trigger-delete" data-trigger-id="${trigger.id}">Delete</button>
        </div>
      </div>
      <div class="workflow-item-desc">
        Pattern: ${escapeHtml(trigger.pattern)} (${trigger.type})<br>
        Message: ${escapeHtml(trigger.message)}
      </div>
    </div>
  `).join('');

  // Add event listeners using event delegation
  container.querySelectorAll('.trigger-toggle').forEach(btn => {
    btn.addEventListener('click', () => {
      const triggerId = btn.dataset.triggerId;
      console.log('🔄 Toggle trigger button clicked for:', triggerId);
      toggleTrigger(triggerId);
    });
  });

  container.querySelectorAll('.trigger-edit').forEach(btn => {
    btn.addEventListener('click', () => {
      const triggerId = btn.dataset.triggerId;
      console.log('✏️ Edit trigger button clicked for:', triggerId);
      editTrigger(triggerId);
    });
  });

  container.querySelectorAll('.trigger-delete').forEach(btn => {
    btn.addEventListener('click', () => {
      const triggerId = btn.dataset.triggerId;
      console.log('🗑️ Delete trigger button clicked for:', triggerId);
      deleteTrigger(triggerId);
    });
  });
}

function showNewTrigger() {
  editingTriggerId = null;
  document.getElementById('trigger-edit-title').textContent = 'Create Trigger';
  document.getElementById('trigger-name').value = '';
  document.getElementById('trigger-type').value = 'keyword';
  document.getElementById('trigger-pattern').value = '';
  document.getElementById('trigger-message').value = '';
  document.getElementById('trigger-enabled').checked = true;
  showModal('trigger-edit-modal');
}

function editTrigger(triggerId) {
  const trigger = triggers.find(t => t.id === triggerId);
  if (!trigger) return;

  editingTriggerId = triggerId;
  document.getElementById('trigger-edit-title').textContent = 'Edit Trigger';
  document.getElementById('trigger-name').value = trigger.name;
  document.getElementById('trigger-type').value = trigger.type;
  document.getElementById('trigger-pattern').value = trigger.pattern;
  document.getElementById('trigger-message').value = trigger.message;
  document.getElementById('trigger-enabled').checked = trigger.enabled;
  showModal('trigger-edit-modal');
}

async function saveTrigger() {
  const name = document.getElementById('trigger-name').value.trim();
  const type = document.getElementById('trigger-type').value;
  const pattern = document.getElementById('trigger-pattern').value.trim();
  const message = document.getElementById('trigger-message').value.trim();
  const enabled = document.getElementById('trigger-enabled').checked;

  if (!name || !pattern || !message) {
    alert('Please fill in all fields');
    return;
  }

  const trigger = {
    id: editingTriggerId || `trigger-${Date.now()}`,
    name,
    type,
    pattern,
    message,
    enabled
  };

  if (editingTriggerId) {
    const index = triggers.findIndex(t => t.id === editingTriggerId);
    triggers[index] = trigger;
  } else {
    triggers.push(trigger);
  }

  saveTriggers();
  closeModal('trigger-edit-modal');
  renderTriggers();
}

async function deleteTrigger(triggerId) {
  if (!await window.xnautConfirmDialog('Delete this trigger?', 'Delete')) return;

  triggers = triggers.filter(t => t.id !== triggerId);
  saveTriggers();
  renderTriggers();
}

function toggleTrigger(triggerId) {
  const trigger = triggers.find(t => t.id === triggerId);
  if (trigger) {
    trigger.enabled = !trigger.enabled;
    saveTriggers();
    renderTriggers();
  }
}

// The single trigger path (XNAUT-199). The Rust half was deleted: it had no
// caller, its store was never read back, and the only action the UI has ever
// offered is a notification, which this delivers.
const triggerLastFired = new Map();
const TRIGGER_COOLDOWN_MS = 10000;

function checkTriggers(output, now = Date.now()) {
  // The PTY chunk still carries its escape sequences, and OSC 7 puts the
  // working directory in it. Matching raw text made `cd ~/error-logs` fire an
  // error trigger, and split a word a colour code ran through.
  const text = String(output)
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
    .replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, '');

  for (const trigger of triggers) {
    if (!trigger.enabled) continue;

    let matched = false;

    if (trigger.type === 'keyword') {
      const keywords = trigger.pattern.split(',').map(k => k.trim().toLowerCase()).filter(Boolean);
      matched = keywords.some(keyword => text.toLowerCase().includes(keyword));
    } else if (trigger.type === 'regex') {
      try {
        const regex = new RegExp(trigger.pattern, 'i');
        matched = regex.test(text);
      } catch (e) {
        console.error('Invalid regex pattern:', e);
      }
    }

    if (!matched) continue;

    // Output arrives one flush per 16 ms, so a streaming agent that printed
    // "error" once would notify on every chunk that still held the word.
    const last = triggerLastFired.get(trigger.id);
    if (last !== undefined && now - last < TRIGGER_COOLDOWN_MS) continue;
    triggerLastFired.set(trigger.id, now);

    showNotification(trigger.name, trigger.message);
  }
}

// Agent Space renders its own panes and never creates an xterm, so its PTY
// output reached no trigger at all until it could call this.
window.xnautCheckTriggers = checkTriggers;

// Session sharing (shareCurrentSession / copyShareCode / #share-modal) was
// removed in XNAUT-265. It had no transport: the backend "shared" a session by
// putting it in an in-process HashMap, so nobody outside this process could join.
// It also could not have displayed a code if it had one, because share_session
// returned a bare String and this read `result.share_code` off it. And no
// element with id btn-share-session was ever in index.html, so none of it ran.

// Notifications
function requestNotificationPermission() {
  if ('Notification' in window && Notification.permission === 'default') {
    Notification.requestPermission();
  }
}

function showNotification(title, body) {
  if ('Notification' in window && Notification.permission === 'granted') {
    new Notification(title, { body, icon: '/icon.png' });
  }
}

window.xnautNotify = showNotification;

// ─── Sandbox verify: visible wherever you are (XNAUT-250) ────────────────────
// The PM panel paints verify progress only while that ticket's detail view is
// open, so a verify started from chat rendered NOWHERE — observed live twice
// on 2026-08-30, including on a passing run. One fixed pill, updated in place
// per `sandbox-verify-changed` event; the OS notification carries the verdict.
function initVerifyPill() {
  let hideTimer = null;
  const pill = document.createElement('div');
  pill.id = 'verify-pill';
  // Inline styles on purpose: the pill must render identically over every
  // panel, and panel stylesheets are scoped to their panes.
  pill.style.cssText = [
    'position:fixed', 'right:14px', 'bottom:14px', 'z-index:9999',
    'display:none', 'padding:8px 14px', 'border-radius:8px',
    'font:12px/1.4 ui-monospace,monospace', 'color:#e6e6e6',
    'background:rgba(30,32,38,0.95)', 'border:1px solid #3a3d45',
    'box-shadow:0 4px 16px rgba(0,0,0,0.4)', 'pointer-events:none',
  ].join(';');
  document.body.appendChild(pill);

  window.__TAURI__.event.listen('sandbox-verify-changed', ({ payload: record }) => {
    if (!record || !record.ticket_id) return;
    clearTimeout(hideTimer);
    pill.textContent = verifyPillLine(record);
    pill.style.display = 'block';
    pill.style.borderColor =
      record.status === 'passed' ? '#3fb950'
      : record.status === 'failed' ? '#f85149'
      : '#3a3d45';
    if (record.status === 'passed' || record.status === 'failed') {
      showNotification('Sandbox verify', verifyPillLine(record));
      hideTimer = setTimeout(() => { pill.style.display = 'none'; }, 10000);
    }
  }).catch((error) => console.warn('verify pill listener failed:', error));
}

function verifyPillLine(record) {
  const done = (record.steps || []).filter(
    (step) => step.exit_code !== null && step.exit_code !== undefined
  );
  const red = done.find((step) => step.exit_code !== 0);
  if (record.status === 'passed') return `✓ ${record.ticket_id} verified (${record.sandbox_id})`;
  if (record.status === 'failed') {
    return red
      ? `✗ ${record.ticket_id} failed at ${red.name} (exit ${red.exit_code})`
      : `✗ ${record.ticket_id} verify failed`;
  }
  const next = (record.steps || [])[done.length];
  return `⎔ ${record.ticket_id} verify: ${next ? next.name : 'starting'}…`;
}
window.xnautVerifyPillLine = verifyPillLine;

function testNotification() {
  showNotification('XNAUT Test', 'Notifications are working! 🎉');
}

// LLM Models
function updateLLMModels() {
  const provider = document.getElementById('llm-provider').value;
  const modelSelect = document.getElementById('llm-model');

  const models = {
    anthropic: [
      { value: 'claude-3-5-sonnet-20241022', label: 'Claude 3.5 Sonnet (Latest)' },
      { value: 'claude-3-5-sonnet-20240620', label: 'Claude 3.5 Sonnet (June)' },
      { value: 'claude-3-opus-20240229', label: 'Claude 3 Opus' },
      { value: 'claude-3-sonnet-20240229', label: 'Claude 3 Sonnet' },
      { value: 'claude-3-haiku-20240307', label: 'Claude 3 Haiku' }
    ],
    openai: [
      { value: 'gpt-4o', label: 'GPT-4o' },
      { value: 'gpt-4o-mini', label: 'GPT-4o Mini' },
      { value: 'gpt-4-turbo', label: 'GPT-4 Turbo' },
      { value: 'gpt-4', label: 'GPT-4' },
      { value: 'gpt-3.5-turbo', label: 'GPT-3.5 Turbo' }
    ],
    openrouter: [
      { value: 'anthropic/claude-3.5-sonnet', label: 'Claude 3.5 Sonnet' },
      { value: 'anthropic/claude-3-opus', label: 'Claude 3 Opus' },
      { value: 'openai/gpt-4o', label: 'GPT-4o' },
      { value: 'openai/gpt-4-turbo', label: 'GPT-4 Turbo' },
      { value: 'meta-llama/llama-3.1-70b-instruct', label: 'Llama 3.1 70B' }
    ],
    perplexity: [
      { value: 'llama-3.1-sonar-large-128k-online', label: 'Sonar Large Online' },
      { value: 'llama-3.1-sonar-small-128k-online', label: 'Sonar Small Online' },
      { value: 'llama-3.1-sonar-large-128k-chat', label: 'Sonar Large Chat' },
      { value: 'llama-3.1-sonar-small-128k-chat', label: 'Sonar Small Chat' }
    ]
  };

  modelSelect.innerHTML = models[provider].map(m =>
    `<option value="${m.value}">${m.label}</option>`
  ).join('');
}

// Modal Management
function showModal(modalId) {
  document.getElementById(modalId).classList.add('show');
}

function closeModal(modalId) {
  document.getElementById(modalId).classList.remove('show');
}

// Command Snippets Functions
function loadSnippets() {
  try {
    const saved = localStorage.getItem('xnaut-snippets');
    if (saved) {
      commandSnippets = JSON.parse(saved);
    }

    const savedCategories = localStorage.getItem('xnaut-snippet-categories');
    if (savedCategories) {
      snippetCategories = JSON.parse(savedCategories);
    }
  } catch (e) {
    console.error('Failed to load snippets:', e);
  }
}

function saveSnippets() {
  localStorage.setItem('xnaut-snippets', JSON.stringify(commandSnippets));
}

function exportSnippets() {
  const payload = {
    version: 1,
    exportedAt: new Date().toISOString(),
    categories: snippetCategories,
    snippets: commandSnippets,
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `xnaut-snippets-${new Date().toISOString().slice(0, 10)}.json`;
  a.click();
  URL.revokeObjectURL(url);
}

function importSnippets() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.json';
  input.onchange = async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    try {
      const text = await file.text();
      const data = JSON.parse(text);
      if (!Array.isArray(data.snippets)) throw new Error('Invalid snippet file');

      const incoming = data.snippets;
      const existingIds = new Set(commandSnippets.map(s => s.id));
      let added = 0;
      for (const s of incoming) {
        if (!existingIds.has(s.id)) {
          commandSnippets.push(s);
          added++;
        }
      }
      if (Array.isArray(data.categories)) {
        for (const cat of data.categories) {
          if (!snippetCategories.includes(cat)) snippetCategories.push(cat);
        }
        saveCategories();
      }
      saveSnippets();
      renderSnippets();
      renderCategoryPills();
      populateCategoryDropdown();
      alert(`Imported ${added} new snippet${added !== 1 ? 's' : ''} (duplicates skipped).`);
    } catch (err) {
      alert('Failed to import: ' + err.message);
    }
  };
  input.click();
}

function saveCategories() {
  localStorage.setItem('xnaut-snippet-categories', JSON.stringify(snippetCategories));
}

function toggleSnippetsPanel() {
  const panel = document.getElementById('snippets-panel');
  const isHidden = panel.style.display === 'none' || !panel.style.display;

  if (isHidden) {
    panel.style.display = 'flex';
    // Open at 1/3 of screen width (min 300px, max 800px), user can then resize
    const targetWidth = Math.max(300, Math.min(800, Math.round(window.innerWidth / 3)));
    panel.style.width = targetWidth + 'px';
    // Force layout recalculation
    panel.offsetHeight;
    renderCategoryPills();
    renderSnippets();
  } else {
    panel.style.display = 'none';
  }

  requestAnimationFrame(() => {
    resizeAllTerminals();
  });
}

function renderCategoryPills() {
  // Alphabet index instead of category pills
  const container = document.getElementById('snippet-alpha-index');
  if (!container) return;

  // Get unique first letters from categories that have snippets
  const letters = new Set();
  snippetCategories.forEach(cat => {
    const count = commandSnippets.filter(s => s.category === cat).length;
    if (count > 0) {
      letters.add(cat.charAt(0).toUpperCase());
    }
  });

  const sortedLetters = Array.from(letters).sort();

  let html = '<div class="alpha-btn' + (!activeSnippetCategory ? ' active' : '') + '" data-letter="" style="padding:2px 6px; cursor:pointer; font-size:11px; border-radius:3px; color:var(--text-secondary);">All</div>';
  sortedLetters.forEach(letter => {
    const isActive = activeSnippetCategory && activeSnippetCategory.charAt(0).toUpperCase() === letter;
    html += '<div class="alpha-btn' + (isActive ? ' active' : '') + '" data-letter="' + letter + '" style="padding:2px 6px; cursor:pointer; font-size:11px; border-radius:3px; color:var(--text-secondary);">' + letter + '</div>';
  });

  container.innerHTML = html;

  container.querySelectorAll('.alpha-btn').forEach(btn => {
    btn.onmouseenter = () => { btn.style.background = 'rgba(255,255,255,0.05)'; };
    btn.onmouseleave = () => { if (!btn.classList.contains('active')) btn.style.background = 'none'; };
    btn.onclick = () => {
      const letter = btn.dataset.letter;
      if (!letter) {
        activeSnippetCategory = null;
      } else {
        // Show categories starting with this letter as sub-pills
        const matchingCats = snippetCategories.filter(c =>
          c.charAt(0).toUpperCase() === letter &&
          commandSnippets.some(s => s.category === c)
        );
        if (matchingCats.length === 1) {
          activeSnippetCategory = matchingCats[0];
        } else {
          // Show dropdown of matching categories
          activeSnippetCategory = null;
          showCategoryDropdown(btn, matchingCats);
          return;
        }
      }
      renderCategoryPills();
      renderSnippets();
    };
  });

  // Style active button
  container.querySelectorAll('.alpha-btn.active').forEach(btn => {
    btn.style.background = 'var(--accent)';
    btn.style.color = 'white';
  });

  // Search handler
  const searchInput = document.getElementById('snippet-search');
  if (searchInput && !searchInput._bound) {
    searchInput._bound = true;
    searchInput.oninput = () => {
      const query = searchInput.value.toLowerCase();
      if (!query) {
        activeSnippetCategory = null;
        renderSnippets();
        return;
      }
      // Search across name, description, content, category
      const filtered = commandSnippets.filter(s =>
        (s.name || '').toLowerCase().includes(query) ||
        (s.description || '').toLowerCase().includes(query) ||
        (s.content || '').toLowerCase().includes(query) ||
        (s.category || '').toLowerCase().includes(query)
      );
      renderSnippetsFiltered(filtered);
    };
  }
}

function showCategoryDropdown(anchor, categories) {
  const existing = document.getElementById('cat-dropdown');
  if (existing) existing.remove();

  const dd = document.createElement('div');
  dd.id = 'cat-dropdown';
  dd.style.cssText = 'position:absolute; z-index:200; background:var(--bg-secondary); border:1px solid var(--border); border-radius:6px; padding:4px 0; min-width:140px; box-shadow:0 4px 12px rgba(0,0,0,0.3);';

  const rect = anchor.getBoundingClientRect();
  dd.style.left = rect.left + 'px';
  dd.style.top = (rect.bottom + 4) + 'px';

  categories.forEach(cat => {
    const item = document.createElement('div');
    item.style.cssText = 'padding:6px 14px; cursor:pointer; font-size:12px; color:var(--text-primary);';
    item.textContent = cat + ' (' + commandSnippets.filter(s => s.category === cat).length + ')';
    item.onmouseenter = () => { item.style.background = 'rgba(255,255,255,0.05)'; };
    item.onmouseleave = () => { item.style.background = 'none'; };
    item.onclick = () => {
      activeSnippetCategory = cat;
      dd.remove();
      renderCategoryPills();
      renderSnippets();
    };
    dd.appendChild(item);
  });

  document.body.appendChild(dd);
  setTimeout(() => {
    document.addEventListener('mousedown', function close(e) {
      if (!dd.contains(e.target)) { dd.remove(); document.removeEventListener('mousedown', close); }
    });
  }, 50);
}

function renderSnippetsFiltered(filtered) {
  const origSnippets = commandSnippets;
  const origCategory = activeSnippetCategory;
  commandSnippets = filtered;
  activeSnippetCategory = null;
  renderSnippets();
  commandSnippets = origSnippets;
  activeSnippetCategory = origCategory;
}

function showManageCategories() {
  document.getElementById('category-list').value = snippetCategories.join('\n');
  showModal('category-modal');
}

function saveCategories_modal() {
  const text = document.getElementById('category-list').value;
  const newCategories = text.split('\n')
    .map(line => line.trim())
    .filter(line => line.length > 0);

  snippetCategories = newCategories;
  saveCategories();
  renderCategoryPills();
  populateCategoryDropdown();
  closeModal('category-modal');
}

function showNewSnippet() {
  editingSnippetId = null;
  document.getElementById('snippet-modal-title').textContent = '📝 New Command Snippet';
  document.getElementById('snippet-name').value = '';
  document.getElementById('snippet-description').value = '';
  document.getElementById('snippet-content').value = '';
  populateCategoryDropdown();
  document.getElementById('snippet-category').value = activeSnippetCategory || '';
  document.getElementById('btn-delete-snippet').style.display = 'none';
  showModal('snippet-modal');
}

function populateCategoryDropdown() {
  const select = document.getElementById('snippet-category');
  select.innerHTML = '<option value="">-- No Category --</option>';
  snippetCategories.forEach(cat => {
    const option = document.createElement('option');
    option.value = cat;
    option.textContent = cat;
    select.appendChild(option);
  });
}

function editSnippet(snippetId) {
  const snippet = commandSnippets.find(s => s.id === snippetId);
  if (!snippet) return;

  editingSnippetId = snippetId;
  document.getElementById('snippet-modal-title').textContent = '✏️ Edit Snippet';
  document.getElementById('snippet-name').value = snippet.name;
  document.getElementById('snippet-description').value = snippet.description || '';
  document.getElementById('snippet-content').value = snippet.content;
  populateCategoryDropdown();
  document.getElementById('snippet-category').value = snippet.category || '';
  document.getElementById('btn-delete-snippet').style.display = 'block';
  showModal('snippet-modal');
}

function saveSnippet() {
  const name = document.getElementById('snippet-name').value.trim();
  const description = document.getElementById('snippet-description').value.trim();
  const content = document.getElementById('snippet-content').value;
  const category = document.getElementById('snippet-category').value;

  if (!name || !content) {
    alert('Please enter a name and content for the snippet');
    return;
  }

  if (editingSnippetId) {
    // Update existing
    const snippet = commandSnippets.find(s => s.id === editingSnippetId);
    if (snippet) {
      snippet.name = name;
      snippet.description = description;
      snippet.content = content;
      snippet.category = category;
      snippet.updatedAt = new Date().toISOString();
    }
  } else {
    // Create new
    commandSnippets.push({
      id: Date.now().toString(),
      name,
      description,
      content,
      category,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString()
    });
  }

  saveSnippets();
  renderSnippets();
  renderCategoryPills();
  closeModal('snippet-modal');
}

async function deleteSnippet() {
  if (!editingSnippetId) return;

  if (await window.xnautConfirmDialog('Delete this snippet?', 'Delete')) {
    commandSnippets = commandSnippets.filter(s => s.id !== editingSnippetId);
    saveSnippets();
    renderSnippets();
    closeModal('snippet-modal');
  }
}

function renderSnippets() {
  const container = document.getElementById('snippets-list');
  if (!container) return;

  // Filter by active category
  let filteredSnippets = [...commandSnippets];
  if (activeSnippetCategory) {
    filteredSnippets = filteredSnippets.filter(s => s.category === activeSnippetCategory);
  }
  // Sort: favorites first, then alphabetical
  filteredSnippets.sort((a, b) => {
    if (a.favorite && !b.favorite) return -1;
    if (!a.favorite && b.favorite) return 1;
    return (a.name || '').localeCompare(b.name || '');
  });

  if (filteredSnippets.length === 0) {
    const message = activeSnippetCategory
      ? `No snippets in "${activeSnippetCategory}"`
      : 'No snippets yet';
    container.innerHTML = `
      <div class="error-empty">
        <div class="error-empty-icon">📝</div>
        <div class="error-empty-text">
          ${message}<br>
          <small>${activeSnippetCategory ? 'Click "All" to see all snippets' : 'Click ➕ New to create your first command snippet'}</small>
        </div>
      </div>
    `;
    return;
  }

  // Extract commands from markdown content
  function extractCommands(content) {
    const commands = [];
    const codeBlockRegex = /```(?:bash|sh|shell|zsh)?\n([\s\S]*?)```/g;
    let match;
    while ((match = codeBlockRegex.exec(content)) !== null) {
      match[1].trim().split('\n').forEach(line => {
        const cmd = line.trim();
        if (cmd && !cmd.startsWith('#')) commands.push(cmd);
      });
    }
    // If no code blocks, treat each non-empty line as a command
    if (commands.length === 0) {
      content.split('\n').forEach(line => {
        const cmd = line.trim();
        if (cmd && !cmd.startsWith('#') && !cmd.startsWith('//')) commands.push(cmd);
      });
    }
    return commands;
  }

  container.innerHTML = filteredSnippets.map(snippet => {
    const commands = extractCommands(snippet.content);

    const commandRows = commands.map(cmd => `
      <div class="snippet-cmd" data-cmd="${escapeHtml(cmd)}">
        <code>${escapeHtml(cmd)}</code>
        <div class="snippet-cmd-actions">
          <button class="snippet-action-btn copy-cmd" title="Copy">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
          </button>
          <button class="snippet-action-btn run-cmd" title="Run">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
          </button>
          <button class="snippet-action-btn explain-cmd" title="Explain">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
          </button>
        </div>
      </div>
    `).join('');

    const isFav = (snippet.favorite === true);
    return `
      <div class="snippet-card" data-snippet-id="${snippet.id}">
        <div class="snippet-card-header" data-toggle-commands>
          <div style="display:flex; align-items:center; gap:8px; flex:1; min-width:0;">
            <button class="snippet-action-btn fav-snippet" data-snippet-id="${snippet.id}" title="Favorite" style="color:${isFav ? '#f59e0b' : 'var(--text-secondary)'}; font-size:14px; padding:0;">
              ${isFav ? '★' : '☆'}
            </button>
            <span style="font-size:13px; font-weight:500; color:var(--text-primary); overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">${escapeHtml(snippet.name)}</span>
            ${snippet.category ? '<span style="font-size:9px; padding:1px 5px; border-radius:2px; background:rgba(255,255,255,0.08); color:var(--text-secondary);">' + escapeHtml(snippet.category) + '</span>' : ''}
          </div>
          <div class="snippet-card-actions">
            <button class="snippet-action-btn share-snippet" data-snippet-id="${snippet.id}" title="Share">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/><polyline points="16 6 12 2 8 6"/><line x1="12" y1="2" x2="12" y2="15"/></svg>
            </button>
            <button class="snippet-action-btn edit-snippet" data-snippet-id="${snippet.id}" title="Edit">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
            </button>
          </div>
        </div>
        <div class="snippet-commands" style="display:none;">${commandRows}</div>
      </div>
    `;
  }).join('');

  // Attach command action listeners
  container.querySelectorAll('.copy-cmd').forEach(btn => {
    btn.onclick = () => {
      const cmd = btn.closest('.snippet-cmd').dataset.cmd;
      navigator.clipboard.writeText(cmd);
      btn.innerHTML = '✓';
      setTimeout(() => { btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>'; }, 1000);
    };
  });
  container.querySelectorAll('.run-cmd').forEach(btn => {
    btn.onclick = async () => {
      const cmd = btn.closest('.snippet-cmd').dataset.cmd;
      const tab = tabs.find(t => t.id === activeTabId);
      if (!tab || !tab.terminals.length) return;
      const terminal = tab.terminals[tab.focusedPaneIndex || 0];
      if (terminal) {
        try {
          await invoke('write_to_terminal', { sessionId: terminal.sessionId, data: cmd + '\n' });
        } catch (e) { console.error('Run command failed:', e); }
      }
    };
  });
  // Click header to expand/collapse commands
  container.querySelectorAll('[data-toggle-commands]').forEach(header => {
    header.onclick = (e) => {
      if (e.target.closest('.snippet-action-btn')) return;
      const card = header.parentNode;
      const cmds = card.querySelector('.snippet-commands');
      const isOpen = cmds && cmds.style.display !== 'none';

      // Collapse all cards first
      container.querySelectorAll('.snippet-card').forEach(c => {
        c.querySelector('.snippet-commands').style.display = 'none';
        c.style.display = '';
      });

      if (!isOpen) {
        // Expand this one and hide all others
        cmds.style.display = 'block';
        container.querySelectorAll('.snippet-card').forEach(c => {
          if (c !== card) c.style.display = 'none';
        });
      }
    };
  });
  // Favorite toggle
  container.querySelectorAll('.fav-snippet').forEach(btn => {
    btn.onclick = (e) => {
      e.stopPropagation();
      const id = btn.dataset.snippetId;
      const snippet = commandSnippets.find(s => s.id === id);
      if (snippet) {
        snippet.favorite = !snippet.favorite;
        localStorage.setItem('xnaut-snippets', JSON.stringify(commandSnippets));
        renderSnippets();
      }
    };
  });
  container.querySelectorAll('.edit-snippet').forEach(btn => {
    btn.onclick = () => editSnippet(btn.dataset.snippetId);
  });
  container.querySelectorAll('.share-snippet').forEach(btn => {
    btn.onclick = () => {
      const id = btn.dataset.snippetId;
      const snippet = commandSnippets.find(s => s.id === id);
      if (snippet) {
        navigator.clipboard.writeText(JSON.stringify(snippet, null, 2));
        alert('Snippet copied to clipboard as JSON — share it with your team!');
      }
    };
  });
  // Explain command handler
  container.querySelectorAll('.explain-cmd').forEach(btn => {
    btn.onclick = async () => {
      const cmd = btn.closest('.snippet-cmd').dataset.cmd;
      btn.innerHTML = '...';
      try {
        await explainCommand(cmd);
      } catch (e) {
        console.error('Explain failed:', e);
      }
      btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>';
    };
  });
}

// ===================================================================
// XNAUT-47: Command Snippets quick-access dropdown
// A toolbar dropdown (leftmost icon) that lists every executable
// command pulled from the saved snippets. Each command can be Run
// (pushed straight to the active terminal) or Copied to the clipboard.
// ===================================================================

// Pull the individual shell commands out of a snippet's markdown body.
// Mirrors the extraction used by the side-panel snippet cards.
function extractSnippetCommands(content) {
  const commands = [];
  if (!content) return commands;
  const codeBlockRegex = /```(?:bash|sh|shell|zsh)?\n([\s\S]*?)```/g;
  let match;
  while ((match = codeBlockRegex.exec(content)) !== null) {
    match[1].trim().split('\n').forEach(line => {
      const cmd = line.trim();
      if (cmd && !cmd.startsWith('#')) commands.push(cmd);
    });
  }
  if (commands.length === 0) {
    content.split('\n').forEach(line => {
      const cmd = line.trim();
      if (cmd && !cmd.startsWith('#') && !cmd.startsWith('//')) commands.push(cmd);
    });
  }
  return commands;
}

// Push a command to the currently focused terminal. Returns true on success.
async function runCommandInActiveTerminal(cmd) {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || !tab.terminals || !tab.terminals.length) return false;
  const terminal = tab.terminals[tab.focusedPaneIndex || 0];
  if (!terminal) return false;
  try {
    await invoke('write_to_terminal', { sessionId: terminal.sessionId, data: cmd + '\n' });
    return true;
  } catch (e) {
    console.error('Run command failed:', e);
    return false;
  }
}

function toggleCommandsDropdown(forceOpen) {
  const dd = document.getElementById('commands-dropdown');
  const btn = document.getElementById('btn-commands-menu');
  if (!dd) return;
  const shouldOpen = typeof forceOpen === 'boolean' ? forceOpen : dd.hasAttribute('hidden');
  if (shouldOpen) {
    dd.removeAttribute('hidden');
    if (btn) btn.setAttribute('aria-expanded', 'true');
    renderCommandsDropdown('');
    const search = document.getElementById('commands-search');
    if (search) { search.value = ''; setTimeout(() => search.focus(), 0); }
  } else {
    dd.setAttribute('hidden', '');
    if (btn) btn.setAttribute('aria-expanded', 'false');
  }
}

function renderCommandsDropdown(filterText) {
  const container = document.getElementById('commands-list');
  if (!container) return;
  const filter = (filterText || '').trim().toLowerCase();

  // Favorites first, then alphabetical — same ordering as the panel.
  const ordered = [...commandSnippets].sort((a, b) => {
    if (a.favorite && !b.favorite) return -1;
    if (!a.favorite && b.favorite) return 1;
    return (a.name || '').localeCompare(b.name || '');
  });

  let html = '';
  let total = 0;
  ordered.forEach(snippet => {
    const cmds = extractSnippetCommands(snippet.content).filter(cmd =>
      !filter || cmd.toLowerCase().includes(filter) || (snippet.name || '').toLowerCase().includes(filter)
    );
    if (!cmds.length) return;
    total += cmds.length;
    const label = (snippet.favorite ? '★ ' : '') + escapeHtml(snippet.name || 'Untitled');
    html += `<div class="commands-group-label">${label}</div>`;
    html += cmds.map(cmd => `
      <div class="commands-row" data-cmd="${escapeHtml(cmd)}">
        <code title="${escapeHtml(cmd)}">${escapeHtml(cmd)}</code>
        <div class="commands-row-actions">
          <button class="commands-btn copy-btn" title="Copy command" aria-label="Copy command">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
          </button>
          <button class="commands-btn run-btn" title="Run in terminal" aria-label="Run command">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>
          </button>
        </div>
      </div>
    `).join('');
  });

  if (total === 0) {
    container.innerHTML = `<div class="commands-empty">${
      commandSnippets.length === 0
        ? 'No snippets yet.<br><small>Use “Manage” to add commands.</small>'
        : 'No matching commands.'
    }</div>`;
    return;
  }
  container.innerHTML = html;

  container.querySelectorAll('.copy-btn').forEach(btn => {
    btn.onclick = (e) => {
      e.stopPropagation();
      const cmd = btn.closest('.commands-row').dataset.cmd;
      navigator.clipboard.writeText(cmd);
      const orig = btn.innerHTML;
      btn.innerHTML = '✓';
      setTimeout(() => { btn.innerHTML = orig; }, 1000);
    };
  });
  container.querySelectorAll('.run-btn').forEach(btn => {
    btn.onclick = async (e) => {
      e.stopPropagation();
      const cmd = btn.closest('.commands-row').dataset.cmd;
      const ok = await runCommandInActiveTerminal(cmd);
      const orig = btn.innerHTML;
      btn.innerHTML = ok ? '✓' : '⚠';
      setTimeout(() => { btn.innerHTML = orig; }, 1000);
      if (ok) toggleCommandsDropdown(false);
    };
  });
}

async function explainCommand(cmd) {
  // Asks the configured provider and answers in the chat panel.
  //
  // This used to type `antbot agent -m '…'` into the user's terminal, which
  // meant the feature only worked if a CLI happened to be installed, and it put
  // a command in their shell history that they did not write. AntBot is gone;
  // the provider they configured is the one that answers.
  const prompt = 'Explain this command in detail: what it does, what the flags mean, '
    + 'and a short example.\n\nCommand:\n' + cmd;
  try {
    addChatMessage('user', 'Explain: ' + cmd);
    const response = await callAI(prompt);
    addChatMessage('assistant', response);
  } catch (e) {
    addChatMessage('assistant', 'Could not explain that: ' + (e.message || e));
  }
}

function attachSnippetCodeBlockListeners(container) {
  // Copy buttons
  container.querySelectorAll('.copy-code').forEach(btn => {
    btn.onclick = () => {
      const code = btn.dataset.code;
      navigator.clipboard.writeText(code);
      btn.textContent = '✅ Copied';
      setTimeout(() => btn.textContent = '📋 Copy', 2000);
    };
  });

  // Run buttons
  container.querySelectorAll('.run-code').forEach(btn => {
    btn.onclick = async () => {
      const code = btn.dataset.code;
      await executeCodeInTerminal(code, btn);
    };
  });

  // Analyze/Improve buttons
  container.querySelectorAll('.analyze-code').forEach(btn => {
    btn.onclick = async () => {
      await analyzeCommandForImprovement(btn.dataset.code, btn);
    };
  });
}

async function analyzeCommandForImprovement(command, buttonEl) {
  const apiKey = getAPIKey();
  if (!apiKey) {
    alert('Please set your API key in Settings to use AI analysis');
    return;
  }

  buttonEl.textContent = '⏳ Analyzing...';
  buttonEl.disabled = true;

  try {
    const prompt = "Analyze this shell command and provide:\n1. Brief explanation of what it does\n2. Potential issues or improvements\n3. A simplified or better version if possible\n\nCommand:\n" + command + "\n\nKeep response concise (3-4 sentences max).";

    const response = await invoke('ask_ai', {
      prompt: prompt,
      context: '',
      provider: settings.llmProvider || 'anthropic',
      apiKey: apiKey,
      model: settings.llmModel || 'claude-sonnet-4-5-20250929'
    });

    // Show analysis in chat
    if (document.getElementById('chat-panel').style.display === 'none') {
      toggleChatPanel();
    }
    addChatMessage('assistant', "**Command Analysis:**\n\nOriginal command:\n" + command + "\n\n" + response);

    buttonEl.textContent = '✅ Done';
    setTimeout(() => {
      buttonEl.textContent = '🤖 Improve';
      buttonEl.disabled = false;
    }, 2000);

  } catch (error) {
    console.error('Failed to analyze command:', error);
    buttonEl.textContent = '❌ Failed';
    setTimeout(() => {
      buttonEl.textContent = '🤖 Improve';
      buttonEl.disabled = false;
    }, 2000);
  }
}

// ==================== Keybinding System ====================
const DEFAULT_KEYBINDINGS = {
  'newTab':          { key: 't', ctrl: true, shift: false, alt: false, meta: false, label: 'New Tab' },
  'closeTab':        { key: 'w', ctrl: true, shift: false, alt: false, meta: false, label: 'Close Tab' },
  'historySearch':   { key: 'r', ctrl: true, shift: false, alt: false, meta: false, label: 'Command History' },
  'splitHorizontal': { code: 'KeyD', ctrl: false, shift: true, alt: true, meta: false, label: 'Split Horizontal (Shift+Opt+D)' },
  'splitVertical':   { code: 'KeyD', ctrl: false, shift: false, alt: true, meta: false, label: 'Split Vertical (Opt+D)' },
  'splitBrowser':    { code: 'KeyB', ctrl: false, shift: false, alt: true, meta: false, label: 'Split → Browser (Opt+B)' },
  'splitMarkdown':   { code: 'KeyM', ctrl: false, shift: false, alt: true, meta: false, label: 'Split → Markdown (Opt+M)' },
  'closePane':       { code: 'KeyW', ctrl: false, shift: false, alt: true, meta: false, label: 'Close Pane' },
  'paneLeft':        { code: 'ArrowLeft', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Left' },
  'paneRight':       { code: 'ArrowRight', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Right' },
  'paneUp':          { code: 'ArrowUp', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Up' },
  'paneDown':        { code: 'ArrowDown', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Down' },
};

let keybindings = {};

function loadKeybindings() {
  const saved = localStorage.getItem('xnaut-keybindings');
  keybindings = saved ? { ...DEFAULT_KEYBINDINGS, ...JSON.parse(saved) } : { ...DEFAULT_KEYBINDINGS };
}

function saveKeybindings() {
  localStorage.setItem('xnaut-keybindings', JSON.stringify(keybindings));
}

function formatBinding(binding) {
  const parts = [];
  if (binding.ctrl) parts.push('Ctrl');
  if (binding.alt) parts.push('Opt');
  if (binding.shift) parts.push('Shift');
  if (binding.meta) parts.push('Cmd');
  const keyName = binding.key || (binding.code || '').replace('Key', '').replace('Arrow', '');
  parts.push(keyName.toUpperCase());
  return parts.join(' + ');
}

function matchesBinding(e, binding) {
  if (!!e.ctrlKey !== !!binding.ctrl) return false;
  if (!!e.altKey !== !!binding.alt) return false;
  if (!!e.shiftKey !== !!binding.shift) return false;
  if (!!e.metaKey !== !!binding.meta) return false;
  if (binding.code) return e.code === binding.code;
  if (binding.key) return e.key === binding.key;
  return false;
}

function renderKeybindingsUI() {
  const container = document.getElementById('keybinding-list');
  if (!container) return;
  container.innerHTML = '';

  for (const [action, binding] of Object.entries(keybindings)) {
    const row = document.createElement('div');
    row.style.cssText = 'display:flex; justify-content:space-between; align-items:center; padding:8px 0; border-bottom:1px solid #2a2a2f;';
    row.dataset.action = action;

    const label = document.createElement('span');
    label.style.cssText = 'color:#dce0e5; font-size:13px;';
    label.textContent = binding.label || action;

    const btn = document.createElement('button');
    btn.style.cssText = 'background:#2a2a2f; border:1px solid #464b57; border-radius:6px; color:#a9afbc; padding:4px 12px; font-family:"JetBrains Mono",monospace; font-size:12px; cursor:pointer; min-width:120px; text-align:center;';
    btn.textContent = formatBinding(binding);
    btn.title = 'Click to rebind';

    btn.addEventListener('click', () => {
      btn.textContent = 'Press keys...';
      btn.style.borderColor = '#3b82f6';

      const handler = (e) => {
        e.preventDefault();
        e.stopPropagation();
        if (['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) return;

        const newBinding = {
          ...binding,
          ctrl: e.ctrlKey,
          alt: e.altKey,
          shift: e.shiftKey,
          meta: e.metaKey,
        };
        if (e.code.startsWith('Key') || e.code.startsWith('Arrow') || e.code.startsWith('Digit')) {
          newBinding.code = e.code;
          delete newBinding.key;
        } else {
          newBinding.key = e.key;
          delete newBinding.code;
        }

        // Check for conflicts
        for (const [otherAction, otherBinding] of Object.entries(keybindings)) {
          if (otherAction !== action && formatBinding(otherBinding) === formatBinding(newBinding)) {
            btn.textContent = `Conflicts with: ${otherBinding.label || otherAction}`;
            btn.style.borderColor = '#ff6b6b';
            setTimeout(() => {
              btn.textContent = formatBinding(keybindings[action]);
              btn.style.borderColor = '#464b57';
            }, 2000);
            document.removeEventListener('keydown', handler, true);
            return;
          }
        }

        keybindings[action] = newBinding;
        btn.textContent = formatBinding(newBinding);
        btn.style.borderColor = '#51cf66';
        setTimeout(() => { btn.style.borderColor = '#464b57'; }, 1000);
        document.removeEventListener('keydown', handler, true);
      };

      document.addEventListener('keydown', handler, true);
    });

    row.appendChild(label);
    row.appendChild(btn);
    container.appendChild(row);
  }
}

function initKeybindingSearch() {
  const search = document.getElementById('keybinding-search');
  if (!search) return;
  search.addEventListener('input', () => {
    const query = search.value.toLowerCase();
    const rows = document.querySelectorAll('#keybinding-list > div');
    rows.forEach(row => {
      const text = row.textContent.toLowerCase();
      row.style.display = text.includes(query) ? 'flex' : 'none';
    });
  });
}

loadKeybindings();

// Event Listeners
function _on(id, ev, fn) { const el = document.getElementById(id); if (el) el[ev] = fn; }

function setupEventListeners() {
  // Top bar buttons
  _on('btn-new-tab', 'onclick', (e) => showNewTabMenu(e.currentTarget));

  // Projects (tasks + plan) launcher — opens the unified Projects panel.
  _on('btn-projects', 'onclick', () => {
    if (typeof window.xnautSidebarNavigate === 'function') window.xnautSidebarNavigate('pm');
    else if (typeof window.xnautAttachPmTab === 'function') window.xnautAttachPmTab();
  });

  // 3-dot menu (uses [hidden] attribute + .menu-item class — hover styling via CSS)
  _on('btn-more-menu', 'onclick', () => {
    const dd = document.getElementById('more-menu-dropdown');
    const btn = document.getElementById('btn-more-menu');
    if (!dd) return;
    const open = dd.hasAttribute('hidden');
    if (open) {
      dd.removeAttribute('hidden');
      if (btn) btn.setAttribute('aria-expanded', 'true');
    } else {
      dd.setAttribute('hidden', '');
      if (btn) btn.setAttribute('aria-expanded', 'false');
    }
  });
  document.querySelectorAll('#more-menu-dropdown .menu-item').forEach(item => {
    item.onclick = () => {
      const dd = document.getElementById('more-menu-dropdown');
      const btn = document.getElementById('btn-more-menu');
      if (dd) dd.setAttribute('hidden', '');
      if (btn) btn.setAttribute('aria-expanded', 'false');
      const action = item.dataset.action;
      if (action === 'snippets') toggleSnippetsPanel();
      else if (action === 'ssh') { loadSSHProfiles(); showSSHModal(); }
      else if (action === 'explain') explainScreen();
      else if (action === 'worklog') toggleWorkLog();
      else if (action === 'worklog-week') worklogRangeReport(7);
      else if (action === 'graph') openGraphPane();
      else if (action === 'mesh' && window.xnautOpenMesh) window.xnautOpenMesh();
      else if (action === 'agents' && window.xnautAttachAgentsTab) window.xnautAttachAgentsTab();
      else if (action === 'loops' && window.xnautAttachLoopsTab) window.xnautAttachLoopsTab();
      else if (action === 'settings') toggleSettingsPanel();
      // Reachable when startup was clean too, which is the whole point: this is
      // where debug.log is discoverable from inside the app (XNAUT-75).
      else if (action === 'diagnostics') startupHealth().show();
    };
  });
  // Close 3-dot menu on click outside
  document.addEventListener('mousedown', (e) => {
    const dd = document.getElementById('more-menu-dropdown');
    if (dd && !dd.hasAttribute('hidden') && !e.target.closest('#btn-more-menu') && !e.target.closest('#more-menu-dropdown')) {
      dd.setAttribute('hidden', '');
      const btn = document.getElementById('btn-more-menu');
      if (btn) btn.setAttribute('aria-expanded', 'false');
    }
  });

  // XNAUT-47: Command Snippets quick-access dropdown (leftmost toolbar icon).
  _on('btn-commands-menu', 'onclick', () => toggleCommandsDropdown());
  _on('btn-commands-manage', 'onclick', () => {
    toggleCommandsDropdown(false);
    toggleSnippetsPanel();
  });
  _on('commands-search', 'oninput', (e) => renderCommandsDropdown(e.target.value));
  // Close the commands dropdown on outside click.
  document.addEventListener('mousedown', (e) => {
    const dd = document.getElementById('commands-dropdown');
    if (dd && !dd.hasAttribute('hidden') && !e.target.closest('#btn-commands-menu') && !e.target.closest('#commands-dropdown')) {
      toggleCommandsDropdown(false);
    }
  });
  // Close on Escape when the dropdown is open.
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      const dd = document.getElementById('commands-dropdown');
      if (dd && !dd.hasAttribute('hidden')) toggleCommandsDropdown(false);
    }
  });


  // Open the active project's repo in the browser (Forgejo/GitHub). No repo or
  // on Home → the backend returns the default forge home instead.
  _on('btn-open-repo', 'onclick', async () => {
    try {
      const url = await invoke('repo_web_url', { path: activeProjectPath });
      if (!url) return;
      if (window.__TAURI__?.shell?.open) window.__TAURI__.shell.open(url);
      else if (window.__TAURI__) invoke('plugin:shell|open', { path: url });
      else window.open(url, '_blank');
    } catch (e) {
      console.error('open repo failed:', e);
    }
  });

  // Use event delegation for status bar buttons (they're created dynamically per terminal)

  // Simpler approach: use mouseenter for submenus
  document.addEventListener('mouseenter', (e) => {
    // Capture phase is what makes this delegation work at all, since mouseenter
    // does not bubble. The cost is that e.target is whatever the pointer
    // entered, including the document itself and text nodes, and those have no
    // classList: reading it threw TypeError twice on every walk of the UI.
    if (!(e.target instanceof Element)) return;
    if (e.target.classList.contains('llm-provider-item')) {
      // Hide all submenus
      document.querySelectorAll('.llm-model-submenu').forEach(s => s.style.display = 'none');
      // Show this one
      const submenu = e.target.querySelector('.llm-model-submenu');
      if (submenu) submenu.style.display = 'block';
    }
  }, true);

  document.addEventListener('click', (e) => {
    const target = e.target;

    // LLM dropdown toggle
    if (target.id === 'llm-dropdown-btn' || target.closest('#llm-dropdown-btn')) {
      e.stopPropagation();
      const menu = document.getElementById('llm-dropdown-menu');
      if (menu) {
        menu.style.display = menu.style.display === 'none' ? 'block' : 'none';
      }
      return;
    }

    // LLM model selection
    if (target.classList.contains('llm-model-option')) {
      const provider = target.dataset.provider;
      const model = target.dataset.model;
      const label = target.textContent;

      // Update settings
      settings.llmProvider = provider;
      settings.llmModel = model;
      localStorage.setItem('xnaut-settings', JSON.stringify(settings));

      // Update button text
      const providerIcon = {
        anthropic: '🔮',
        openai: '🟢',
        openrouter: '🤖',
        perplexity: '🔍'
      }[provider];
      const selectionEl = document.getElementById('llm-current-selection');
      if (selectionEl) {
        selectionEl.textContent = `${providerIcon} ${label}`;
      }

      // Close menu
      const menuToClose = document.getElementById('llm-dropdown-menu');
      if (menuToClose) {
        menuToClose.style.display = 'none';
      }
      return;
    }

    // Close LLM dropdown if clicking outside
    const llmDropdown = document.getElementById('llm-dropdown-menu');
    if (llmDropdown && !target.closest('.llm-dropdown-wrapper')) {
      llmDropdown.style.display = 'none';
    }

    if (target.id === 'btn-debug') showDebugInfo();
    else if (target.id === 'btn-settings') {
      toggleSettingsPanel();
    }
    else if (target.id === 'btn-toggle-chat') toggleChatPanel();
    else if (target.closest && target.closest('#btn-worklog-clock')) toggleWorkLog();
    else if (target.id === 'btn-toggle-snippets') toggleSnippetsPanel();
    else if (target.id === 'btn-triggers') showTriggersModal();
  });

  // Settings panel
  _on('btn-close-settings-panel', 'onclick', () => toggleSettingsPanel());
  document.querySelectorAll('.settings-nav-item').forEach(item => {
    item.onclick = () => loadSettingsSection(item.dataset.section);
  });

  // Old settings modal (kept for backward compat)
  _on('btn-close-settings', 'onclick', () => closeModal('settings-modal'));
  _on('btn-save-settings', 'onclick', saveSettings);
  _on('btn-reset-appearance', 'onclick', resetAppearanceToDefaults);
  document.getElementById('btn-reset-keybindings')?.addEventListener('click', () => {
    keybindings = { ...DEFAULT_KEYBINDINGS };
    saveKeybindings();
    renderKeybindingsUI();
  });

  // Chat
  // Chat panel removed — these are no-ops if elements don't exist
  const btnSendChat = document.getElementById('btn-send-chat');
  if (btnSendChat) btnSendChat.onclick = sendChatMessage;
  const btnClearChat = document.getElementById('btn-clear-chat');
  if (btnClearChat) btnClearChat.onclick = clearChat;
  const btnAnalyze = document.getElementById('btn-analyze-output');
  if (btnAnalyze) btnAnalyze.onclick = analyzeTerminalOutput;
  const btnCollapseChat = document.getElementById('btn-collapse-chat');
  if (btnCollapseChat) btnCollapseChat.onclick = toggleChatPanel;
  const btnNewSession = document.getElementById('btn-new-chat-session');
  if (btnNewSession) btnNewSession.onclick = () => {
    const session = createNewChatSession('New Chat');
    console.log('✅ Created new chat session:', session.id);
  };
  const chatInput = document.getElementById('chat-input');
  if (chatInput) chatInput.onkeydown = (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendChatMessage();
    }
  };


  // Editor
  _on('btn-toggle-editor', 'onclick', () => {
    const panel = document.getElementById('editor-panel');
    if (panel) {
      if (panel.style.display === 'none' || !panel.style.display) {
        panel.style.display = 'flex';
        const textarea = document.getElementById('editor-textarea');
        if (textarea && !editorState.path) {
          textarea.value = '// Open a file from the File Navigator to edit it here\n// Or paste content and use Save As';
        }
      } else {
        panel.style.display = 'none';
      }
      requestAnimationFrame(() => resizeAllTerminals());
    }
  });
  _on('btn-editor-save', 'onclick', saveEditorFile);
  _on('btn-editor-close', 'onclick', closeEditor);
  _on('btn-editor-preview', 'onclick', toggleEditorPreview);
  _on('btn-test-editor', 'onclick', () => { alert('Test button clicked!'); openFileInEditor('/Users/dre/.zshrc'); });

  // Snippets
  _on('btn-new-snippet', 'onclick', showNewSnippet);
  _on('btn-export-snippets', 'onclick', exportSnippets);
  _on('btn-import-snippets', 'onclick', importSnippets);
  _on('btn-close-snippet-modal', 'onclick', () => closeModal('snippet-modal'));
  _on('btn-cancel-snippet', 'onclick', () => closeModal('snippet-modal'));
  _on('btn-save-snippet', 'onclick', saveSnippet);
  _on('btn-delete-snippet', 'onclick', deleteSnippet);
  _on('btn-collapse-snippets', 'onclick', toggleSnippetsPanel);
  _on('btn-manage-categories', 'onclick', showManageCategories);

  // Category Management
  _on('btn-close-category-modal', 'onclick', () => closeModal('category-modal'));
  _on('btn-cancel-categories', 'onclick', () => closeModal('category-modal'));
  _on('btn-save-categories', 'onclick', saveCategories_modal);

  // SSH
  _on('btn-close-ssh', 'onclick', () => closeModal('ssh-modal'));
  _on('btn-new-ssh-profile', 'onclick', showNewSSHProfile);
  _on('ssh-search', 'oninput', (e) => renderSSHProfiles(e.target.value));
  _on('btn-close-ssh-profile', 'onclick', () => closeModal('ssh-profile-modal'));
  _on('btn-save-ssh-profile', 'onclick', saveSSHProfile);
  _on('btn-test-ssh', 'onclick', testSSHConnection);
  _on('ssh-auth-method', 'onchange', toggleSSHAuthMethod);

  // SSH Profile Actions - Event Delegation
  const sshProfilesList = document.getElementById('ssh-profiles-list');
  if (sshProfilesList) {
    sshProfilesList.addEventListener('click', (e) => {
      const btn = e.target.closest('.ssh-action-btn');
      if (!btn) return;

      const action = btn.dataset.action;
      const profileId = btn.dataset.profileId;

      console.log(`🔘 SSH action clicked: ${action} for profile ${profileId}`);

      switch (action) {
        case 'connect':
          console.log('🔌 Connecting SSH...');
          connectSSH(profileId);
          break;
        case 'disconnect':
          console.log('🔌 Disconnecting SSH...');
          disconnectSSH(profileId);
          break;
        case 'edit':
          console.log('✏️ Editing SSH profile...');
          editSSHProfile(profileId);
          break;
        case 'delete':
          console.log('🗑️ Deleting SSH profile...');
          deleteSSHProfile(profileId);
          break;
      }
    });
    console.log('✅ SSH profile event delegation set up');
  }

  // Triggers
  _on('btn-close-triggers', 'onclick', () => closeModal('triggers-modal'));
  _on('btn-new-trigger', 'onclick', showNewTrigger);
  _on('btn-test-notifications', 'onclick', testNotification);
  _on('trigger-search', 'oninput', (e) => renderTriggers(e.target.value));
  _on('btn-close-trigger-edit', 'onclick', () => closeModal('trigger-edit-modal'));
  _on('btn-save-trigger', 'onclick', saveTrigger);

  // Share

  // History
  _on('btn-close-history', 'onclick', () => closeModal('history-modal'));
  _on('history-search', 'oninput', (e) => renderHistoryResults(e.target.value));

  // LLM provider/model now handled by cascading dropdown event delegation (see above)

  // Shell type change
  _on('shell-type', 'onchange', (e) => {
    const customGroup = document.getElementById('custom-shell-group');
    if (customGroup) customGroup.style.display = e.target.value === 'custom' ? 'block' : 'none';
  });

  // Terminal opacity slider
  _on('terminal-opacity', 'oninput', (e) => {
    const val = document.getElementById('opacity-value');
    if (val) val.textContent = e.target.value;
  });

  // Keyboard shortcuts (driven by keybinding registry)
  const KEYBINDING_ACTIONS = {
    newTab: () => createNewTab(),
    closeTab: () => { if (activeTabId) closeTab(activeTabId); },
    historySearch: () => showCommandHistory(),
    splitVertical: () => splitPane('vertical'),
    splitHorizontal: () => splitPane('horizontal'),
    splitBrowser: () => splitPane('vertical', 'browser'),
    splitMarkdown: () => splitPane('vertical', 'markdown'),
    closePane: () => closePane(),
    paneLeft: () => navigatePane('ArrowLeft'),
    paneRight: () => navigatePane('ArrowRight'),
    paneUp: () => navigatePane('ArrowUp'),
    paneDown: () => navigatePane('ArrowDown'),
  };

  document.addEventListener('keydown', (e) => {
    for (const [action, binding] of Object.entries(keybindings)) {
      if (matchesBinding(e, binding) && KEYBINDING_ACTIONS[action]) {
        e.preventDefault();
        KEYBINDING_ACTIONS[action]();
        return;
      }
    }
  });

  // delta: +1 bigger, -1 smaller, 0 reset to 14. Applies to every live
  // terminal across all tabs, persists, and refits so rows/cols stay correct.
  function adjustTerminalFontSize(delta) {
    const cur = settings.fontSize || 14;
    const next = delta === 0 ? 14 : Math.max(8, Math.min(32, cur + delta));
    if (next === cur && delta !== 0) return;
    settings.fontSize = next;
    localStorage.setItem('xnaut-settings', JSON.stringify(settings));
    tabs.forEach((tab) => {
      (tab.terminals || []).forEach((t) => {
        if (!t || !t.term || !t.term.options) return;
        t.term.options.fontSize = terminalFontSize();
        try {
          if (t.handleResize) t.handleResize();
          else if (t.fitAddon) t.fitAddon.fit();
        } catch (_) { /* pane not ready */ }
      });
    });
  }

  // Markdown / plan doc zoom — scales the rendered view + raw editor via CSS
  // `zoom` (px-based child styles scale proportionally). delta: +1/-1/0(reset).
  function adjustMarkdownFontSize(delta) {
    const cur = settings.mdZoom || 1;
    const next = delta === 0 ? 1 : Math.max(0.6, Math.min(2.6, +(cur + delta * 0.1).toFixed(2)));
    if (next === cur && delta !== 0) return;
    settings.mdZoom = next;
    localStorage.setItem('xnaut-settings', JSON.stringify(settings));
    let st = document.getElementById('md-zoom-style');
    if (!st) { st = document.createElement('style'); st.id = 'md-zoom-style'; document.head.appendChild(st); }
    st.textContent = `.md-view,.md-edit,.plan-doc-view,.plan-doc-edit{zoom:${next};}`;
  }
  // Apply any persisted markdown zoom on load.
  if (settings.mdZoom && settings.mdZoom !== 1) {
    const st = document.createElement('style');
    st.id = 'md-zoom-style';
    st.textContent = `.md-view,.md-edit,.plan-doc-view,.plan-doc-edit{zoom:${settings.mdZoom};}`;
    document.head.appendChild(st);
  }

  // True when the user is looking at a markdown/plan doc (so zoom targets it).
  function inMarkdownContext(e) {
    const t = (e && e.target) || document.activeElement;
    if (t && t.closest && t.closest('.md-pane, .md-view, .md-edit, .plan-doc-view, .plan-doc-edit')) return true;
    const tab = tabs.find((x) => x.id === activeTabId);
    return !!(tab && (tab.isMarkdown || tab.panelFactory === 'xnautCreatePlanPane'));
  }


  // App zoom — Cmd/Ctrl + = / - / 0 across the WHOLE interface.
  //
  // There was a handler for these keys already, but it only ever routed to the
  // markdown view or the terminal font, so anywhere else in the app the keys
  // did nothing at all. Reported by André, who needs it to read the thing.
  //
  // CSS `zoom` on the root, not a font-size scale: the interface is laid out in
  // px, so scaling the root font would move nothing.
  const ZOOM_STEPS = [0.8, 0.9, 1, 1.1, 1.25, 1.4, 1.6, 1.8, 2];
  function currentZoom() {
    const stored = Number(localStorage.getItem('xnaut-ui-zoom'));
    return Number.isFinite(stored) && stored > 0 ? stored : 1;
  }
  function applyAppZoom(value) {
    const zoom = Math.min(2, Math.max(0.8, value));
    document.documentElement.style.zoom = zoom === 1 ? '' : String(zoom);
    // Native child webviews (browser panes) are positioned from
    // getBoundingClientRect, which reports CSS px in the ZOOMED coordinate
    // space while Tauri positions in unzoomed points. Everything that places
    // one multiplies by this.
    window.xnautUiZoom = zoom;
    try { localStorage.setItem('xnaut-ui-zoom', String(zoom)); } catch (_) {}
    unzoomTerminals(zoom);
    window.dispatchEvent(new Event('resize'));
  }
  // xterm maps a mouse position to a buffer row by dividing by a cell height it
  // measured itself, and CSS zoom scales what the DOM reports without touching
  // that measurement. Under zoom the two disagree by exactly the zoom factor, so
  // a click selected a row further down the further down the screen it was: at
  // 1.25, clicking row 20 selected row 25, and dragging a block was impossible.
  // Measured in tests/terminal-selection.spec.mjs.
  //
  // So terminals cancel the interface zoom on their own subtree and scale their
  // FONT instead — a path xterm re-measures — which keeps the text the size the
  // zoom asked for while the mouse math stays honest. `.terminal-output` and not
  // the whole container on purpose: browser panes are siblings in that grid and
  // place a native webview from a zoomed rect (browser-pane.js), so they must
  // keep seeing the interface zoom.
  function unzoomTerminals(zoom) {
    let st = document.getElementById('terminal-unzoom-style');
    if (!st) {
      st = document.createElement('style');
      st.id = 'terminal-unzoom-style';
      document.head.appendChild(st);
    }
    st.textContent = zoom === 1 ? '' : `.terminal-output{zoom:${(1 / zoom).toFixed(6)};}`;
    tabs.forEach((tab) => {
      (tab.terminals || []).forEach((t) => {
        if (!t || !t.term || !t.term.options) return;
        t.term.options.fontSize = terminalFontSize();
        try {
          if (t.handleResize) t.handleResize();
          else if (t.fitAddon) t.fitAddon.fit();
        } catch (_) { /* pane not ready */ }
      });
    });
  }

  function adjustAppZoom(delta) {
    if (delta === 0) return applyAppZoom(1);
    const now = currentZoom();
    const index = ZOOM_STEPS.reduce((best, step, i) =>
      Math.abs(step - now) < Math.abs(ZOOM_STEPS[best] - now) ? i : best, 0);
    applyAppZoom(ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, index + delta))]);
  }
  window.xnautAdjustAppZoom = adjustAppZoom;
  applyAppZoom(currentZoom());

  function inTerminalContext(event) {
    const target = event.target;
    return !!(target && target.closest && target.closest('.xterm, .terminal-container, .terminal-pane'));
  }

  // Font zoom — capture phase so it fires before the focused xterm textarea
  // swallows the keydown. Cmd/Ctrl + = / - / 0. Markdown and terminals keep
  // their own scaling; everything else zooms the interface.
  document.addEventListener('keydown', (e) => {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
    let delta = null;
    if (e.key === '=' || e.key === '+') delta = 1;
    else if (e.key === '-' || e.key === '_') delta = -1;
    else if (e.key === '0') delta = 0;
    if (delta === null) return;
    e.preventDefault(); e.stopPropagation();
    if (inMarkdownContext(e)) adjustMarkdownFontSize(delta);
    else if (inTerminalContext(e)) adjustTerminalFontSize(delta);
    else adjustAppZoom(delta);
  }, true);

  // Global keyboard shortcuts
  document.addEventListener('keydown', (e) => {
    // Cmd+S — save editor file
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
      const editorPanel = document.getElementById('editor-panel');
      if (editorPanel && editorPanel.style.display !== 'none') {
        e.preventDefault();
        saveEditorFile();
      }
    }
    // Escape — close modals and panels
    if (e.key === 'Escape') {
      const modals = document.querySelectorAll('.modal.show');
      modals.forEach(modal => modal.classList.remove('show'));
      const settingsPanel = document.getElementById('settings-panel');
      if (settingsPanel && settingsPanel.style.display !== 'none') {
        settingsPanel.style.display = 'none';
      }
    }
  });

  // Add resize handles to side panels
  initializeResizablePanels();
}

// Make side panels resizable
function initializeResizablePanels() {
  const panels = document.querySelectorAll('.side-panel.right-panel');

  panels.forEach(panel => {
    // Create resize handle
    const handle = document.createElement('div');
    handle.className = 'resize-handle';
    panel.insertBefore(handle, panel.firstChild);

    let isResizing = false;
    let startX = 0;
    let startWidth = 0;

    handle.addEventListener('mousedown', (e) => {
      isResizing = true;
      startX = e.clientX;
      startWidth = panel.offsetWidth;
      document.body.style.cursor = 'ew-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });

    document.addEventListener('mousemove', (e) => {
      if (!isResizing) return;

      const deltaX = startX - e.clientX; // Reversed for right panels
      const newWidth = startWidth + deltaX;

      // Respect min and max width
      const minWidth = 200;
      const maxWidth = 800;
      const clampedWidth = Math.max(minWidth, Math.min(maxWidth, newWidth));

      panel.style.width = clampedWidth + 'px';

      // Resize terminals while dragging
      requestAnimationFrame(() => {
        resizeAllTerminals();
      });
    });

    document.addEventListener('mouseup', () => {
      if (isResizing) {
        isResizing = false;
        document.body.style.cursor = '';
        document.body.style.userSelect = '';

        // Final resize after dragging ends
        requestAnimationFrame(() => {
          resizeAllTerminals();
        });
      }
    });
  });
}

// Resize all terminals to fit their containers
function resizeAllTerminals() {
  for (const tab of tabs) {
    for (const terminal of tab.terminals) {
      if (terminal.fitAddon) {
        try {
          terminal.fitAddon.fit();
          // Notify backend of new dimensions
          if (terminal.sessionId && invoke) {
            window.xnautZellijSettle(terminal.sessionId);
            invoke('resize_terminal', {
              sessionId: terminal.sessionId,
              cols: terminal.term.cols,
              rows: terminal.term.rows
            }).catch(() => {});
          }
        } catch (e) {
          console.warn('Failed to fit terminal:', e);
        }
      }
    }
  }
}

function toggleChatPanel() {
  const panel = document.getElementById('chat-panel');
  const isHidden = panel.style.display === 'none' || !panel.style.display;

  if (isHidden) {
    panel.style.display = 'flex';
    // Open at 1/3 of screen width (min 300px, max 800px), user can then resize
    const targetWidth = Math.max(300, Math.min(800, Math.round(window.innerWidth / 3)));
    panel.style.width = targetWidth + 'px';
    // Force layout recalculation
    panel.offsetHeight;
    // Trigger reflow for all terminals
    requestAnimationFrame(() => {
      resizeAllTerminals();
    });
  } else {
    panel.style.display = 'none';
    // Trigger reflow for all terminals
    requestAnimationFrame(() => {
      resizeAllTerminals();
    });
  }
}



// ==================== Built-in File Editor ====================
const editorState = { path: null, originalContent: '', modified: false };

window.openFileInEditor = async function(filePath) {
  try {
    let content;
    try {
      content = await invoke('read_file', { path: filePath });
    } catch (readErr) {
      content = '// Could not read file: ' + readErr + '\n// Path: ' + filePath;
    }
    const panel = document.getElementById('editor-panel');
    const textarea = document.getElementById('editor-textarea');

    if (!panel) { alert('ERROR: editor-panel element not found in DOM'); return; }
    if (!textarea) { alert('ERROR: editor-textarea element not found in DOM'); return; }
    const preview = document.getElementById('editor-preview');
    const filename = document.getElementById('editor-filename');
    const modIndicator = document.getElementById('editor-modified');
    const previewBtn = document.getElementById('btn-editor-preview');

    if (!panel || !textarea) return;

    editorState.path = filePath;
    editorState.originalContent = content;
    editorState.modified = false;

    const name = filePath.split('/').pop();
    filename.textContent = name;
    modIndicator.style.display = 'none';
    textarea.value = content;

    const ext = name.split('.').pop()?.toLowerCase();
    const isMarkdown = ['md', 'markdown', 'mdx'].includes(ext);
    const highlighted = document.getElementById('editor-highlighted');
    const lineNumbers = document.getElementById('editor-line-numbers');

    // Map file extensions to highlight.js languages
    const langMap = {
      js: 'javascript', ts: 'typescript', py: 'python', rs: 'rust', go: 'go',
      rb: 'ruby', sh: 'bash', bash: 'bash', zsh: 'bash', fish: 'bash',
      json: 'json', yaml: 'yaml', yml: 'yaml', toml: 'ini',
      html: 'html', css: 'css', scss: 'scss', xml: 'xml', svg: 'xml',
      sql: 'sql', md: 'markdown', markdown: 'markdown',
      dockerfile: 'dockerfile', makefile: 'makefile',
      c: 'c', cpp: 'cpp', h: 'c', java: 'java', swift: 'swift',
      kt: 'kotlin', php: 'php', r: 'r', lua: 'lua', vim: 'vim',
    };
    const lang = langMap[ext] || 'plaintext';

    const lines = content.split('\n');
    if (lineNumbers) lineNumbers.style.display = 'none';

    if (isMarkdown && typeof marked !== 'undefined') {
      preview.innerHTML = marked.parse(content);
      preview.style.display = 'block';
      textarea.style.display = 'none';
      if (highlighted) highlighted.style.display = 'none';
      previewBtn.textContent = 'Edit';
    } else {
      preview.style.display = 'none';
      textarea.style.display = 'none';
      if (highlighted && typeof hljs !== 'undefined') {
        let highlightedCode;
        try {
          highlightedCode = hljs.highlight(content, { language: lang }).value;
        } catch (e) {
          highlightedCode = hljs.highlightAuto(content).value;
        }
        // Embed line numbers into each line
        const codeLines = highlightedCode.split('\n');
        const numberedLines = codeLines.map((line, i) =>
          '<span class="editor-ln">' + (i + 1) + '</span>' + line
        ).join('\n');
        highlighted.innerHTML = '<pre><code class="hljs">' + numberedLines + '</code></pre>';
        highlighted.style.display = 'block';
      }
      previewBtn.textContent = 'Edit';
    }

    panel.style.display = 'flex';

    // Track modifications
    textarea.oninput = () => {
      editorState.modified = textarea.value !== editorState.originalContent;
      modIndicator.style.display = editorState.modified ? 'inline' : 'none';
    };

    // Resize terminals
    requestAnimationFrame(() => resizeAllTerminals());
  } catch (e) {
    console.error('Failed to open file:', e);
    alert('Failed to open file: ' + e);
  }
}

window.saveEditorFile = async function() {
  if (!editorState.path) return;
  const textarea = document.getElementById('editor-textarea');
  if (!textarea) return;
  // Same rule as the markdown pane: while a slice is building, its worktree
  // belongs to the agent (XNAUT-106).
  if (window.xnautSliceWriteBlocked && window.xnautSliceWriteBlocked(editorState.path)) return;
  try {
    await invoke('write_file', { path: editorState.path, content: textarea.value });
    editorState.originalContent = textarea.value;
    editorState.modified = false;
    const modIndicator = document.getElementById('editor-modified');
    if (modIndicator) modIndicator.style.display = 'none';
  } catch (e) {
    console.error('Failed to save file:', e);
    alert('Failed to save: ' + e);
  }
}

window.closeEditor = async function() {
  if (editorState.modified) {
    if (!await window.xnautConfirmDialog('You have unsaved changes.', 'Close anyway',
      'The edits since the last save are lost.')) return;
  }
  const panel = document.getElementById('editor-panel');
  if (panel) panel.style.display = 'none';
  editorState.path = null;
  editorState.modified = false;
  requestAnimationFrame(() => resizeAllTerminals());
}

window.toggleEditorPreview = function() {
  const textarea = document.getElementById('editor-textarea');
  const preview = document.getElementById('editor-preview');
  const highlighted = document.getElementById('editor-highlighted');
  const lineNumbers = document.getElementById('editor-line-numbers');
  const btn = document.getElementById('btn-editor-preview');
  if (!textarea || !btn) return;

  const isEditing = textarea.style.display !== 'none';

  if (isEditing) {
    // Switch from edit mode to view mode
    if (highlighted) {
      // Re-highlight with current content
      const ext = (editorState.path || '').split('.').pop()?.toLowerCase();
      const isMarkdown = ['md', 'markdown', 'mdx'].includes(ext);
      if (isMarkdown && typeof marked !== 'undefined' && preview) {
        preview.innerHTML = marked.parse(textarea.value);
        preview.style.display = 'block';
        highlighted.style.display = 'none';
      } else if (typeof hljs !== 'undefined') {
        const code = hljs.highlightAuto(textarea.value).value;
        highlighted.innerHTML = '<pre><code class="hljs">' + code + '</code></pre>';
        highlighted.style.display = 'block';
        if (preview) preview.style.display = 'none';
      }
      if (lineNumbers) {
        lineNumbers.innerHTML = textarea.value.split('\n').map((_, i) => (i + 1)).join('\n');
        lineNumbers.style.display = 'block';
      }
    }
    textarea.style.display = 'none';
    btn.textContent = 'Edit';
  } else {
    // Switch to edit mode
    textarea.style.display = 'block';
    if (highlighted) highlighted.style.display = 'none';
    if (preview) preview.style.display = 'none';
    if (lineNumbers) {
      lineNumbers.innerHTML = textarea.value.split('\n').map((_, i) => (i + 1)).join('\n');
      lineNumbers.style.display = 'block';
      lineNumbers.style.overflow = 'hidden';
      textarea.onscroll = () => { lineNumbers.scrollTop = textarea.scrollTop; };
      textarea.oninput = () => {
        lineNumbers.innerHTML = textarea.value.split('\n').map((_, i) => (i + 1)).join('\n');
        const modIndicator = document.getElementById('editor-modified');
        editorState.modified = textarea.value !== editorState.originalContent;
        if (modIndicator) modIndicator.style.display = editorState.modified ? 'inline' : 'none';
      };
    }
    textarea.focus();
    btn.textContent = 'Preview';
  }
}

function formatFileSize(bytes) {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

async function insertPathToTerminal(path) {
  // Find the active tab
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || !tab.terminals || tab.terminals.length === 0) {
    console.error('No active terminal found');
    return;
  }

  // Get the focused terminal in the active tab
  const terminal = tab.terminals[tab.focusedPaneIndex || 0];
  const backendSessionId = terminal.sessionId;
  const term = terminal.term;

  if (term && backendSessionId) {
    // Insert path with quotes if it contains spaces, and add a space after
    const quotedPath = path.includes(' ') ? `"${path}" ` : `${path} `;

    // Send it to the PTY so the shell receives it
    try {
      await invoke('write_to_terminal', {
        sessionId: backendSessionId,
        data: quotedPath
      });
      console.log('✅ Path inserted:', quotedPath);
    } catch (error) {
      console.error('❌ Failed to write path to terminal:', error);
    }
  }
}

// Debug function
async function showDebugInfo() {
  let debugInfo = '🐛 xNAUT DEBUG INFO\n\n';

  // Check JavaScript
  debugInfo += '✅ JavaScript is running!\n\n';

  // Check Tauri API
  debugInfo += `Tauri API: ${window.__TAURI__ ? '✅ Available' : '❌ MISSING'}\n`;
  if (window.__TAURI__) {
    debugInfo += `  - invoke: ${typeof invoke === 'function' ? '✅' : '❌'}\n`;
    debugInfo += `  - listen: ${typeof listen === 'function' ? '✅' : '❌'}\n`;
  }
  debugInfo += '\n';

  // Check DOM Elements
  debugInfo += `DOM Elements:\n`;
  debugInfo += `  - statusDot: ${statusDot ? '✅' : '❌'}\n`;
  debugInfo += `  - statusText: ${statusText ? '✅' : '❌'}\n`;
  debugInfo += `  - tabsContainer: ${tabsContainer ? '✅' : '❌'}\n`;
  debugInfo += `  - terminalContainer: ${terminalContainer ? '✅' : '❌'}\n`;
  debugInfo += '\n';

  // Check State
  debugInfo += `Application State:\n`;
  debugInfo += `  - Tabs: ${tabs.length}\n`;
  debugInfo += `  - Active Tab: ${activeTabId || 'None'}\n`;
  debugInfo += `  - Settings loaded: ${Object.keys(settings).length > 0 ? '✅' : '❌'}\n`;
  debugInfo += '\n';

  // Test Tauri Command
  if (window.__TAURI__) {
    debugInfo += 'Testing Tauri command...\n';
    try {
      const result = await invoke('create_terminal_session');
      debugInfo += `✅ Command succeeded!\n`;
      debugInfo += `   Session ID: ${result.session_id}\n`;
    } catch (error) {
      debugInfo += `❌ Command FAILED:\n`;
      debugInfo += `   Error: ${error.message || error}\n`;
      debugInfo += `   Type: ${typeof error}\n`;
    }
  }

  alert(debugInfo);

  // Also log to console
  console.log(debugInfo);
}

// Utility functions
function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  // Also escape quotes for safe use inside HTML attributes (data-code="...")
  return div.innerHTML.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// Export for debugging
window.xnaut = {
  invoke,
  listen,
  tabs,
  settings,
  sshProfiles,
  triggers,
  getActiveTerminal: () => {
    const tab = tabs.find(t => t.id === activeTabId);
    return tab && tab.terminals.length > 0 ? tab.terminals[0] : null;
  }
};

console.log('🎯 XNAUT loaded. Access debug info via window.xnaut');
