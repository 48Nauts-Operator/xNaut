// ABOUTME: Ralph panel UI controller — binds DOM elements to orchestrator API.
// ABOUTME: Loaded as ES module. Exposes window.ralphUI for app.js integration.

import { orchestrator } from './ralph-orchestrator.js';
import { RALPH_EVENTS, PROCESS_STATES, EXECUTION_MODES } from './ralph-types.js';

/**
 * Ralph UI Controller — manages the panel lifecycle.
 */
class RalphUI {
  constructor() {
    this._initialized = false;
    this._elements = {};
  }

  // ==================== Initialization ====================

  init() {
    if (this._initialized) return;
    this._cacheElements();
    this._bindButtons();
    this._bindModeButtons();
    this._bindEvents();
    this._initialized = true;
    this._log('Ralph UI initialized');
  }

  _cacheElements() {
    const $ = (id) => document.getElementById(id);
    this._elements = {
      panel:          $('ralph-panel'),
      statusBadge:    $('ralph-status-badge'),
      collapseBtn:    $('btn-collapse-ralph'),
      projectPath:    $('ralph-project-path'),
      loadBtn:        $('btn-ralph-load'),
      runBtn:         $('btn-ralph-run'),
      pauseBtn:       $('btn-ralph-pause'),
      stopBtn:        $('btn-ralph-stop'),
      testBtn:        $('btn-ralph-test'),
      activeCli:      $('ralph-active-cli'),
      storyList:      $('ralph-story-list'),
      storyProgress:  $('ralph-story-progress'),
      costEstimated:  $('ralph-cost-estimated'),
      costActual:     $('ralph-cost-actual'),
      storiesDone:    $('ralph-stories-done'),
      log:            $('ralph-log'),
      clearLogBtn:    $('btn-ralph-clear-log'),
    };
  }

  _bindButtons() {
    const { loadBtn, runBtn, pauseBtn, stopBtn, testBtn, collapseBtn, clearLogBtn } = this._elements;

    loadBtn?.addEventListener('click', () => this._onLoad());
    runBtn?.addEventListener('click', () => this._onRun());
    pauseBtn?.addEventListener('click', () => this._onPause());
    stopBtn?.addEventListener('click', () => this._onStop());
    testBtn?.addEventListener('click', () => this._onTest());
    clearLogBtn?.addEventListener('click', () => this._clearLog());

    collapseBtn?.addEventListener('click', () => {
      this._elements.panel?.classList.toggle('collapsed');
      if (typeof resizeAllTerminals === 'function') {
        requestAnimationFrame(resizeAllTerminals);
      }
    });

    // Enter key in project path field triggers load
    this._elements.projectPath?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this._onLoad();
    });
  }

  _bindModeButtons() {
    const btns = document.querySelectorAll('.ralph-mode-btn');
    btns.forEach(btn => {
      btn.addEventListener('click', () => {
        btns.forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        orchestrator.executionMode = btn.dataset.mode;
        // Regenerate plan if PRD is loaded
        if (orchestrator.prd) {
          orchestrator.generatePlan();
          this._renderPlan();
        }
      });
    });
  }

  _bindEvents() {
    const on = (event, handler) => window.addEventListener(event, (e) => handler(e.detail));

    on(RALPH_EVENTS.LOG, (d) => this._appendLog(d.message));

    on(RALPH_EVENTS.CLI_DETECTED, (d) => {
      const healthy = d.clis.filter(c => c.healthy).map(c => c.name);
      this._elements.activeCli.textContent = healthy.join(', ') || 'none found';
    });

    on(RALPH_EVENTS.PLAN_READY, () => this._renderPlan());

    on(RALPH_EVENTS.EXECUTION_STARTED, () => {
      this._setStatus('running', 'Running');
      this._updateControls('running');
    });

    on(RALPH_EVENTS.EXECUTION_PAUSED, () => {
      this._setStatus('paused', 'Paused');
      this._updateControls('paused');
    });

    on(RALPH_EVENTS.EXECUTION_STOPPED, () => {
      this._setStatus('', 'Idle');
      this._updateControls('idle');
      this._refreshStories();
    });

    on(RALPH_EVENTS.EXECUTION_COMPLETE, () => {
      this._setStatus('complete', 'Done');
      this._updateControls('idle');
      this._refreshStories();
      this._updateCosts();
    });

    on(RALPH_EVENTS.STORY_STARTED, (d) => {
      this._markStory(d.story.id, 'running');
    });

    on(RALPH_EVENTS.STORY_COMPLETED, (d) => {
      this._markStory(d.story.id, d.success ? 'passed' : 'failed');
      this._updateCosts();
      this._updateProgress();
    });

    on(RALPH_EVENTS.STORY_FAILED, (d) => {
      this._markStory(d.story.id, 'failed');
      this._updateCosts();
      this._updateProgress();
    });

    on(RALPH_EVENTS.AC_PROGRESS, (d) => {
      this._appendLog(`  AC ${d.current}/${d.total}: ${d.passes ? 'PASS' : 'FAIL'}`, d.passes ? 'success' : 'error');
    });

    on(RALPH_EVENTS.COST_UPDATE, () => this._updateCosts());
  }

  // ==================== Actions ====================

  async _onLoad() {
    const path = this._elements.projectPath?.value?.trim();
    if (!path) {
      this._appendLog('Enter a project path first', 'error');
      return;
    }

    this._appendLog(`Loading project: ${path}`, 'highlight');
    this._setStatus('', 'Loading...');

    try {
      await orchestrator.loadProject(path);
      const clis = await orchestrator.detectClis();
      orchestrator.generatePlan();
      this._renderPlan();
      this._updateControls('loaded');
      this._setStatus('', 'Ready');

      const healthy = clis.filter(c => c.healthy).map(c => c.name);
      this._elements.activeCli.textContent = healthy.join(', ') || 'none';
    } catch (e) {
      this._appendLog(`Failed to load: ${e}`, 'error');
      this._setStatus('error', 'Error');
    }
  }

  async _onRun() {
    this._setStatus('running', 'Running');
    this._updateControls('running');
    try {
      await orchestrator.run();
    } catch (e) {
      this._appendLog(`Run error: ${e}`, 'error');
      this._setStatus('error', 'Error');
      this._updateControls('idle');
    }
  }

  _onPause() {
    orchestrator.pause();
  }

  async _onStop() {
    await orchestrator.stop();
  }

  async _onTest() {
    if (!orchestrator.currentStoryId && orchestrator.prd) {
      const pending = orchestrator.prd.userStories.find(s => !s.passes && !s.skipped);
      if (pending) {
        this._appendLog(`Testing story: ${pending.id}`, 'highlight');
        const result = await orchestrator.runAcTests(pending.id);
        this._appendLog(`Test result: ${result?.allPassed ? 'ALL PASSED' : 'SOME FAILED'}`,
          result?.allPassed ? 'success' : 'error');
        this._refreshStories();
      }
    }
  }

  // ==================== Panel Toggle (called from app.js) ====================

  toggle() {
    const panel = this._elements.panel || document.getElementById('ralph-panel');
    if (!panel) return;
    const isHidden = panel.style.display === 'none' || !panel.style.display;
    if (isHidden) {
      panel.style.display = 'flex';
      const targetWidth = Math.max(300, Math.min(600, Math.round(window.innerWidth / 3.5)));
      panel.style.width = targetWidth + 'px';
      panel.offsetHeight; // force layout
      if (!this._initialized) this.init();
    } else {
      panel.style.display = 'none';
    }
    requestAnimationFrame(() => {
      if (typeof resizeAllTerminals === 'function') resizeAllTerminals();
    });
  }

  isVisible() {
    const panel = this._elements.panel || document.getElementById('ralph-panel');
    return panel && panel.style.display !== 'none' && panel.style.display !== '';
  }

  // ==================== Rendering ====================

  _renderPlan() {
    const plan = orchestrator.plan;
    if (!plan) return;

    const list = this._elements.storyList;
    if (!list) return;
    list.textContent = ''; // clear safely

    plan.stories.forEach(s => {
      const item = document.createElement('div');
      item.className = 'ralph-story-item';
      item.dataset.storyId = s.storyId;

      // Find original story for pass/skip status
      const original = orchestrator.prd?.userStories.find(us => us.id === s.storyId);
      let statusClass = 'pending';
      let icon = '\u25CB'; // ○
      if (original?.passes) { statusClass = 'passed'; icon = '\u2713'; } // ✓
      else if (original?.skipped) { statusClass = 'skipped'; icon = '\u2298'; } // ⊘

      const iconSpan = document.createElement('span');
      iconSpan.className = 'ralph-story-icon';
      iconSpan.textContent = icon;

      const titleSpan = document.createElement('span');
      titleSpan.className = 'ralph-story-title';
      titleSpan.title = s.title;
      titleSpan.textContent = `${s.storyId}: ${s.title}`;

      const badgeSpan = document.createElement('span');
      badgeSpan.className = `ralph-story-badge ${statusClass}`;
      badgeSpan.textContent = statusClass;

      item.appendChild(iconSpan);
      item.appendChild(titleSpan);
      item.appendChild(badgeSpan);
      list.appendChild(item);
    });

    // Update summary
    this._updateProgress();
    this._updateCosts();

    // Show estimated cost
    this._elements.costEstimated.textContent = `$${plan.summary.estimatedTotalCost.toFixed(2)}`;
  }

  _markStory(storyId, status) {
    const item = this._elements.storyList?.querySelector(`[data-story-id="${storyId}"]`);
    if (!item) return;

    const icons = { running: '\u27F3', passed: '\u2713', failed: '\u2717', skipped: '\u2298', pending: '\u25CB' };
    const icon = item.querySelector('.ralph-story-icon');
    const badge = item.querySelector('.ralph-story-badge');

    if (icon) icon.textContent = icons[status] || '\u25CB';
    if (badge) {
      badge.className = `ralph-story-badge ${status}`;
      badge.textContent = status;
    }

    if (status === 'running') item.classList.add('active');
    else item.classList.remove('active');

    if (status === 'passed') item.classList.add('passed');
  }

  _refreshStories() {
    if (orchestrator.prd && orchestrator.plan) {
      this._renderPlan();
    }
  }

  _updateProgress() {
    if (!orchestrator.prd) return;
    const stories = orchestrator.prd.userStories;
    const passed = stories.filter(s => s.passes).length;
    const total = stories.filter(s => !s.skipped).length;
    this._elements.storyProgress.textContent = `${passed}/${total}`;
    this._elements.storiesDone.textContent = `${passed}/${total}`;
  }

  _updateCosts() {
    const costs = orchestrator.getState().costs;
    if (!costs) return;
    const actual = typeof costs.totalActual === 'number' ? costs.totalActual : 0;
    this._elements.costActual.textContent = `$${actual.toFixed(2)}`;
  }

  // ==================== Status & Controls ====================

  _setStatus(className, text) {
    const badge = this._elements.statusBadge;
    if (!badge) return;
    badge.className = `ralph-status-badge ${className}`;
    badge.textContent = text;
  }

  _updateControls(state) {
    const { runBtn, pauseBtn, stopBtn, testBtn } = this._elements;
    switch (state) {
      case 'idle':
        runBtn.disabled = !orchestrator.prd;
        pauseBtn.disabled = true;
        stopBtn.disabled = true;
        testBtn.disabled = !orchestrator.prd;
        break;
      case 'loaded':
        runBtn.disabled = false;
        pauseBtn.disabled = true;
        stopBtn.disabled = true;
        testBtn.disabled = false;
        break;
      case 'running':
        runBtn.disabled = true;
        pauseBtn.disabled = false;
        stopBtn.disabled = false;
        testBtn.disabled = true;
        break;
      case 'paused':
        runBtn.disabled = false;
        pauseBtn.disabled = true;
        stopBtn.disabled = false;
        testBtn.disabled = false;
        break;
    }
  }

  // ==================== Log ====================

  _appendLog(message, type = '') {
    const log = this._elements.log;
    if (!log) return;
    const entry = document.createElement('div');
    entry.className = `ralph-log-entry ${type}`;
    entry.textContent = message;
    log.appendChild(entry);
    log.scrollTop = log.scrollHeight;

    // Keep last 200 entries
    while (log.children.length > 200) {
      log.removeChild(log.firstChild);
    }
  }

  _log(message) {
    this._appendLog(`[${new Date().toLocaleTimeString()}] ${message}`);
  }

  _clearLog() {
    if (this._elements.log) {
      this._elements.log.textContent = '';
    }
  }
}

// ── Singleton & Global Export ──
const ralphUI = new RalphUI();
window.ralphUI = ralphUI;

// Auto-init if panel already visible (unlikely but safe)
if (document.readyState === 'complete' || document.readyState === 'interactive') {
  // Defer to ensure DOM is fully parsed
  setTimeout(() => {
    if (document.getElementById('ralph-panel')?.style.display !== 'none') {
      ralphUI.init();
    }
  }, 0);
}

export { ralphUI };
