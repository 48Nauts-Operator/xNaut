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
let detectedErrors = [];
let errorMonitorEnabled = true;
let errorSeverityFilter = null; // 'critical', 'warning', 'info', or null for all
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

async function splitPane(direction) {
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

  // Create new terminal pane as second child
  await createTerminal(tab.id, 'p' + (++sessionCounter), branch);

  // Refit all
  refitAllTerminals(tab);
  console.log('Split ' + direction + ': ' + tab.terminals.length + ' panes');
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

function refitAllTerminals(tab) {
  requestAnimationFrame(() => {
    setTimeout(() => {
      tab.terminals.forEach(t => {
        if (t.fitAddon) {
          try {
            t.fitAddon.fit();
            invoke('resize_terminal', { sessionId: t.sessionId, cols: t.term.cols, rows: t.term.rows }).catch(() => {});
          } catch (e) {}
        }
      });
    }, 100);
  });
}

// Close the currently focused pane
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
    await invoke('close_terminal', { sessionId: terminal.sessionId });
    window.removeEventListener('resize', terminal.handleResize);
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
      updateSharedStatusBar(terminal.sessionId);
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
    // Also update the shared status bar directly
    const sharedPath = document.getElementById('shared-status-path');
    if (sharedPath) {
      sharedPath.textContent = displayPath;
    }

    // Get git info for current directory
    try {
      const gitInfo = await invoke('get_git_info', { path: currentDir });

      const statusGit = document.getElementById(`status-git-${sessionId}`);
      const sharedGit = document.getElementById('shared-status-git');
      const gitHtml = gitInfo && gitInfo.is_repo
        ? `<span class="git-branch">⎇ ${gitInfo.branch}</span>${gitInfo.changes > 0 ? ` <span class="git-stats">• ${gitInfo.changes} ${gitInfo.changes === 1 ? 'change' : 'changes'}</span>` : ''}`
        : '';
      if (statusGit) statusGit.innerHTML = gitHtml;
      if (sharedGit) sharedGit.innerHTML = gitHtml;
    } catch (gitError) {
      // Not in a git repo, silently ignore
      const statusGit = document.getElementById(`status-git-${sessionId}`);
      if (statusGit) {
        statusGit.textContent = '';
      }
    }
  } catch (error) {
    console.error('Error updating directory status:', error);
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
  } catch (error) {
    console.error('❌ Failed to load Tauri API:', error);
    if (statusText) statusText.textContent = '❌ Tauri API Missing';

    alert('🚨 CRITICAL ERROR\n\nTauri API failed to load!\n\n' + error.message + '\n\nThe app cannot function without the Tauri API.\n\nPlease ensure you built the app correctly with:\ncd src-tauri\ncargo tauri build');
  }
});

async function init() {
  console.log('🚀 XNAUT Initializing...');
  console.log('✅ Tauri API available');

  try {
    await loadSettings();
    loadCommandHistory();
    await loadSSHProfiles();
    loadTriggers();
    initChatSessions(); // Initialize chat session history
    loadSnippets(); // Load command snippets
    requestNotificationPermission();

    initSharedStatusBar();
    detectAntBot();
    applyFileBrowserPosition();
    console.log('✅ Data loaded, setting up event listeners...');
    try {
      setupEventListeners();
      console.log('✅ Event listeners set up');
    } catch (err) {
      console.error('❌ Event listeners error:', err);
      throw err;
    }

    // Create initial terminal tab
    console.log('📝 Creating initial terminal tab...');
    createNewTab();

    console.log('✅ XNAUT Ready!');
    if (statusText) statusText.textContent = 'Ready';

    // Check for updates after startup
    setTimeout(() => checkForUpdates(), 3000);
    if (statusDot) statusDot.classList.add('connected');
  } catch (error) {
    console.error('❌ Initialization error:', error);
    if (statusText) statusText.textContent = '❌ Init Failed';
    alert(`Initialization failed!\n\n${error.message}\n\nClick the 🐛 bug button for more info.`);
  }
}

function updateStatus(message) {
  statusText.textContent = message;
}

// Shared Status Bar (one per app, controls apply to focused pane)
// ==================== Settings Panel ====================
// ==================== Auto-Update ====================
async function checkForUpdates() {
  try {
    if (!window.__TAURI__) return;
    const { check } = window.__TAURI__['updater'] || {};
    if (!check) {
      console.log('Updater plugin not available, checking GitHub API...');
      // Fallback: check GitHub releases API directly
      const resp = await fetch('https://api.github.com/repos/48Nauts-Operator/xNaut/releases/latest');
      if (!resp.ok) return;
      const release = await resp.json();
      const latestVersion = release.tag_name?.replace('v', '');
      const currentVersion = '1.4.0';
      if (latestVersion && latestVersion !== currentVersion && latestVersion > currentVersion) {
        showUpdateBanner(latestVersion, release.html_url);
      }
      return;
    }
    const update = await check();
    if (update?.available) {
      showUpdateBanner(update.version, null, update);
    }
  } catch (e) {
    console.log('Update check skipped:', e);
  }
}

function showUpdateBanner(version, downloadUrl, updateObj) {
  const existing = document.getElementById('update-banner');
  if (existing) existing.remove();

  const banner = document.createElement('div');
  banner.id = 'update-banner';
  banner.style.cssText = 'position:fixed; top:0; left:0; right:0; z-index:9999; background:linear-gradient(90deg, #3b82f6, #6366f1); color:white; padding:8px 16px; display:flex; justify-content:center; align-items:center; gap:12px; font-size:13px; font-weight:500;';

  const text = document.createElement('span');
  text.textContent = 'xNAUT v' + version + ' is available!';

  const updateBtn = document.createElement('button');
  updateBtn.textContent = 'Update Now';
  updateBtn.style.cssText = 'background:white; color:#3b82f6; border:none; padding:4px 16px; border-radius:4px; font-size:12px; font-weight:600; cursor:pointer;';
  updateBtn.onclick = async () => {
    if (updateObj && updateObj.downloadAndInstall) {
      updateBtn.textContent = 'Downloading...';
      updateBtn.disabled = true;
      try {
        await updateObj.downloadAndInstall();
        // Tauri will restart automatically
      } catch (e) {
        updateBtn.textContent = 'Failed — retry';
        updateBtn.disabled = false;
      }
    } else if (downloadUrl) {
      window.__TAURI__?.shell?.open(downloadUrl) || window.open(downloadUrl, '_blank');
    }
  };

  const dismiss = document.createElement('button');
  dismiss.textContent = '×';
  dismiss.style.cssText = 'background:none; border:none; color:white; cursor:pointer; font-size:18px; margin-left:8px;';
  dismiss.onclick = () => banner.remove();

  banner.appendChild(text);
  banner.appendChild(updateBtn);
  banner.appendChild(dismiss);
  document.body.appendChild(banner);
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

// Load custom themes from localStorage on startup
function loadCustomThemes() {
  try {
    const custom = JSON.parse(localStorage.getItem('xnaut-custom-themes') || '{}');
    Object.assign(THEME_PRESETS, custom);
  } catch (e) {}
}
loadCustomThemes();

function applyFileBrowserPosition() {
  const panel = document.getElementById('files-panel');
  const container = document.querySelector('.main-container');
  if (!panel || !container) return;
  const pos = settings.fileBrowserPosition || 'left';
  if (pos === 'right') {
    // Move files panel to end of container
    container.appendChild(panel);
    panel.style.borderRight = 'none';
    panel.style.borderLeft = '1px solid var(--border)';
  } else {
    // Move files panel to start of container
    container.insertBefore(panel, container.firstChild);
    panel.style.borderLeft = 'none';
    panel.style.borderRight = '1px solid var(--border)';
  }
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
          <button class="btn-test" onclick="testProvider('ollama')">Test</button>
        </div>
        <div class="settings-row">
          <label><span class="status-dot-sm gray" id="lmstudio-status"></span>LM Studio</label>
          <input type="url" id="set-lmstudio-url" value="${settings.lmstudioUrl || 'http://localhost:1234'}" placeholder="http://localhost:1234">
          <button class="btn-test" onclick="testProvider('lmstudio')">Test</button>
        </div>
        <div class="settings-row">
          <label><span class="status-dot-sm gray" id="antbot-status"></span>AntBot</label>
          <span style="color:var(--text-secondary); font-size:12px;">Auto-detected via CLI</span>
          <button class="btn-test" onclick="testProvider('antbot')">Test</button>
        </div>
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
      </div>
      <div class="settings-group">
        <h4>Default Model</h4>
        <div class="settings-row">
          <label>Provider</label>
          <select id="set-default-provider" onchange="updateModelDropdown()">
            <option value="ollama" ${settings.llmProvider === 'ollama' ? 'selected' : ''}>Ollama (Local)</option>
            <option value="lmstudio" ${settings.llmProvider === 'lmstudio' ? 'selected' : ''}>LM Studio (Local)</option>
            <option value="antbot" ${settings.llmProvider === 'antbot' ? 'selected' : ''}>AntBot (Local)</option>
            <option value="anthropic" ${settings.llmProvider === 'anthropic' ? 'selected' : ''}>Anthropic</option>
            <option value="openai" ${settings.llmProvider === 'openai' ? 'selected' : ''}>OpenAI</option>
            <option value="openrouter" ${settings.llmProvider === 'openrouter' ? 'selected' : ''}>OpenRouter</option>
            <option value="perplexity" ${settings.llmProvider === 'perplexity' ? 'selected' : ''}>Perplexity</option>
          </select>
        </div>
        <div class="settings-row">
          <label>Model</label>
          <select id="set-default-model"></select>
        </div>
      </div>
      <div class="settings-group">
        <h4>MCP Servers</h4>
        <p style="color:var(--text-secondary); font-size:12px;">Coming soon — configure Model Context Protocol servers</p>
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
      <button class="btn btn-primary" onclick="saveAISettings()" style="width:100%; margin-top:8px;">Save AI Settings</button>
    `,
    appearance: () => {
      const themeGrid = Object.entries(THEME_PRESETS).map(([name, colors]) =>
        `<div class="theme-card" data-theme="${name}" style="cursor:pointer; padding:8px 12px; border-radius:6px; border:1px solid ${settings.activeTheme === name ? 'var(--accent)' : 'var(--border)'}; background:${settings.activeTheme === name ? 'rgba(59,130,246,0.1)' : 'transparent'}; display:flex; justify-content:space-between; align-items:center;">
          <div style="display:flex; align-items:center; gap:10px;">
            <span style="width:20px; height:20px; border-radius:4px; background:${colors.bg}; border:1px solid var(--border); display:inline-block;"></span>
            <span style="font-size:13px;">${name}</span>
          </div>
          <span style="display:flex; gap:3px;">${[colors.red, colors.green, colors.blue, colors.yellow, colors.magenta, colors.cyan].map(c => '<span style="width:8px; height:8px; border-radius:50%; background:' + c + ';"></span>').join('')}</span>
        </div>`
      ).join('');
      return `
        <h3>Theme</h3>
        <div class="settings-group" style="display:flex; flex-direction:column; gap:4px;">${themeGrid}</div>
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
        <h3>Layout</h3>
        <div class="settings-group">
          <div class="settings-row">
            <label>File Browser Position</label>
            <select id="set-filebrowser-pos">
              <option value="left" ${(settings.fileBrowserPosition || 'left') === 'left' ? 'selected' : ''}>Left</option>
              <option value="right" ${settings.fileBrowserPosition === 'right' ? 'selected' : ''}>Right</option>
            </select>
          </div>
        </div>
        <button id="btn-save-appearance" class="btn btn-primary" style="width:100%; margin-top:8px;">Save Appearance</button>
      `;
    },
    shortcuts: () => {
      const rows = Object.entries(keybindings).map(([action, binding]) =>
        `<div class="settings-row" data-action="${action}">
          <label>${binding.label || action}</label>
          <button class="btn-test" style="min-width:100px; font-family:monospace;" onclick="rebindKey('${action}', this)">${formatBinding(binding)}</button>
        </div>`
      ).join('');
      return `
        <h3>Keyboard Shortcuts</h3>
        <div class="settings-group">${rows}</div>
        <button class="btn btn-primary" onclick="resetAllKeybindings()" style="width:100%; margin-top:8px; background:#6c757d;">Reset All to Defaults</button>
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
        <button class="btn btn-primary" onclick="toggleSettingsPanel(); showModal('ssh-modal'); loadSSHProfiles();" style="width:100%; margin-top:8px;">Manage SSH Profiles</button>
      </div>
      <button class="btn btn-primary" onclick="saveNautifySettings()" style="width:100%; margin-top:8px;">Save Shell Settings</button>
    `,
    triggers: () => `
      <h3>Triggers & Notifications</h3>
      <p style="color:var(--text-secondary); font-size:13px; margin-bottom:16px;">Pattern-match terminal output and trigger actions automatically.</p>
      <div class="settings-group" id="triggers-settings-list"></div>
      <button class="btn btn-primary" onclick="toggleSettingsPanel(); showModal('triggers-modal'); renderTriggers();" style="width:100%; margin-top:8px;">Manage Triggers</button>
    `,
  };

  content.innerHTML = (sections[section] || sections.ai)();

  // Post-render hooks
  if (section === 'ai') {
    updateModelDropdown();
  }
  if (section === 'appearance') {
    const slider = document.getElementById('set-opacity');
    const val = document.getElementById('set-opacity-val');
    if (slider && val) slider.oninput = () => { val.textContent = slider.value + '%'; };

    // Theme card clicks
    document.querySelectorAll('.theme-card').forEach(card => {
      card.onclick = () => applyThemeFromSettings(card.dataset.theme);
    });

    // Save appearance button
    // File browser position
    const posSelect = document.getElementById('set-filebrowser-pos');
    if (posSelect) posSelect.onchange = () => {
      settings.fileBrowserPosition = posSelect.value;
      applyFileBrowserPosition();
      localStorage.setItem('xnaut-settings', JSON.stringify(settings));
    };

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
  antbot: [
    { id: 'local', name: 'Local LLM (Auto-detect)' },
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
      const resp = await fetch(url + '/api/tags');
      const data = await resp.json();
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
      const resp = await fetch(url + '/v1/models');
      const data = await resp.json();
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
  const models = MODEL_OPTIONS[provider] || [];
  select.innerHTML = models.map(m =>
    '<option value="' + m.id + '"' + (settings.llmModel === m.id ? ' selected' : '') + '>' + m.name + '</option>'
  ).join('');
};

window.saveAISettings = function() {
  settings.ollamaUrl = document.getElementById('set-ollama-url')?.value;
  settings.lmstudioUrl = document.getElementById('set-lmstudio-url')?.value;
  settings.apiKeyAnthropic = document.getElementById('set-api-anthropic')?.value;
  settings.apiKeyOpenAI = document.getElementById('set-api-openai')?.value;
  settings.apiKeyOpenRouter = document.getElementById('set-api-openrouter')?.value;
  settings.apiKeyPerplexity = document.getElementById('set-api-perplexity')?.value;
  settings.llmProvider = document.getElementById('set-default-provider')?.value;
  settings.llmModel = document.getElementById('set-default-model')?.value;
  settings.voiceEnabled = document.getElementById('set-voice-enabled')?.checked;
  settings.kokoroUrl = document.getElementById('set-kokoro-url')?.value;
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));
};

window.saveAppearanceSettings = function() {
  settings.terminalFontFamily = document.getElementById('set-font-family')?.value;
  settings.fontSize = parseInt(document.getElementById('set-font-size')?.value) || 14;
  settings.terminalOpacity = parseInt(document.getElementById('set-opacity')?.value) ?? 100;
  settings.fontLigatures = document.getElementById('set-ligatures')?.value || 'normal';
  settings.fileBrowserPosition = document.getElementById('set-filebrowser-pos')?.value || 'left';
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));
  applyAppearanceToAllTerminals();
  applyFileBrowserPosition();
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

window.saveNautifySettings = function() {
  settings.shellType = document.getElementById('set-shell-type')?.value;
  localStorage.setItem('xnaut-settings', JSON.stringify(settings));
};

window.testProvider = async function(provider) {
  const dot = document.getElementById(provider + '-status');
  if (dot) dot.className = 'status-dot-sm gray';
  try {
    if (provider === 'antbot') {
      const result = await invoke('check_antbot');
      if (dot) dot.className = 'status-dot-sm ' + (result.available ? 'green' : 'red');
    } else if (provider === 'ollama') {
      const url = document.getElementById('set-ollama-url')?.value || 'http://localhost:11434';
      const resp = await fetch(url + '/api/tags');
      if (dot) dot.className = 'status-dot-sm ' + (resp.ok ? 'green' : 'red');
    } else if (provider === 'lmstudio') {
      const url = document.getElementById('set-lmstudio-url')?.value || 'http://localhost:1234';
      const resp = await fetch(url + '/v1/models');
      if (dot) dot.className = 'status-dot-sm ' + (resp.ok ? 'green' : 'red');
    }
  } catch (e) {
    if (dot) dot.className = 'status-dot-sm red';
  }
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

// AntBot Detection
async function detectAntBot() {
  try {
    const result = await invoke('check_antbot');
    if (result.available) {
      console.log('🐜 AntBot detected:', result.version);
      const item = document.getElementById('antbot-provider-item');
      if (item) item.style.display = '';
    }
  } catch (e) {
    console.log('🐜 AntBot not available');
  }
}

// Terminal Management
async function createTerminal(tabId, paneId, parentContainer) {
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
    fontSize: settings.fontSize || 14,
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
    console.log('🔄 Attempting to create terminal session...');

    // Determine shell to use
    let shell = null;
    if (settings.shellType && settings.shellType !== 'default') {
      shell = settings.shellType === 'custom' ? settings.customShell : settings.shellType;
      console.log(`Using custom shell: ${shell}`);
    }

    // Create terminal session via Tauri
    const config = shell ? { shell } : null;
    const result = await invoke('create_terminal_session', { config });
    console.log('📦 Terminal session result:', result);

    if (!result || !result.session_id) {
      throw new Error('Invalid response from create_terminal_session: ' + JSON.stringify(result));
    }

    const backendSessionId = result.session_id;
    console.log(`✅ Terminal session created: ${backendSessionId}`);

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
      console.log('📥 Received terminal output:', event.payload);
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

      // Parse for errors
      parseTerminalOutput(cleanData);

      // Keep buffer manageable
      if (terminalOutputBuffer.length > maxBufferSize) {
        terminalOutputBuffer = terminalOutputBuffer.slice(-maxBufferSize);
      }
    });

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
    fontSize: settings.fontSize || 14,
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

  setTimeout(() => {
    fitAddon.fit();
  }, 10);

  // Handle resize
  const handleResize = () => {
    if (term && fitAddon) {
      fitAddon.fit();
    }
  };
  window.addEventListener('resize', handleResize);

  // Listen for SSH output (already set up in connectSSH, but ensure it writes to this terminal)
  await listen(`ssh-output-${sshSessionId}`, (event) => {
    if (term) {
      term.write(event.payload.data);
    }
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

  tabs.push(tab);
  renderTabs();
  switchTab(tabId);
}

function renderTabs() {
  tabsContainer.innerHTML = '';
  tabs.forEach(tab => {
    const tabEl = document.createElement('div');
    tabEl.className = `tab ${tab.id === activeTabId ? 'active' : ''}`;
    tabEl.dataset.sessionId = tab.id;

    // Add backendSessionId if available
    if (tab.terminals && tab.terminals.length > 0) {
      tabEl.dataset.backendSessionId = tab.terminals[0].sessionId;
    }

    tabEl.innerHTML = `
      <span>${tab.name}</span>
      <button class="tab-close" data-tab-id="${tab.id}">×</button>
    `;

    // Handle tab switching
    tabEl.addEventListener('click', (e) => {
      if (!e.target.classList.contains('tab-close')) {
        switchTab(tab.id);
      }
    });

    // Handle close button
    const closeBtn = tabEl.querySelector('.tab-close');
    closeBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeTab(tab.id);
    });

    tabsContainer.appendChild(tabEl);
  });
}

async function switchTab(tabId) {
  activeTabId = tabId;
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
      // Check if this is an SSH tab that needs a terminal UI
      if (tab.isSSH && tab.sshSessionId) {
        console.log('📡 Creating terminal UI for SSH session:', tab.sshSessionId);
        await createSSHTerminal(tabId, tab.sshSessionId);
      } else {
        await createTerminal(tabId);
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
        focusedTerminal.term.focus();
      }
    }
  }

  renderTabs();
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
      console.log('  🔌 Closing terminal session:', terminal.sessionId);
      await invoke('close_terminal', { sessionId: terminal.sessionId });
      window.removeEventListener('resize', terminal.handleResize);
    } catch (error) {
      console.error('Error closing terminal:', error);
    }
  }

  tabs.splice(tabIndex, 1);
  console.log('📊 Remaining tabs:', tabs.length);

  if (tabs.length === 0) {
    console.log('🆕 No tabs left, creating new tab');
    createNewTab();
  } else if (activeTabId === tabId) {
    console.log('🔄 Closed active tab, switching to first tab');
    switchTab(tabs[0].id);
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
const THEME_PRESETS = {
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
  'Nord': {
    bg: '#2e3440', fg: '#d8dee9', cursor: '#88c0d0', chrome: '#242933', selection: 'rgba(67,76,94,0.6)',
    black: '#3b4252', red: '#bf616a', green: '#a3be8c', yellow: '#ebcb8b', blue: '#81a1c1', magenta: '#b48ead', cyan: '#88c0d0', white: '#e5e9f0',
    brightBlack: '#4c566a', brightRed: '#bf616a', brightGreen: '#a3be8c', brightYellow: '#ebcb8b', brightBlue: '#81a1c1', brightMagenta: '#b48ead', brightCyan: '#8fbcbb', brightWhite: '#eceff4',
  },
  'Solarized Dark': {
    bg: '#002b36', fg: '#839496', cursor: '#b58900', chrome: '#001f27', selection: 'rgba(7,54,66,0.7)',
    black: '#073642', red: '#dc322f', green: '#859900', yellow: '#b58900', blue: '#268bd2', magenta: '#d33682', cyan: '#2aa198', white: '#eee8d5',
    brightBlack: '#586e75', brightRed: '#cb4b16', brightGreen: '#859900', brightYellow: '#b58900', brightBlue: '#268bd2', brightMagenta: '#6c71c4', brightCyan: '#2aa198', brightWhite: '#fdf6e3',
  },
  'Monokai': {
    bg: '#272822', fg: '#f8f8f2', cursor: '#f92672', chrome: '#1e1f1c', selection: 'rgba(73,72,62,0.6)',
    black: '#272822', red: '#f92672', green: '#a6e22e', yellow: '#f4bf75', blue: '#66d9ef', magenta: '#ae81ff', cyan: '#a1efe4', white: '#f8f8f2',
    brightBlack: '#75715e', brightRed: '#f92672', brightGreen: '#a6e22e', brightYellow: '#f4bf75', brightBlue: '#66d9ef', brightMagenta: '#ae81ff', brightCyan: '#a1efe4', brightWhite: '#f9f8f5',
  },
  'Gruvbox Dark': {
    bg: '#282828', fg: '#ebdbb2', cursor: '#fe8019', chrome: '#1d2021', selection: 'rgba(60,56,54,0.7)',
    black: '#282828', red: '#cc241d', green: '#98971a', yellow: '#d79921', blue: '#458588', magenta: '#b16286', cyan: '#689d6a', white: '#a89984',
    brightBlack: '#928374', brightRed: '#fb4934', brightGreen: '#b8bb26', brightYellow: '#fabd2f', brightBlue: '#83a598', brightMagenta: '#d3869b', brightCyan: '#8ec07c', brightWhite: '#ebdbb2',
  },
  'One Dark': {
    bg: '#282c34', fg: '#abb2bf', cursor: '#61afef', chrome: '#21252b', selection: 'rgba(62,68,81,0.6)',
    black: '#282c34', red: '#e06c75', green: '#98c379', yellow: '#e5c07b', blue: '#61afef', magenta: '#c678dd', cyan: '#56b6c2', white: '#abb2bf',
    brightBlack: '#5c6370', brightRed: '#e06c75', brightGreen: '#98c379', brightYellow: '#e5c07b', brightBlue: '#61afef', brightMagenta: '#c678dd', brightCyan: '#56b6c2', brightWhite: '#ffffff',
  },
  'Tokyo Night': {
    bg: '#1a1b26', fg: '#a9b1d6', cursor: '#7aa2f7', chrome: '#16161e', selection: 'rgba(51,59,97,0.6)',
    black: '#15161e', red: '#f7768e', green: '#9ece6a', yellow: '#e0af68', blue: '#7aa2f7', magenta: '#bb9af7', cyan: '#7dcfff', white: '#a9b1d6',
    brightBlack: '#414868', brightRed: '#f7768e', brightGreen: '#9ece6a', brightYellow: '#e0af68', brightBlue: '#7aa2f7', brightMagenta: '#bb9af7', brightCyan: '#7dcfff', brightWhite: '#c0caf5',
  },
  'Catppuccin Mocha': {
    bg: '#1e1e2e', fg: '#cdd6f4', cursor: '#f5c2e7', chrome: '#181825', selection: 'rgba(88,91,112,0.5)',
    black: '#45475a', red: '#f38ba8', green: '#a6e3a1', yellow: '#f9e2af', blue: '#89b4fa', magenta: '#f5c2e7', cyan: '#94e2d5', white: '#bac2de',
    brightBlack: '#585b70', brightRed: '#f38ba8', brightGreen: '#a6e3a1', brightYellow: '#f9e2af', brightBlue: '#89b4fa', brightMagenta: '#f5c2e7', brightCyan: '#94e2d5', brightWhite: '#a6adc8',
  },
  'Rosé Pine': {
    bg: '#191724', fg: '#e0def4', cursor: '#ebbcba', chrome: '#1f1d2e', selection: 'rgba(110,106,134,0.4)',
    black: '#26233a', red: '#eb6f92', green: '#31748f', yellow: '#f6c177', blue: '#9ccfd8', magenta: '#c4a7e7', cyan: '#ebbcba', white: '#e0def4',
    brightBlack: '#6e6a86', brightRed: '#eb6f92', brightGreen: '#31748f', brightYellow: '#f6c177', brightBlue: '#9ccfd8', brightMagenta: '#c4a7e7', brightCyan: '#ebbcba', brightWhite: '#e0def4',
  },
  'Kanagawa': {
    bg: '#1f1f28', fg: '#dcd7ba', cursor: '#c8c093', chrome: '#16161d', selection: 'rgba(73,73,95,0.5)',
    black: '#16161d', red: '#c34043', green: '#76946a', yellow: '#c0a36e', blue: '#7e9cd8', magenta: '#957fb8', cyan: '#6a9589', white: '#c8c093',
    brightBlack: '#727169', brightRed: '#e82424', brightGreen: '#98bb6c', brightYellow: '#e6c384', brightBlue: '#7fb4ca', brightMagenta: '#938aa9', brightCyan: '#7aa89f', brightWhite: '#dcd7ba',
  },
  'Solarized Light': {
    bg: '#fdf6e3', fg: '#657b83', cursor: '#268bd2', chrome: '#eee8d5', selection: 'rgba(7,54,66,0.15)',
    black: '#073642', red: '#dc322f', green: '#859900', yellow: '#b58900', blue: '#268bd2', magenta: '#d33682', cyan: '#2aa198', white: '#eee8d5',
    brightBlack: '#586e75', brightRed: '#cb4b16', brightGreen: '#859900', brightYellow: '#b58900', brightBlue: '#268bd2', brightMagenta: '#6c71c4', brightCyan: '#2aa198', brightWhite: '#fdf6e3',
  },
  // --- Warp Community Themes (from github.com/warpdotdev/themes) ---
  'Cyberpunk Neon': {
    bg: '#000b1e', fg: '#0abdc6', cursor: '#0abdc6', chrome: '#000814', selection: 'rgba(255,255,255,0.2)',
    black: '#123e7c', red: '#ff0000', green: '#d300c4', yellow: '#f57800', blue: '#123e7c', magenta: '#711c91', cyan: '#0abdc6', white: '#d7d7d5',
    brightBlack: '#1c61c2', brightRed: '#ff0000', brightGreen: '#d300c4', brightYellow: '#f57800', brightBlue: '#00ff00', brightMagenta: '#711c91', brightCyan: '#0abdc6', brightWhite: '#d7d7d5',
  },
  'Night Owl': {
    bg: '#011627', fg: '#d6deeb', cursor: '#7e57c2', chrome: '#010f1e', selection: 'rgba(255,255,255,0.2)',
    black: '#011627', red: '#EF5350', green: '#22da6e', yellow: '#addb67', blue: '#82aaff', magenta: '#c792ea', cyan: '#21c7a8', white: '#ffffff',
    brightBlack: '#575656', brightRed: '#ef5350', brightGreen: '#22da6e', brightYellow: '#ffeb95', brightBlue: '#82aaff', brightMagenta: '#c792ea', brightCyan: '#7fdbca', brightWhite: '#ffffff',
  },
  'Synthwave 84': {
    bg: '#191621', fg: '#f1f1f1', cursor: '#f92aad', chrome: '#14111c', selection: 'rgba(255,255,255,0.2)',
    black: '#382C4D', red: '#fe4450', green: '#72f1b8', yellow: '#fede5d', blue: '#34d3fb', magenta: '#f92aad', cyan: '#36f9f6', white: '#f2ebe0',
    brightBlack: '#7b6a98', brightRed: '#ff7d86', brightGreen: '#c6ffe5', brightYellow: '#fff0b4', brightBlue: '#aaeeff', brightMagenta: '#ffa8df', brightCyan: '#9efffd', brightWhite: '#f1f1f1',
  },
  'Material Theme': {
    bg: '#1e282d', fg: '#c4c7d1', cursor: '#80cbc4', chrome: '#182125', selection: 'rgba(255,255,255,0.2)',
    black: '#666666', red: '#eb606b', green: '#c3e88d', yellow: '#f7eb95', blue: '#80cbc4', magenta: '#ff2f90', cyan: '#aeddff', white: '#ffffff',
    brightBlack: '#ff262b', brightRed: '#eb606b', brightGreen: '#c3e88d', brightYellow: '#f7eb95', brightBlue: '#7dc6bf', brightMagenta: '#6c71c4', brightCyan: '#35434d', brightWhite: '#ffffff',
  },
  'Everforest': {
    bg: '#2b3339', fg: '#d3c6aa', cursor: '#7a8478', chrome: '#232a2f', selection: 'rgba(255,255,255,0.2)',
    black: '#445055', red: '#e67e80', green: '#a7c080', yellow: '#dbbc7f', blue: '#7fbbb3', magenta: '#d699b6', cyan: '#83c092', white: '#d3c6aa',
    brightBlack: '#445055', brightRed: '#e67e80', brightGreen: '#a7c080', brightYellow: '#dbbc7f', brightBlue: '#7fbbb3', brightMagenta: '#d699b6', brightCyan: '#83c092', brightWhite: '#d3c6aa',
  },
  'Challenger Deep': {
    bg: '#1e1c31', fg: '#cbe1e7', cursor: '#65b2ff', chrome: '#181630', selection: 'rgba(255,255,255,0.2)',
    black: '#141228', red: '#ff5458', green: '#62d196', yellow: '#ffb378', blue: '#65b2ff', magenta: '#906cff', cyan: '#63f2f1', white: '#a6b3cc',
    brightBlack: '#565575', brightRed: '#ff8080', brightGreen: '#95ffa4', brightYellow: '#ffe9aa', brightBlue: '#91ddff', brightMagenta: '#c991e1', brightCyan: '#aaffe4', brightWhite: '#cbe3e7',
  },
  'Poimandres': {
    bg: '#1B1E28', fg: '#ACCDFF', cursor: '#C5E9FF', chrome: '#161821', selection: 'rgba(255,255,255,0.2)',
    black: '#1B1E28', red: '#679DFF', green: '#E4C7FF', yellow: '#FAC2FF', blue: '#DDFFFF', magenta: '#C5E9FF', cyan: '#DDFFFF', white: '#FFFFFF',
    brightBlack: '#ACCDFF', brightRed: '#679DFF', brightGreen: '#E4C7FF', brightYellow: '#FAC2FF', brightBlue: '#D7FFFF', brightMagenta: '#C5E9FF', brightCyan: '#D7FFFF', brightWhite: '#FFFFFF',
  },
  'Tokyo Night Storm': {
    bg: '#24283b', fg: '#a9b1d6', cursor: '#7aa2f7', chrome: '#1e2233', selection: 'rgba(255,255,255,0.2)',
    black: '#32344a', red: '#f7768e', green: '#9ece6a', yellow: '#e0af68', blue: '#7aa2f7', magenta: '#ad8ee6', cyan: '#449dab', white: '#9699a8',
    brightBlack: '#444b6a', brightRed: '#ff7a93', brightGreen: '#b9f27c', brightYellow: '#ff9e64', brightBlue: '#7da6ff', brightMagenta: '#bb9af7', brightCyan: '#0db9d7', brightWhite: '#acb0d0',
  },
  'Halcyon': {
    bg: '#1d2433', fg: '#a2aabc', cursor: '#8695b7', chrome: '#181e2b', selection: 'rgba(255,255,255,0.2)',
    black: '#8695b7', red: '#f07078', green: '#bae67e', yellow: '#ffd580', blue: '#5ccfe6', magenta: '#c3a6ff', cyan: '#5ccfe6', white: '#d7dce2',
    brightBlack: '#171c28', brightRed: '#ef6b73', brightGreen: '#bae67e', brightYellow: '#ffd580', brightBlue: '#5ccfe6', brightMagenta: '#c3a6ff', brightCyan: '#5ccfe6', brightWhite: '#d7dce2',
  },
  'Outrun': {
    bg: '#0c0a20', fg: '#7984D1', cursor: '#fc28a8', chrome: '#080618', selection: 'rgba(255,255,255,0.2)',
    black: '#283034', red: '#ff0081', green: '#a7da1e', yellow: '#f7b83d', blue: '#1ea8fc', magenta: '#A875FF', cyan: '#16f1fc', white: '#f9faff',
    brightBlack: '#435056', brightRed: '#ff2e97', brightGreen: '#cbfc44', brightYellow: '#ffd400', brightBlue: '#42c6ff', brightMagenta: '#ff2afc', brightCyan: '#39fff6', brightWhite: '#ffffff',
  },
  'Shades of Purple': {
    bg: '#2d2b55', fg: '#ffffff', cursor: '#fad000', chrome: '#252348', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#ec3a37', green: '#3ad900', yellow: '#fad000', blue: '#7857fe', magenta: '#ff2c70', cyan: '#80fcff', white: '#ffffff',
    brightBlack: '#5c5c61', brightRed: '#ec3a37', brightGreen: '#3ad900', brightYellow: '#fad000', brightBlue: '#6943ff', brightMagenta: '#fb94ff', brightCyan: '#80fcff', brightWhite: '#ffffff',
  },
  'Spaceduck': {
    bg: '#0f111b', fg: '#ecf0c1', cursor: '#b3a1e6', chrome: '#0a0c15', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#e33400', green: '#5ccc96', yellow: '#b3a1e6', blue: '#00a3cc', magenta: '#f2ce00', cyan: '#7a5ccc', white: '#686f9a',
    brightBlack: '#686f9a', brightRed: '#e33400', brightGreen: '#5ccc96', brightYellow: '#b3a1e6', brightBlue: '#00a3cc', brightMagenta: '#f2ce00', brightCyan: '#7a5ccc', brightWhite: '#f0f1ce',
  },
  'Cobalt 2': {
    bg: '#122637', fg: '#ffffff', cursor: '#1460d2', chrome: '#0e1e2d', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#ff0000', green: '#37dd21', yellow: '#fee409', blue: '#1460d2', magenta: '#ff005d', cyan: '#00bbbb', white: '#bbbbbb',
    brightBlack: '#545454', brightRed: '#f40d17', brightGreen: '#3bcf1d', brightYellow: '#ecc809', brightBlue: '#5555ff', brightMagenta: '#ff55ff', brightCyan: '#6ae3f9', brightWhite: '#ffffff',
  },
  'GitHub Dark': {
    bg: '#0d1117', fg: '#c9d1d9', cursor: '#F78166', chrome: '#090e14', selection: 'rgba(255,255,255,0.2)',
    black: '#0d1117', red: '#ff7b72', green: '#3fb950', yellow: '#d29922', blue: '#58a6ff', magenta: '#bc8cff', cyan: '#76e3ea', white: '#b1bac4',
    brightBlack: '#161b22', brightRed: '#ffa198', brightGreen: '#56d364', brightYellow: '#e3b341', brightBlue: '#79c0ff', brightMagenta: '#d2a8ff', brightCyan: '#b3f0ff', brightWhite: '#b1bac4',
  },
  'Lucario': {
    bg: '#2B3E50', fg: '#F8F8F2', cursor: '#F8F8F2', chrome: '#233241', selection: 'rgba(255,255,255,0.2)',
    black: '#4F4F4F', red: '#FF6C60', green: '#FBB036', yellow: '#FFFFB6', blue: '#5796ED', magenta: '#FF73FD', cyan: '#8EE478', white: '#EEEEEE',
    brightBlack: '#4F4F4F', brightRed: '#FA6960', brightGreen: '#FBB036', brightYellow: '#FEFFB9', brightBlue: '#6B9FED', brightMagenta: '#FC6FFA', brightCyan: '#8EE478', brightWhite: '#FFFFFF',
  },
  'Matrix': {
    bg: '#282a36', fg: '#00ff51', cursor: '#00c2ff', chrome: '#21232e', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#c91b00', green: '#00c200', yellow: '#c7c400', blue: '#3650c2', magenta: '#c930c7', cyan: '#00c5c7', white: '#c7c7c7',
    brightBlack: '#676767', brightRed: '#ff6d67', brightGreen: '#00f74e', brightYellow: '#fefb67', brightBlue: '#6871ff', brightMagenta: '#ff76ff', brightCyan: '#5ffdff', brightWhite: '#fffefe',
  },
  'Jellybeans': {
    bg: '#121212', fg: '#dedede', cursor: '#e1c0fa', chrome: '#0d0d0d', selection: 'rgba(255,255,255,0.2)',
    black: '#929292', red: '#e27373', green: '#94b979', yellow: '#ffba7b', blue: '#97bedc', magenta: '#e1c0fa', cyan: '#00988e', white: '#dedede',
    brightBlack: '#929292', brightRed: '#ffa1a1', brightGreen: '#94b979', brightYellow: '#ffdca0', brightBlue: '#97bedc', brightMagenta: '#e1c0fa', brightCyan: '#00988e', brightWhite: '#ffffff',
  },
  'Panda': {
    bg: '#25282a', fg: '#f3f2f2', cursor: '#65bcfe', chrome: '#1e2022', selection: 'rgba(255,255,255,0.2)',
    black: '#29292a', red: '#fe2b6c', green: '#14fbdc', yellow: '#feb76b', blue: '#6db0fe', magenta: '#fe74b4', cyan: '#15fbdb', white: '#f3f2f2',
    brightBlack: '#6f7683', brightRed: '#fe2b6c', brightGreen: '#14fbdc', brightYellow: '#ffc88f', brightBlue: '#65bcfe', brightMagenta: '#fea9d8', brightCyan: '#15fbdb', brightWhite: '#f3f2f2',
  },
  'Horizon Dark': {
    bg: '#1c1e26', fg: '#e0e0e0', cursor: '#26bbd9', chrome: '#171920', selection: 'rgba(255,255,255,0.2)',
    black: '#16161c', red: '#e95678', green: '#29d398', yellow: '#fab795', blue: '#26bbd9', magenta: '#ee64ac', cyan: '#59e1e3', white: '#d5d8da',
    brightBlack: '#5b5858', brightRed: '#ec6a88', brightGreen: '#3fdaa4', brightYellow: '#fbc3a7', brightBlue: '#3fc4de', brightMagenta: '#f075b5', brightCyan: '#6be4e6', brightWhite: '#d5d8da',
  },
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

function applyAppChrome(chromeColor) {
  if (!chromeColor) return;
  const preset = settings.activeTheme ? THEME_PRESETS[settings.activeTheme] : null;
  const root = document.documentElement.style;

  // Background tiers
  root.setProperty('--bg-primary', preset?.bg || chromeColor);
  root.setProperty('--bg-secondary', chromeColor);
  root.setProperty('--bg-tertiary', shiftColor(chromeColor, 16));

  // Text colors from theme
  if (preset) {
    root.setProperty('--text-primary', preset.fg);
    root.setProperty('--text-secondary', shiftColor(preset.fg, -40));
    root.setProperty('--border', shiftColor(chromeColor, 20));
    root.setProperty('--accent', preset.blue || '#3b82f6');
    root.setProperty('--accent-hover', shiftColor(preset.blue || '#3b82f6', -20));
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

    // Render theme presets in settings modal
    renderThemePresets();
  } catch (e) {
    console.error('Failed to load settings:', e);
  }
}

function saveSettings() {
  const shellType = document.getElementById('shell-type').value;
  settings = {
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
  // Enter key — reset input tracking and save to history
  if (data === '\r' || data === '\n') {
    if (autocompleteState.currentInput.trim()) {
      commandHistory.push({ command: autocompleteState.currentInput.trim(), timestamp: Date.now() });
      if (commandHistory.length > 1000) commandHistory = commandHistory.slice(-1000);
      localStorage.setItem('xnaut-history', JSON.stringify(commandHistory));
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

  resultsContainer.innerHTML = results.map(item => `
    <div class="history-item" onclick="executeHistoryCommand('${item.command.replace(/'/g, "\\'")}')">
      <div class="history-timestamp">${new Date(item.timestamp).toLocaleString()}</div>
      <div class="history-command">${escapeHtml(item.command)}</div>
    </div>
  `).join('');
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
  document.getElementById('chat-messages').innerHTML = '';
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
    const localProviders = ['antbot', 'ollama', 'lmstudio'];
    if (!apiKey && !localProviders.includes(provider)) {
      addChatMessage('assistant', 'Please set your API key in Settings first.');
      return;
    }

    const context = terminalOutputBuffer.slice(-2000); // Last 2000 chars
    const model = settings.llmModel || 'claude-sonnet-4-5-20250929';

    console.log('🤖 Sending AI request:', { provider, model, promptLength: message.length, contextLength: context.length });

    let response;
    if (provider === 'antbot') {
      response = await invoke('ask_antbot', { prompt: message, context: context });
    } else if (provider === 'ollama') {
      const url = settings.ollamaUrl || 'http://localhost:11434';
      const resp = await fetch(url + '/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: model,
          messages: [
            ...(context ? [{ role: 'system', content: 'Terminal context:\n' + context }] : []),
            { role: 'user', content: message }
          ],
          stream: false
        })
      });
      const data = await resp.json();
      response = data.message?.content || data.response || 'No response';
    } else if (provider === 'lmstudio') {
      const url = settings.lmstudioUrl || 'http://localhost:1234';
      const resp = await fetch(url + '/v1/chat/completions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: model,
          messages: [
            ...(context ? [{ role: 'system', content: 'Terminal context:\n' + context }] : []),
            { role: 'user', content: message }
          ]
        })
      });
      const data = await resp.json();
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
    const response = await invoke('ai_analyze_error', {
      errorText: context,
      context: 'User requested analysis of recent terminal output'
    });

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
async function loadSSHProfiles() {
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
  document.getElementById('ssh-private-key').value = '';
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
  document.getElementById('ssh-private-key').value = profile.privateKey || '';
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
  const privateKey = document.getElementById('ssh-private-key').value;

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
    privateKey: authMethod === 'privateKey' ? privateKey : undefined
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

function deleteSSHProfile(profileId) {
  if (!confirm('Delete this SSH profile?')) return;

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
      privateKey: profile.authMethod === 'privateKey' ? profile.privateKey : undefined
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

    tabs.push(tab);
    renderTabs();
    switchTab(tabId);

    // Listen for SSH output
    console.log('📡 Setting up SSH output listener for:', sessionId);
    await listen(`ssh-output-${sessionId}`, (event) => {
      const terminal = findTerminalBySSHSession(sessionId);
      if (terminal) {
        terminal.term.write(event.payload.data);
      }
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

function findTerminalBySSHSession(sessionId) {
  for (const tab of tabs) {
    if (tab.isSSH && tab.sshSessionId === sessionId && tab.terminals.length > 0) {
      return tab.terminals[0];
    }
  }
  return null;
}

async function testSSHConnection() {
  console.log('🧪 Testing SSH connection');

  // Get values from form
  const host = document.getElementById('ssh-host').value;
  const port = document.getElementById('ssh-port').value;
  const username = document.getElementById('ssh-username').value;
  const authMethod = document.getElementById('ssh-auth-method').value;
  const password = authMethod === 'password' ? document.getElementById('ssh-password').value : undefined;
  const privateKey = authMethod === 'privateKey' ? document.getElementById('ssh-private-key').value : undefined;

  if (!host || !username) {
    alert('Please enter host and username');
    return;
  }

  if (authMethod === 'password' && !password) {
    alert('Please enter password');
    return;
  }

  if (authMethod === 'privateKey' && !privateKey) {
    alert('Please enter private key');
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
      privateKey
    };

    console.log('📡 Testing SSH connection with:', { host, port, username });
    await invoke('create_ssh_session', { config });

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

function deleteWorkflow(workflowId) {
  if (!confirm('Delete this workflow?')) return;

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

    // Add to Rust backend
    try {
      await invoke('add_trigger', {
        pattern: trigger.pattern,
        patternType: trigger.type,
        action: 'notify',
        message: trigger.message
      });
    } catch (error) {
      console.error('Error adding trigger to backend:', error);
    }
  }

  saveTriggers();
  closeModal('trigger-edit-modal');
  renderTriggers();
}

function deleteTrigger(triggerId) {
  if (!confirm('Delete this trigger?')) return;

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

function checkTriggers(output) {
  for (const trigger of triggers) {
    if (!trigger.enabled) continue;

    let matched = false;

    if (trigger.type === 'keyword') {
      const keywords = trigger.pattern.split(',').map(k => k.trim().toLowerCase());
      matched = keywords.some(keyword => output.toLowerCase().includes(keyword));
    } else if (trigger.type === 'regex') {
      try {
        const regex = new RegExp(trigger.pattern, 'i');
        matched = regex.test(output);
      } catch (e) {
        console.error('Invalid regex pattern:', e);
      }
    }

    if (matched) {
      showNotification(trigger.name, trigger.message);
    }
  }
}

// Session Sharing
async function shareCurrentSession() {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab || tab.terminals.length === 0) {
    alert('No active terminal session to share');
    return;
  }

  const terminal = tab.terminals[0];

  try {
    const result = await invoke('create_shared_session', {
      sessionId: terminal.sessionId
    });

    const shareCode = result.share_code;
    document.getElementById('share-code').value = shareCode;
    showModal('share-modal');
  } catch (error) {
    console.error('Error creating shared session:', error);
    alert(`Failed to share session: ${error.message}`);
  }
}

function copyShareCode() {
  const input = document.getElementById('share-code');
  input.select();
  navigator.clipboard.writeText(input.value);

  const btn = document.getElementById('btn-copy-share-code');
  btn.textContent = 'Copied!';
  setTimeout(() => {
    btn.textContent = 'Copy to Clipboard';
  }, 2000);
}

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

// Error Monitor Functions
function toggleErrorPanel() {
  const panel = document.getElementById('error-panel');
  const isHidden = panel.style.display === 'none' || !panel.style.display;

  if (isHidden) {
    panel.style.display = 'flex';
    errorMonitorEnabled = true;
    renderErrors();
  } else {
    panel.style.display = 'none';
    errorMonitorEnabled = false;
  }

  requestAnimationFrame(() => {
    resizeAllTerminals();
  });
}

function parseTerminalOutput(output) {
  if (!errorMonitorEnabled) return;

  // Detect various error patterns
  const errorPatterns = [
    // JavaScript/TypeScript errors
    {
      pattern: /Error:([^\n]+)/gi,
      type: 'Error',
      severity: 'critical'
    },
    // Stack traces
    {
      pattern: /at\s+(.+?)\s+\((.+?):(\d+):(\d+)\)/gi,
      type: 'Stack Trace',
      severity: 'info'
    },
    // Next.js specific errors
    {
      pattern: /Dynamic server usage:([^\n]+)/gi,
      type: 'Next.js Dynamic Error',
      severity: 'warning'
    },
    // Build errors
    {
      pattern: /ERROR\s+in\s+([^\n]+)/gi,
      type: 'Build Error',
      severity: 'critical'
    },
    // Warnings
    {
      pattern: /WARNING:?([^\n]+)/gi,
      type: 'Warning',
      severity: 'warning'
    },
    // Deprecation notices
    {
      pattern: /deprecated|deprecation/gi,
      type: 'Deprecation',
      severity: 'info'
    },
    // Failed tests
    {
      pattern: /FAIL\s+([^\n]+)/gi,
      type: 'Test Failure',
      severity: 'critical'
    }
  ];

  let match;
  for (const errorPattern of errorPatterns) {
    const regex = new RegExp(errorPattern.pattern);
    while ((match = regex.exec(output)) !== null) {
      const error = {
        id: Date.now() + Math.random(),
        timestamp: new Date().toLocaleTimeString(),
        type: errorPattern.type,
        severity: errorPattern.severity,
        message: match[1] ? match[1].trim() : match[0].trim(),
        fullText: match[0],
        context: extractErrorContext(output, match.index),
        stack: extractStackTrace(output, match.index),
        location: extractLocation(match[0]),
        analyzed: false
      };

      // Avoid duplicates
      const isDuplicate = detectedErrors.some(e =>
        e.message === error.message &&
        Date.now() - new Date('1970-01-01 ' + e.timestamp).getTime() < 5000
      );

      if (!isDuplicate) {
        detectedErrors.unshift(error); // Add to beginning
        if (detectedErrors.length > 100) {
          detectedErrors = detectedErrors.slice(0, 100); // Keep only last 100
        }
        renderErrors();

        // Auto-analyze critical errors
        if (error.severity === 'critical' && settings.apiKeyAnthropic) {
          setTimeout(() => analyzeError(error.id), 1000);
        }
      }
    }
  }
}

function extractErrorContext(output, errorIndex) {
  const lines = output.split('\n');
  let charCount = 0;
  let errorLineIndex = 0;

  for (let i = 0; i < lines.length; i++) {
    charCount += lines[i].length + 1;
    if (charCount >= errorIndex) {
      errorLineIndex = i;
      break;
    }
  }

  const start = Math.max(0, errorLineIndex - 2);
  const end = Math.min(lines.length, errorLineIndex + 3);
  return lines.slice(start, end).join('\n');
}

function extractStackTrace(output, errorIndex) {
  const afterError = output.substring(errorIndex);
  const stackLines = [];
  const lines = afterError.split('\n');

  for (let i = 0; i < Math.min(10, lines.length); i++) {
    if (lines[i].trim().startsWith('at ') || lines[i].includes(':')) {
      stackLines.push(lines[i].trim());
    } else if (stackLines.length > 0) {
      break; // Stop after stack trace ends
    }
  }

  return stackLines.join('\n');
}

function extractLocation(errorText) {
  const locationMatch = errorText.match(/([\/\w.-]+):(\d+):(\d+)/);
  if (locationMatch) {
    return {
      file: locationMatch[1],
      line: locationMatch[2],
      column: locationMatch[3]
    };
  }
  return null;
}

function renderErrors() {
  const errorList = document.getElementById('error-list');
  if (!errorList) return;

  updateErrorStats();

  // Apply filter
  let filteredErrors = detectedErrors;
  if (errorSeverityFilter) {
    filteredErrors = detectedErrors.filter(e => e.severity === errorSeverityFilter);
  }

  if (filteredErrors.length === 0) {
    const message = errorSeverityFilter
      ? `No ${errorSeverityFilter} errors`
      : 'No errors detected';

    errorList.innerHTML = `
      <div class="error-empty">
        <div class="error-empty-icon">✅</div>
        <div class="error-empty-text">
          ${message}<br>
          <small>${errorSeverityFilter ? 'Click a stat to change filter' : 'Errors will appear here automatically'}</small>
        </div>
      </div>
    `;
    return;
  }

  errorList.innerHTML = filteredErrors.map(error => {
    const severityIcon = {
      critical: '🔴',
      warning: '🟡',
      info: '🔵'
    }[error.severity] || '⚪';

    let analysisHtml = '';
    if (error.analyzing) {
      analysisHtml = `
        <div class="error-ai-analysis">
          <div class="error-ai-analysis-loading">
            <span>🤖</span>
            <span>AI is analyzing this error...</span>
          </div>
        </div>
      `;
    } else if (error.analysis) {
      analysisHtml = `
        <div class="error-ai-analysis">
          <div class="error-ai-analysis-header">
            🤖 AI Analysis
          </div>
          <div class="error-ai-analysis-content">
            ${escapeHtml(error.analysis)}
          </div>
        </div>
      `;
    }

    return `
      <div class="error-item error-${error.severity}" data-error-id="${error.id}">
        <div class="error-item-header">
          <div class="error-item-title">
            <span class="error-severity">${severityIcon}</span>
            <span class="error-type">${escapeHtml(error.type)}</span>
          </div>
          <span class="error-timestamp">${error.timestamp}</span>
        </div>

        <div class="error-message">${escapeHtml(error.message)}</div>

        ${error.location ? `
          <div class="error-location">
            <span class="error-location-icon">📍</span>
            <span>${escapeHtml(error.location.file)}:${error.location.line}:${error.location.column}</span>
          </div>
        ` : ''}

        ${error.stack ? `
          <details>
            <summary style="cursor: pointer; color: var(--accent); font-size: 12px; margin-top: 8px;">📋 Stack Trace</summary>
            <div class="error-stack">${escapeHtml(error.stack)}</div>
          </details>
        ` : ''}

        ${analysisHtml}

        <div class="error-actions">
          ${!error.analysis && !error.analyzing ? `
            <button class="error-action-btn" data-action="analyze" data-error-id="${error.id}">
              🤖 Analyze with AI
            </button>
          ` : ''}
          <button class="error-action-btn" data-action="copy" data-error-id="${error.id}">
            📋 Copy
          </button>
          <button class="error-action-btn danger" data-action="dismiss" data-error-id="${error.id}">
            ✕ Dismiss
          </button>
        </div>
      </div>
    `;
  }).join('');

  // Add event listeners for error actions
  errorList.querySelectorAll('.error-action-btn').forEach(btn => {
    btn.onclick = (e) => {
      e.stopPropagation();
      const action = btn.dataset.action;
      const errorId = parseFloat(btn.dataset.errorId);
      handleErrorAction(action, errorId);
    };
  });
}

function updateErrorStats() {
  const criticalCount = detectedErrors.filter(e => e.severity === 'critical').length;
  const warningCount = detectedErrors.filter(e => e.severity === 'warning').length;
  const infoCount = detectedErrors.filter(e => e.severity === 'info').length;

  const criticalEl = document.getElementById('error-count-critical');
  const warningEl = document.getElementById('error-count-warning');
  const infoEl = document.getElementById('error-count-info');

  if (criticalEl) criticalEl.textContent = criticalCount;
  if (warningEl) warningEl.textContent = warningCount;
  if (infoEl) infoEl.textContent = infoCount;

  // Update active state for filter
  const criticalStat = document.querySelector('.error-stat-critical');
  const warningStat = document.querySelector('.error-stat-warning');
  const infoStat = document.querySelector('.error-stat-info');

  if (criticalStat) {
    criticalStat.classList.toggle('active', errorSeverityFilter === 'critical');
  }
  if (warningStat) {
    warningStat.classList.toggle('active', errorSeverityFilter === 'warning');
  }
  if (infoStat) {
    infoStat.classList.toggle('active', errorSeverityFilter === 'info');
  }
}

function toggleErrorFilter(severity) {
  // Toggle filter: if clicking the same one, clear filter; otherwise set new filter
  if (errorSeverityFilter === severity) {
    errorSeverityFilter = null;
    console.log('🔍 Showing all errors');
  } else {
    errorSeverityFilter = severity;
    console.log(`🔍 Filtering to ${severity} errors only`);
  }
  renderErrors();
}

async function analyzeError(errorId) {
  const error = detectedErrors.find(e => e.id === errorId);
  if (!error || error.analyzed || error.analyzing) return;

  error.analyzing = true;
  renderErrors();

  try {
    const apiKey = getAPIKey();
    if (!apiKey) {
      error.analysis = 'Please set your API key in Settings to enable AI analysis.';
      error.analyzing = false;
      error.analyzed = true;
      renderErrors();
      return;
    }

    const prompt = `Analyze this error and provide:
1. What the error means
2. Likely cause
3. How to fix it
4. Severity (is it critical or can it be ignored?)

Error Type: ${error.type}
Error Message: ${error.message}
${error.location ? `Location: ${error.location.file}:${error.location.line}` : ''}
${error.stack ? `\nStack Trace:\n${error.stack}` : ''}
${error.context ? `\nContext:\n${error.context}` : ''}

Keep the response concise (3-5 sentences).`;

    const response = await invoke('ask_ai', {
      prompt: prompt,
      context: '',
      provider: settings.llmProvider || 'anthropic',
      apiKey: apiKey,
      model: settings.llmModel || 'claude-sonnet-4-5-20250929'
    });

    error.analysis = response;
    error.analyzed = true;
    error.analyzing = false;
    renderErrors();

  } catch (err) {
    console.error('Error analyzing:', err);
    error.analysis = `Failed to analyze: ${err}`;
    error.analyzing = false;
    renderErrors();
  }
}

async function analyzeAllErrors() {
  const unananalyzedErrors = detectedErrors.filter(e => !e.analyzed && !e.analyzing);

  if (unananalyzedErrors.length === 0) {
    updateStatus('No errors to analyze');
    setTimeout(() => updateStatus('Ready'), 2000);
    return;
  }

  updateStatus(`Analyzing ${unananalyzedErrors.length} errors...`);

  for (const error of unananalyzedErrors) {
    await analyzeError(error.id);
    await new Promise(r => setTimeout(r, 1000)); // Rate limit
  }

  updateStatus('Analysis complete!');
  setTimeout(() => updateStatus('Ready'), 2000);
}

function handleErrorAction(action, errorId) {
  const error = detectedErrors.find(e => e.id === errorId);
  if (!error) return;

  switch (action) {
    case 'analyze':
      analyzeError(errorId);
      break;
    case 'copy':
      const errorText = `${error.type}: ${error.message}\n${error.stack || ''}`;
      navigator.clipboard.writeText(errorText);
      updateStatus('Error copied to clipboard');
      setTimeout(() => updateStatus('Ready'), 2000);
      break;
    case 'dismiss':
      detectedErrors = detectedErrors.filter(e => e.id !== errorId);
      renderErrors();
      break;
  }
}

function clearErrors() {
  detectedErrors = [];
  renderErrors();
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
  const container = document.getElementById('snippet-categories');
  if (!container) return;

  // "All" pill
  let html = `<div class="category-pill category-pill-all ${!activeSnippetCategory ? 'active' : ''}" data-category="">All</div>`;

  // Category pills
  snippetCategories.forEach(cat => {
    const count = commandSnippets.filter(s => s.category === cat).length;
    if (count > 0 || activeSnippetCategory === cat) {
      html += `<div class="category-pill ${activeSnippetCategory === cat ? 'active' : ''}" data-category="${escapeHtml(cat)}">${escapeHtml(cat)} (${count})</div>`;
    }
  });

  container.innerHTML = html;

  // Add click listeners
  container.querySelectorAll('.category-pill').forEach(pill => {
    pill.addEventListener('click', () => {
      const category = pill.dataset.category;
      activeSnippetCategory = category || null;
      renderCategoryPills();
      renderSnippets();
    });
  });
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

function deleteSnippet() {
  if (!editingSnippetId) return;

  if (confirm('Delete this snippet?')) {
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
  let filteredSnippets = commandSnippets;
  if (activeSnippetCategory) {
    filteredSnippets = commandSnippets.filter(s => s.category === activeSnippetCategory);
  }

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
        </div>
      </div>
    `).join('');

    return `
      <div class="snippet-card" data-snippet-id="${snippet.id}">
        <div class="snippet-card-header">
          <div style="display:flex; align-items:center; gap:8px; flex:1; min-width:0;">
            <span style="font-size:13px; font-weight:500; color:var(--text-primary); overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">${escapeHtml(snippet.name)}</span>
            ${snippet.category ? '<span style="font-size:10px; padding:1px 6px; border-radius:3px; background:var(--accent); color:white; opacity:0.8;">' + escapeHtml(snippet.category) + '</span>' : ''}
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
        ${snippet.description ? '<div style="font-size:11px; color:var(--text-secondary); padding:0 12px 4px; margin-top:-2px;">' + escapeHtml(snippet.description) + '</div>' : ''}
        <div class="snippet-commands">${commandRows}</div>
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
    btn.onclick = () => {
      const cmd = btn.closest('.snippet-cmd').dataset.cmd;
      insertPathToTerminal(cmd + '\n');
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
  'closePane':       { code: 'KeyW', ctrl: false, shift: false, alt: true, meta: false, label: 'Close Pane' },
  'paneLeft':        { code: 'ArrowLeft', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Left' },
  'paneRight':       { code: 'ArrowRight', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Right' },
  'paneUp':          { code: 'ArrowUp', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Up' },
  'paneDown':        { code: 'ArrowDown', ctrl: false, shift: false, alt: true, meta: false, label: 'Focus Pane Down' },
  'toggleRalph':     { code: 'KeyR', ctrl: true, shift: true, alt: false, meta: false, label: 'Toggle Ralph Panel' },
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
  _on('btn-new-tab', 'onclick', createNewTab);
  _on('btn-toggle-files-top', 'onclick', toggleFilesPanel);

  // 3-dot menu
  _on('btn-more-menu', 'onclick', () => {
    const dd = document.getElementById('more-menu-dropdown');
    if (dd) dd.style.display = dd.style.display === 'none' ? 'block' : 'none';
  });
  document.querySelectorAll('.more-menu-item').forEach(item => {
    item.onmouseenter = () => { item.style.background = 'rgba(255,255,255,0.05)'; };
    item.onmouseleave = () => { item.style.background = 'none'; };
    item.onclick = () => {
      document.getElementById('more-menu-dropdown').style.display = 'none';
      const action = item.dataset.action;
      if (action === 'errors') toggleErrorPanel();
      else if (action === 'snippets') toggleSnippetsPanel();
      else if (action === 'ralph') toggleRalphPanel();
      else if (action === 'ssh') { showModal('ssh-modal'); loadSSHProfiles(); }
      else if (action === 'settings') toggleSettingsPanel();
    };
  });
  // Close 3-dot menu on click outside
  document.addEventListener('mousedown', (e) => {
    const dd = document.getElementById('more-menu-dropdown');
    if (dd && dd.style.display !== 'none' && !e.target.closest('#btn-more-menu') && !e.target.closest('#more-menu-dropdown')) {
      dd.style.display = 'none';
    }
  });

  // Use event delegation for status bar buttons (they're created dynamically per terminal)

  // Simpler approach: use mouseenter for submenus
  document.addEventListener('mouseenter', (e) => {
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
    else if (target.id === 'btn-toggle-files') toggleFilesPanel();
    else if (target.id === 'btn-toggle-errors') toggleErrorPanel();
    else if (target.id === 'btn-toggle-snippets') toggleSnippetsPanel();
    else if (target.id === 'btn-toggle-ralph') toggleRalphPanel();
    else if (target.id === 'btn-ssh') {
      console.log('🔐 SSH button clicked in event delegation');
      showSSHModal();
    }
    else if (target.id === 'btn-triggers') showTriggersModal();
    else if (target.id === 'btn-share-session') {
      console.log('🔗 Share session button clicked');
      shareCurrentSession();
    }
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

  // File Navigator
  const btnFilesHome = document.getElementById('btn-files-home');
  if (btnFilesHome) btnFilesHome.onclick = loadHomeDirectory;
  const btnCloseFiles = document.getElementById('btn-close-files');
  if (btnCloseFiles) btnCloseFiles.onclick = () => { document.getElementById('files-panel').style.display = 'none'; requestAnimationFrame(() => resizeAllTerminals()); };
  const btnCollapseFiles = document.getElementById('btn-collapse-files');
  if (btnCollapseFiles) btnCollapseFiles.onclick = toggleFilesPanel;

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

  // Error Monitor
  _on('btn-clear-errors', 'onclick', clearErrors);
  _on('btn-analyze-errors', 'onclick', analyzeAllErrors);
  _on('btn-collapse-errors', 'onclick', toggleErrorPanel);

  // Error stat filters - click to filter by severity
  document.addEventListener('click', (e) => {
    const statBox = e.target.closest('.error-stat');
    if (statBox) {
      if (statBox.classList.contains('error-stat-critical')) {
        toggleErrorFilter('critical');
      } else if (statBox.classList.contains('error-stat-warning')) {
        toggleErrorFilter('warning');
      } else if (statBox.classList.contains('error-stat-info')) {
        toggleErrorFilter('info');
      }
    }
  });

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
  _on('btn-close-share', 'onclick', () => closeModal('share-modal'));
  _on('btn-copy-share-code', 'onclick', copyShareCode);

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
    closePane: () => closePane(),
    paneLeft: () => navigatePane('ArrowLeft'),
    paneRight: () => navigatePane('ArrowRight'),
    paneUp: () => navigatePane('ArrowUp'),
    paneDown: () => navigatePane('ArrowDown'),
    toggleRalph: () => toggleRalphPanel(),
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

// ==================== Ralph Panel ====================

function toggleRalphPanel() {
  // Delegate to the module-loaded ralphUI controller
  if (window.ralphUI) {
    window.ralphUI.toggle();
  } else {
    // Module not yet loaded — toggle display manually and let it init on load
    const panel = document.getElementById('ralph-panel');
    if (!panel) return;
    const isHidden = panel.style.display === 'none' || !panel.style.display;
    if (isHidden) {
      panel.style.display = 'flex';
      const targetWidth = Math.max(300, Math.min(600, Math.round(window.innerWidth / 3.5)));
      panel.style.width = targetWidth + 'px';
    } else {
      panel.style.display = 'none';
    }
    requestAnimationFrame(() => { resizeAllTerminals(); });
  }
}

// ==================== File Navigator ====================

let currentDirectory = '';

function toggleFilesPanel() {
  const panel = document.getElementById('files-panel');
  const isHidden = panel.style.display === 'none' || !panel.style.display;

  if (isHidden) {
    panel.style.display = 'flex';
    if (!currentDirectory) loadHomeDirectory();
    requestAnimationFrame(() => resizeAllTerminals());
  } else {
    panel.style.display = 'none';
    requestAnimationFrame(() => resizeAllTerminals());
  }

  // Close button
  const closeBtn = document.getElementById('btn-close-files');
  if (closeBtn) closeBtn.onclick = () => { panel.style.display = 'none'; requestAnimationFrame(() => resizeAllTerminals()); };
}

async function loadHomeDirectory() {
  try {
    const homeDir = await invoke('get_home_directory');
    await loadDirectory(homeDir);
  } catch (error) {
    console.error('Failed to load home directory:', error);
  }
}

async function loadDirectory(path) {
  try {
    console.log('Loading directory:', path);
    const listing = await invoke('list_directory', { path });
    currentDirectory = listing.path;
    renderDirectory(listing);
  } catch (error) {
    console.error('Failed to load directory:', error);
    alert(`Failed to load directory: ${error}`);
  }
}

function renderDirectory(listing) {
  const treeEl = document.getElementById('files-tree');
  const cwdEl = document.getElementById('files-cwd');
  if (!treeEl) return;

  if (cwdEl) cwdEl.textContent = listing.path;

  treeEl.innerHTML = '';

  // Event delegation for file clicks (handles all tree items)
  treeEl.addEventListener('click', (e) => {
    const treeItem = e.target.closest('.tree-item');
    if (!treeItem || !treeItem.dataset.filePath) return;
    if (treeItem.dataset.isDir === '0') {
      openFileInEditor(treeItem.dataset.filePath);
    }
  });
  treeEl.ondblclick = (e) => {
    const treeItem = e.target.closest('.tree-item');
    if (!treeItem || !treeItem.dataset.filePath) return;
    insertPathToTerminal(treeItem.dataset.filePath);
  };
  treeEl.addEventListener('contextmenu', (e) => {
    const treeItem = e.target.closest('.tree-item');
    if (!treeItem || !treeItem.dataset.filePath) return;
    e.preventDefault();
    showFileContextMenu(e.clientX, e.clientY, {
      path: treeItem.dataset.filePath,
      is_directory: treeItem.dataset.isDir === '1',
      name: treeItem.querySelector('.tree-name')?.textContent || ''
    });
  });

  // Root folder
  const rootName = listing.path.split('/').pop() || '/';
  const rootItem = document.createElement('div');
  rootItem.className = 'tree-item selected';
  rootItem.style.paddingLeft = '8px';
  rootItem.innerHTML = '<span class="tree-arrow expanded">›</span><span class="tree-icon">📂</span><span class="tree-name">' + rootName + '</span>';
  treeEl.appendChild(rootItem);

  // Children container
  const childrenEl = document.createElement('div');
  childrenEl.className = 'tree-children open';
  treeEl.appendChild(childrenEl);

  // Sort: directories first, then files
  const dirs = listing.entries.filter(e => e.is_directory).sort((a, b) => a.name.localeCompare(b.name));
  const files = listing.entries.filter(e => !e.is_directory).sort((a, b) => a.name.localeCompare(b.name));

  [...dirs, ...files].forEach(entry => {
    const item = createTreeItem(entry, 1);
    childrenEl.appendChild(item);
  });

  // Search filter
  const searchInput = document.getElementById('files-search');
  if (searchInput) {
    searchInput.value = '';
    searchInput.oninput = () => {
      const q = searchInput.value.toLowerCase();
      childrenEl.querySelectorAll('.tree-item').forEach(el => {
        const name = el.querySelector('.tree-name')?.textContent.toLowerCase() || '';
        el.style.display = !q || name.includes(q) ? 'flex' : 'none';
      });
    };
  }
}

function createTreeItem(entry, depth) {
  const wrapper = document.createElement('div');

  const item = document.createElement('div');
  item.className = 'tree-item';
  item.dataset.filePath = entry.path;
  item.dataset.isDir = entry.is_directory ? '1' : '0';
  item.style.paddingLeft = (8 + depth * 16) + 'px';
  item.draggable = false;

  const arrow = document.createElement('span');
  arrow.className = 'tree-arrow';
  arrow.textContent = entry.is_directory ? '›' : ' ';
  arrow.style.visibility = entry.is_directory ? 'visible' : 'hidden';

  const icon = document.createElement('span');
  icon.className = 'tree-icon';
  icon.textContent = entry.is_directory ? '📁' : getFileIcon(entry.name);

  const name = document.createElement('span');
  name.className = 'tree-name';
  name.textContent = entry.name;

  item.appendChild(arrow);
  item.appendChild(icon);
  item.appendChild(name);
  wrapper.appendChild(item);

  if (entry.is_directory) {
    const children = document.createElement('div');
    children.className = 'tree-children';
    wrapper.appendChild(children);

    // Double-click inserts folder path into terminal
    item.ondblclick = () => insertPathToTerminal(entry.path);

    // Single click expands/collapses
    item.onclick = async () => {
      const isOpen = children.classList.contains('open');
      if (isOpen) {
        children.classList.remove('open');
        arrow.classList.remove('expanded');
        icon.textContent = '📁';
      } else {
        arrow.classList.add('expanded');
        icon.textContent = '📂';
        if (children.childElementCount === 0) {
          try {
            const listing = await invoke('list_directory', { path: entry.path });
            const dirs = listing.entries.filter(e => e.is_directory).sort((a, b) => a.name.localeCompare(b.name));
            const files = listing.entries.filter(e => !e.is_directory).sort((a, b) => a.name.localeCompare(b.name));
            [...dirs, ...files].forEach(e => {
              children.appendChild(createTreeItem(e, depth + 1));
            });
          } catch (e) {
            const err = document.createElement('div');
            err.className = 'tree-item';
            err.style.paddingLeft = (8 + (depth + 1) * 16) + 'px';
            err.style.color = 'var(--text-secondary)';
            err.style.fontStyle = 'italic';
            err.textContent = 'Permission denied';
            children.appendChild(err);
          }
        }
        children.classList.add('open');
      }
    };
  } else {
    // File click — open in editor
    const filePath = entry.path;
    item.onclick = function() {
      // Ensure editor panel is visible first
      const panel = document.getElementById('editor-panel');
      if (panel) panel.style.display = 'flex';
      openFileInEditor(filePath);
    };
  }

  // Context menu handled by event delegation on files-tree container

  return wrapper;
}

function getFileIcon(name) {
  const ext = name.split('.').pop()?.toLowerCase();
  const icons = {
    js: '📜', ts: '📜', py: '🐍', rs: '🦀', go: '🔷', rb: '💎',
    md: '📝', txt: '📄', json: '📋', yaml: '📋', yml: '📋', toml: '📋',
    sh: '⚡', bash: '⚡', zsh: '⚡',
    html: '🌐', css: '🎨', svg: '🎨',
    png: '🖼️', jpg: '🖼️', jpeg: '🖼️', gif: '🖼️', webp: '🖼️',
    pdf: '📕', zip: '📦', tar: '📦', gz: '📦',
    log: '📊', env: '🔒', lock: '🔒',
  };
  if (name.startsWith('.')) return '⚙️';
  return icons[ext] || '📄';
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

window.closeEditor = function() {
  if (editorState.modified) {
    if (!confirm('You have unsaved changes. Close anyway?')) return;
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

// ==================== File Context Menu ====================
function showFileContextMenu(x, y, entry) {
  // Remove existing menu
  const existing = document.getElementById('file-context-menu');
  if (existing) existing.remove();

  const menu = document.createElement('div');
  menu.id = 'file-context-menu';
  menu.style.cssText = 'position:fixed; z-index:9999; background:var(--bg-secondary); border:1px solid var(--border); border-radius:6px; padding:4px 0; min-width:180px; box-shadow:0 4px 12px rgba(0,0,0,0.3); font-size:13px;';
  menu.style.left = x + 'px';
  menu.style.top = y + 'px';

  const items = [
    { label: 'Send to Terminal', action: () => insertPathToTerminal(entry.path) },
    { label: 'Open in Editor', action: () => openFileInEditor(entry.path) },
    { label: 'Copy Path', action: () => navigator.clipboard.writeText(entry.path) },
  ];

  if (entry.is_directory) {
    items.unshift({ label: 'cd into folder', action: () => insertPathToTerminal('cd ' + entry.path + '\n') });
  }

  items.forEach(item => {
    const el = document.createElement('div');
    el.style.cssText = 'padding:6px 16px; cursor:pointer; color:var(--text-primary);';
    el.textContent = item.label;
    el.onmouseenter = () => { el.style.background = 'rgba(255,255,255,0.05)'; };
    el.onmouseleave = () => { el.style.background = 'none'; };
    el.addEventListener('mouseup', (e) => {
      e.stopPropagation();
      e.preventDefault();
      menu.remove();
      item.action();
    });
    menu.appendChild(el);
  });

  document.body.appendChild(menu);

  // Close on click outside (use mousedown so it doesn't race with menu item mouseup)
  const closeMenu = (e) => {
    if (!menu.contains(e.target)) {
      menu.remove();
      document.removeEventListener('mousedown', closeMenu, true);
    }
  };
  setTimeout(() => document.addEventListener('mousedown', closeMenu, true), 50);
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
  workflows,
  sshProfiles,
  triggers,
  getActiveTerminal: () => {
    const tab = tabs.find(t => t.id === activeTabId);
    return tab && tab.terminals.length > 0 ? tab.terminals[0] : null;
  }
};

console.log('🎯 XNAUT loaded. Access debug info via window.xnaut');
