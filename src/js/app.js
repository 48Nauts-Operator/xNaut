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

// State machine: given current layout and split direction, what's the next layout?
function getNextLayout(currentLayout, direction) {
  const paneCount = LAYOUT_TEMPLATES[currentLayout].panes.length;

  if (paneCount >= 9) return null; // Max 9 panes

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

  if (paneCount === 8) {
    return 'grid-9';
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
async function splitPane(direction) {
  const tab = tabs.find(t => t.id === activeTabId);
  if (!tab) return;

  const nextLayout = getNextLayout(tab.layoutType, direction);
  if (!nextLayout) {
    console.log('Cannot split further — max 9 panes per tab');
    return;
  }

  const template = LAYOUT_TEMPLATES[nextLayout];
  const newPaneId = template.panes[tab.terminals.length]; // Next available pane letter

  tab.layoutType = nextLayout;
  tab.colSizes = null; // Reset custom sizes on layout change
  tab.rowSizes = null;

  // Create a new terminal in this tab with the new pane ID
  await createTerminal(tab.id, newPaneId);

  applyLayout(tab);

  console.log(`Split ${direction}: ${tab.layoutType} (${tab.terminals.length} panes)`);
}

// Close the currently focused pane
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
    if (statusPath) {
      // Format path nicely - replace home with ~
      const displayPath = currentDir.replace(homeDir, '~');
      statusPath.textContent = displayPath;
    }

    // Get git info for current directory
    try {
      const gitInfo = await invoke('get_git_info', { path: currentDir });

      const statusGit = document.getElementById(`status-git-${sessionId}`);
      if (statusGit && gitInfo && gitInfo.is_repo) {
        statusGit.innerHTML = `
          <span class="git-branch">⎇ ${gitInfo.branch}</span>
          ${gitInfo.changes > 0 ? `<span class="git-stats">• ${gitInfo.changes} ${gitInfo.changes === 1 ? 'change' : 'changes'}</span>` : ''}
        `;
      } else if (statusGit) {
        statusGit.textContent = '';
      }
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
function initSharedStatusBar() {
  const bar = document.getElementById('shared-status-bar');
  if (!bar) return;
  bar.innerHTML = `
    <div class="status-bar-left">
      <span class="status-icon">📁</span>
      <span class="status-path" id="shared-status-path">~</span>
      <span class="status-git" id="shared-status-git"></span>
    </div>
    <div class="status-bar-right">
      <div class="llm-dropdown-wrapper">
        <button id="llm-dropdown-btn" class="btn btn-sm llm-dropdown-btn">
          <span id="llm-current-selection">🔮 Claude Sonnet 4.5</span>
          <span class="dropdown-arrow">▼</span>
        </button>
        <div id="llm-dropdown-menu" class="llm-dropdown-menu" style="display: none;">
          <div class="llm-provider-item" data-provider="anthropic">
            <span>🔮 Anthropic</span>
            <span class="submenu-arrow">▶</span>
            <div class="llm-model-submenu">
              <div class="llm-model-option" data-provider="anthropic" data-model="claude-sonnet-4-5-20250929">Claude Sonnet 4.5 (Sep 2025)</div>
              <div class="llm-model-option" data-provider="anthropic" data-model="claude-opus-4-1-20250805">Claude Opus 4.1 (Aug 2025)</div>
              <div class="llm-model-option" data-provider="anthropic" data-model="claude-sonnet-4-20250514">Claude Sonnet 4 (May 2025)</div>
              <div class="llm-model-option" data-provider="anthropic" data-model="claude-opus-4-20250514">Claude Opus 4 (May 2025)</div>
              <div class="llm-model-option" data-provider="anthropic" data-model="claude-3-7-sonnet-20250219">Claude Sonnet 3.7 (Feb 2025)</div>
              <div class="llm-model-option" data-provider="anthropic" data-model="claude-3-5-haiku-20241022">Claude Haiku 3.5 (Oct 2024)</div>
              <div class="llm-model-option" data-provider="anthropic" data-model="claude-3-haiku-20240307">Claude Haiku 3 (Mar 2024)</div>
            </div>
          </div>
          <div class="llm-provider-item" data-provider="openai">
            <span>🟢 OpenAI</span>
            <span class="submenu-arrow">▶</span>
            <div class="llm-model-submenu">
              <div class="llm-model-option" data-provider="openai" data-model="gpt-4o">GPT-4o</div>
              <div class="llm-model-option" data-provider="openai" data-model="gpt-4o-mini">GPT-4o Mini</div>
              <div class="llm-model-option" data-provider="openai" data-model="gpt-4-turbo">GPT-4 Turbo</div>
              <div class="llm-model-option" data-provider="openai" data-model="gpt-4">GPT-4</div>
              <div class="llm-model-option" data-provider="openai" data-model="gpt-3.5-turbo">GPT-3.5 Turbo</div>
            </div>
          </div>
          <div class="llm-provider-item" data-provider="openrouter">
            <span>🤖 OpenRouter</span>
            <span class="submenu-arrow">▶</span>
            <div class="llm-model-submenu">
              <div class="llm-model-option" data-provider="openrouter" data-model="anthropic/claude-3.5-sonnet">Claude 3.5 Sonnet</div>
              <div class="llm-model-option" data-provider="openrouter" data-model="anthropic/claude-3-opus">Claude 3 Opus</div>
              <div class="llm-model-option" data-provider="openrouter" data-model="openai/gpt-4o">GPT-4o</div>
              <div class="llm-model-option" data-provider="openrouter" data-model="openai/gpt-4-turbo">GPT-4 Turbo</div>
              <div class="llm-model-option" data-provider="openrouter" data-model="meta-llama/llama-3.1-70b-instruct">Llama 3.1 70B</div>
            </div>
          </div>
          <div class="llm-provider-item" data-provider="perplexity">
            <span>🔍 Perplexity</span>
            <span class="submenu-arrow">▶</span>
            <div class="llm-model-submenu">
              <div class="llm-model-option" data-provider="perplexity" data-model="sonar">Sonar</div>
              <div class="llm-model-option" data-provider="perplexity" data-model="sonar-pro">Sonar Pro</div>
              <div class="llm-model-option" data-provider="perplexity" data-model="sonar-reasoning">Sonar Reasoning</div>
              <div class="llm-model-option" data-provider="perplexity" data-model="sonar-reasoning-pro">Sonar Reasoning Pro</div>
              <div class="llm-model-option" data-provider="perplexity" data-model="sonar-deep-research">Sonar Deep Research</div>
            </div>
          </div>
        </div>
      </div>
      <button id="btn-toggle-chat" class="btn btn-icon btn-sm" title="Toggle AI Chat">💬</button>
      <button id="btn-toggle-files" class="btn btn-icon btn-sm" title="Toggle File Navigator">📁</button>
      <button id="btn-toggle-errors" class="btn btn-icon btn-sm" title="Toggle Error Monitor">🚨</button>
      <button id="btn-toggle-snippets" class="btn btn-icon btn-sm" title="Toggle Command Snippets">📝</button>
      <button id="btn-toggle-ralph" class="btn btn-icon btn-sm" title="Toggle Ralph Orchestrator (Ctrl+Shift+R)">🤖</button>
      <button id="btn-ssh" class="btn btn-icon btn-sm" title="SSH Remote Connections">🔐</button>
      <button id="btn-triggers" class="btn btn-icon btn-sm" title="Triggers & Notifications">🔔</button>
      <button id="btn-share-session" class="btn btn-icon btn-sm" title="Share Current Session">🔗</button>
      <button id="btn-settings" class="btn btn-icon btn-sm" title="Settings (API Keys & Preferences)">⚙️</button>
      <button id="btn-debug" class="btn btn-icon btn-sm" title="Debug Info" style="background: #ff6b6b;">🐛</button>
    </div>
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
async function createTerminal(tabId, paneId) {
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

  // Minimal pane header (path + git info + close button)
  const paneHeader = document.createElement('div');
  paneHeader.className = 'pane-header';
  paneHeader.style.cssText = 'display:flex; justify-content:space-between; align-items:center; padding:2px 8px; font-size:12px; color:#a9afbc; background:rgba(0,0,0,0.15); border-bottom:1px solid rgba(255,255,255,0.05);';
  paneHeader.innerHTML = `
    <div style="display:flex; align-items:center; gap:6px; overflow:hidden;">
      <span style="opacity:0.6;">📁</span>
      <span class="status-path" id="status-path-${sessionId}" style="white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">~</span>
      <span class="status-git" id="status-git-${sessionId}" style="color:#51cf66;"></span>
    </div>
    <button class="pane-close-btn" title="Close pane" style="background:none; border:none; color:#6c757d; cursor:pointer; font-size:14px; padding:0 4px; line-height:1;" data-session="${sessionId}">×</button>
  `;
  pane.appendChild(paneHeader);

  // Close button handler
  paneHeader.querySelector('.pane-close-btn').addEventListener('click', (e) => {
    e.stopPropagation();
    closePane();
  });

  // Terminal area (interactive)
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

  // Add Unicode11 addon for proper box-drawing and wide character support
  if (window.Unicode11Addon) {
    const unicode11Addon = new Unicode11Addon.Unicode11Addon();
    term.loadAddon(unicode11Addon);
    term.unicode.activeVersion = '11';
  }

  // Add WebGL renderer for better glyph rendering and performance
  if (window.WebglAddon) {
    try {
      const webglAddon = new WebglAddon.WebglAddon();
      webglAddon.onContextLoss(() => { webglAddon.dispose(); });
      term.loadAddon(webglAddon);
    } catch (e) { /* canvas fallback */ }
  }

  // Fit terminal to container (with slight delay to ensure layout is ready)
  setTimeout(() => {
    fitAddon.fit();
  }, 10);

  // Add drag and drop support for files
  terminalDiv.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  });

  terminalDiv.addEventListener('drop', async (e) => {
    e.preventDefault();
    const path = e.dataTransfer.getData('text/plain');
    if (path) {
      // Use insertPathToTerminal which will handle both display and PTY write
      await insertPathToTerminal(path);
    }
  });

  // Cool ASCII art banner
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
  term.writeln('\x1b[1;32m💬 Chat with AI\x1b[0m  │  \x1b[1;34m🔐 SSH Sessions\x1b[0m  │  \x1b[1;96m📓 Workflows\x1b[0m');
  term.writeln('');
  term.writeln('\x1b[1;36mInitializing terminal session...\x1b[0m\r\n');

  try {
    console.log('🔄 Attempting to create terminal session...');
    term.writeln('\x1b[1;33m🔄 Connecting to backend...\x1b[0m\r\n');

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
    term.writeln(`\x1b[1;32m✅ Connected! Session: ${backendSessionId}\x1b[0m\r\n`);

    // Inject shell integration for directory tracking and enable colors
    const shellIntegration = `
export TERM=xterm-256color
export CLICOLOR=1
export LSCOLORS=ExGxFxdaCxDaDahbadacec
if [ -n "$ZSH_VERSION" ]; then
  autoload -U colors && colors
  setopt prompt_subst
  precmd() {
    printf "\\033]7;file://%s%s\\007" "$HOSTNAME" "$PWD"
  }
elif [ -n "$BASH_VERSION" ]; then
  export PS1='\\[\\033[01;32m\\]\\u@\\h\\[\\033[00m\\]:\\[\\033[01;34m\\]\\w\\[\\033[00m\\]\\$ '
  PROMPT_COMMAND='printf "\\033]7;file://%s%s\\007" "$HOSTNAME" "$PWD"'
fi
`;

    // Send shell integration code followed by clear to hide the setup
    await invoke('write_to_terminal', {
      sessionId: backendSessionId,
      data: shellIntegration + '\nclear\n'
    });

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

  // Minimal pane header for SSH
  const paneHeader = document.createElement('div');
  paneHeader.className = 'pane-header';
  paneHeader.style.cssText = 'display:flex; justify-content:space-between; align-items:center; padding:2px 8px; font-size:12px; color:#a9afbc; background:rgba(0,0,0,0.15); border-bottom:1px solid rgba(255,255,255,0.05);';
  paneHeader.innerHTML = `
    <div style="display:flex; align-items:center; gap:6px;">
      <span style="opacity:0.6;">🔐</span>
      <span class="status-path" id="status-path-${sessionId}">SSH Connection</span>
    </div>
    <button class="pane-close-btn" title="Close pane" style="background:none; border:none; color:#6c757d; cursor:pointer; font-size:14px; padding:0 4px; line-height:1;">×</button>
  `;
  pane.appendChild(paneHeader);
  paneHeader.querySelector('.pane-close-btn').addEventListener('click', (e) => {
    e.stopPropagation();
    closePane();
  });

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
function applyAppChrome(chromeColor) {
  if (!chromeColor) return;
  document.documentElement.style.setProperty('--bg-secondary', chromeColor);
  // Derive slightly lighter tertiary from chrome color
  const r = parseInt(chromeColor.slice(1, 3), 16);
  const g = parseInt(chromeColor.slice(3, 5), 16);
  const b = parseInt(chromeColor.slice(5, 7), 16);
  const lighter = '#' + [r, g, b].map(c => Math.min(255, c + 16).toString(16).padStart(2, '0')).join('');
  document.documentElement.style.setProperty('--bg-tertiary', lighter);
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
    if (!apiKey) {
      addChatMessage('assistant', 'Please set your API key in Settings first.');
      return;
    }

    const context = terminalOutputBuffer.slice(-2000); // Last 2000 chars
    const provider = settings.llmProvider || 'anthropic';
    const model = settings.llmModel || 'claude-sonnet-4-5-20250929';

    console.log('🤖 Sending AI request:', { provider, model, promptLength: message.length, contextLength: context.length });

    const response = await invoke('ask_ai', {
      prompt: message,
      context: context,
      provider: provider,
      apiKey: apiKey,
      model: model
    });

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

  container.innerHTML = filteredSnippets.map(snippet => {
    // Render markdown with code blocks
    let renderedContent = '';
    if (typeof marked !== 'undefined') {
      const renderer = new marked.Renderer();

      renderer.code = function(code, language) {
        const validLanguage = language && hljs.getLanguage(language) ? language : 'bash';
        const highlighted = hljs.highlight(code, { language: validLanguage }).value;

        return `
          <div class="code-block-wrapper">
            <div class="code-block-header">
              <span class="code-language">${validLanguage}</span>
              <div class="code-actions">
                <button class="code-btn copy-code" data-code="${escapeHtml(code)}">📋 Copy</button>
                <button class="code-btn run-code" data-code="${escapeHtml(code)}">▶️ Run</button>
                <button class="code-btn analyze-code" data-code="${escapeHtml(code)}">🤖 Improve</button>
              </div>
            </div>
            <pre><code class="hljs language-${validLanguage}">${highlighted}</code></pre>
          </div>
        `;
      };

      marked.setOptions({ renderer });
      renderedContent = marked.parse(snippet.content);
    } else {
      renderedContent = `<pre>${escapeHtml(snippet.content)}</pre>`;
    }

    return `
      <div class="snippet-item" data-snippet-id="${snippet.id}">
        <div class="snippet-header">
          <div class="snippet-title">
            ${snippet.category ? `<span class="snippet-category-badge">${escapeHtml(snippet.category)}</span>` : ''}
            ${escapeHtml(snippet.name)}
          </div>
          <button class="snippet-edit-btn" data-snippet-id="${snippet.id}">✏️</button>
        </div>
        ${snippet.description ? `<div class="snippet-description">${escapeHtml(snippet.description)}</div>` : ''}
        <div class="snippet-content">${renderedContent}</div>
      </div>
    `;
  }).join('');

  // Attach event listeners
  attachSnippetCodeBlockListeners(container);

  // Edit button listeners
  container.querySelectorAll('.snippet-edit-btn').forEach(btn => {
    btn.onclick = () => editSnippet(btn.dataset.snippetId);
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
    const prompt = `Analyze this shell command and provide:
1. Brief explanation of what it does
2. Potential issues or improvements
3. A simplified or better version if possible

Command:
\`\`\`bash
${command}
\`\`\`

Keep response concise (3-4 sentences max).`;

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
    addChatMessage('assistant', `**Command Analysis:**\n\nOriginal command:\n\`\`\`bash\n${command}\n\`\`\`\n\n${response}`);

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
  'splitVertical':   { code: 'KeyD', ctrl: false, shift: false, alt: true, meta: false, label: 'Split Vertical' },
  'splitHorizontal': { code: 'KeyD', ctrl: false, shift: true, alt: true, meta: false, label: 'Split Horizontal' },
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
function setupEventListeners() {
  // Top bar buttons (only btn-new-tab is in top bar now)
  document.getElementById('btn-new-tab').onclick = createNewTab;

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
      loadSettings();
      renderKeybindingsUI();
      initKeybindingSearch();
      showModal('settings-modal');
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

  // Settings
  document.getElementById('btn-close-settings').onclick = () => closeModal('settings-modal');
  document.getElementById('btn-save-settings').onclick = saveSettings;
  document.getElementById('btn-reset-appearance').onclick = resetAppearanceToDefaults;
  document.getElementById('btn-reset-keybindings').onclick = () => {
    keybindings = { ...DEFAULT_KEYBINDINGS };
    saveKeybindings();
    renderKeybindingsUI();
  };

  // Chat
  document.getElementById('btn-send-chat').onclick = sendChatMessage;
  document.getElementById('btn-clear-chat').onclick = clearChat;
  document.getElementById('btn-analyze-output').onclick = analyzeTerminalOutput;
  document.getElementById('btn-collapse-chat').onclick = toggleChatPanel;
  document.getElementById('btn-new-chat-session').onclick = () => {
    const session = createNewChatSession('New Chat');
    console.log('✅ Created new chat session:', session.id);
  };
  document.getElementById('chat-input').onkeydown = (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendChatMessage();
    }
  };

  // File Navigator
  document.getElementById('btn-files-home').onclick = loadHomeDirectory;
  document.getElementById('btn-collapse-files').onclick = toggleFilesPanel;

  // Snippets
  document.getElementById('btn-new-snippet').onclick = showNewSnippet;
  document.getElementById('btn-close-snippet-modal').onclick = () => closeModal('snippet-modal');
  document.getElementById('btn-cancel-snippet').onclick = () => closeModal('snippet-modal');
  document.getElementById('btn-save-snippet').onclick = saveSnippet;
  document.getElementById('btn-delete-snippet').onclick = deleteSnippet;
  document.getElementById('btn-collapse-snippets').onclick = toggleSnippetsPanel;
  document.getElementById('btn-manage-categories').onclick = showManageCategories;

  // Category Management
  document.getElementById('btn-close-category-modal').onclick = () => closeModal('category-modal');
  document.getElementById('btn-cancel-categories').onclick = () => closeModal('category-modal');
  document.getElementById('btn-save-categories').onclick = saveCategories_modal;

  // SSH
  document.getElementById('btn-close-ssh').onclick = () => closeModal('ssh-modal');
  document.getElementById('btn-new-ssh-profile').onclick = showNewSSHProfile;
  document.getElementById('ssh-search').oninput = (e) => renderSSHProfiles(e.target.value);
  document.getElementById('btn-close-ssh-profile').onclick = () => closeModal('ssh-profile-modal');
  document.getElementById('btn-save-ssh-profile').onclick = saveSSHProfile;
  document.getElementById('btn-test-ssh').onclick = testSSHConnection;
  document.getElementById('ssh-auth-method').onchange = toggleSSHAuthMethod;

  // Error Monitor
  document.getElementById('btn-clear-errors').onclick = clearErrors;
  document.getElementById('btn-analyze-errors').onclick = analyzeAllErrors;
  document.getElementById('btn-collapse-errors').onclick = toggleErrorPanel;

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
  document.getElementById('btn-close-triggers').onclick = () => closeModal('triggers-modal');
  document.getElementById('btn-new-trigger').onclick = showNewTrigger;
  document.getElementById('btn-test-notifications').onclick = testNotification;
  document.getElementById('trigger-search').oninput = (e) => renderTriggers(e.target.value);
  document.getElementById('btn-close-trigger-edit').onclick = () => closeModal('trigger-edit-modal');
  document.getElementById('btn-save-trigger').onclick = saveTrigger;

  // Share
  document.getElementById('btn-close-share').onclick = () => closeModal('share-modal');
  document.getElementById('btn-copy-share-code').onclick = copyShareCode;

  // History
  document.getElementById('btn-close-history').onclick = () => closeModal('history-modal');
  document.getElementById('history-search').oninput = (e) => renderHistoryResults(e.target.value);

  // LLM provider/model now handled by cascading dropdown event delegation (see above)

  // Shell type change
  document.getElementById('shell-type').onchange = (e) => {
    const customGroup = document.getElementById('custom-shell-group');
    customGroup.style.display = e.target.value === 'custom' ? 'block' : 'none';
  };

  // Terminal opacity slider
  document.getElementById('terminal-opacity').oninput = (e) => {
    document.getElementById('opacity-value').textContent = e.target.value;
  };

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

  // Close modals on escape
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      const modals = document.querySelectorAll('.modal.show');
      modals.forEach(modal => modal.classList.remove('show'));
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
    // Open at 1/3 of screen width (min 300px, max 800px), user can then resize
    const targetWidth = Math.max(300, Math.min(800, Math.round(window.innerWidth / 3)));
    panel.style.width = targetWidth + 'px';
    // Force layout recalculation
    panel.offsetHeight;
    // Load home directory on first open
    if (!currentDirectory) {
      loadHomeDirectory();
    }
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
  const filesListEl = document.getElementById('files-list');
  const breadcrumbEl = document.getElementById('files-breadcrumb');

  // Update breadcrumb
  breadcrumbEl.textContent = listing.path;

  // Clear current list
  filesListEl.innerHTML = '';

  // Add parent directory link if not at root
  if (listing.path !== '/') {
    const parentItem = createFileItem({
      name: '..',
      path: listing.path.split('/').slice(0, -1).join('/') || '/',
      is_directory: true,
      size: 0,
      modified: 0
    });
    filesListEl.appendChild(parentItem);
  }

  // Add all entries
  listing.entries.forEach(entry => {
    const item = createFileItem(entry);
    filesListEl.appendChild(item);
  });
}

function createFileItem(entry) {
  const item = document.createElement('div');
  item.className = 'file-item' + (entry.is_directory ? ' file-item-directory' : '');
  item.draggable = true;

  // Icon
  const icon = document.createElement('span');
  icon.className = 'file-icon';
  icon.textContent = entry.is_directory ? '📁' : '📄';

  // Name
  const name = document.createElement('span');
  name.className = 'file-name';
  name.textContent = entry.name;

  // Size (only for files)
  if (!entry.is_directory && entry.name !== '..') {
    const size = document.createElement('span');
    size.className = 'file-size';
    size.textContent = formatFileSize(entry.size);
    item.appendChild(icon);
    item.appendChild(name);
    item.appendChild(size);
  } else {
    item.appendChild(icon);
    item.appendChild(name);
  }

  // Click handler
  item.onclick = () => {
    if (entry.is_directory) {
      // Single click on folder → Navigate into it
      loadDirectory(entry.path);
    } else {
      // Single click on file → Insert path to terminal
      insertPathToTerminal(entry.path);
    }
  };

  // Drag start handler
  item.addEventListener('dragstart', (e) => {
    e.dataTransfer.setData('text/plain', entry.path);
    e.dataTransfer.effectAllowed = 'copy';
  });

  return item;
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

  // Get the first terminal in the active tab
  const terminal = tab.terminals[0];
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
