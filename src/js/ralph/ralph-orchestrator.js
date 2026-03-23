// ABOUTME: Main Ralph orchestrator — drives the story execution lifecycle.
// ABOUTME: Picks stories, dispatches AI CLIs via PTY, runs AC tests, retries, and updates PRD.

import {
  RALPH_EVENTS, PROCESS_STATES, MAX_RETRIES, MAX_ITERATIONS,
  AC_TEST_TIMEOUT_MS, TOKEN_ESTIMATES, estimateCost,
} from './ralph-types.js';
import { detectTaskType } from './ralph-task-detector.js';
import { getRecommendedModel } from './ralph-capability-matrix.js';
import { costTracker } from './ralph-cost-tracker.js';
import { learningRecorder } from './ralph-learning.js';
import { generateExecutionPlan } from './ralph-execution-planner.js';
import { buildPrompt, buildCliArgs } from './ralph-prompt-builder.js';
import { invoke, listen } from './tauri-bridge.js';

/**
 * Ralph Orchestrator — main execution engine.
 */
class RalphOrchestrator {
  constructor() {
    this.state = PROCESS_STATES.IDLE;
    this.projectPath = null;
    this.prd = null;
    this.plan = null;
    this.currentStoryId = null;
    this.currentSessionId = null; // PTY session ID for the running agent
    this.retryCount = 0;
    this.iteration = 0;
    this.executionMode = 'balanced';
    this.activeCli = null;
    this.logLines = [];
    this._stopRequested = false;
  }

  // ==================== Public API ====================

  /**
   * Load a project and its PRD.
   * @param {string} projectPath
   */
  async loadProject(projectPath) {
    this.projectPath = projectPath;
    try {
      this.prd = await invoke('ralph_read_prd', { projectPath });
      this._log(`Loaded PRD: ${this.prd.project} (${this.prd.userStories.length} stories)`);
      this._emit(RALPH_EVENTS.LOG, { message: `PRD loaded: ${this.prd.project}` });
    } catch (e) {
      this._log(`Failed to load PRD: ${e}`);
      throw e;
    }
  }

  /**
   * Detect available CLIs.
   */
  async detectClis() {
    const clis = await invoke('ralph_detect_clis');
    this._emit(RALPH_EVENTS.CLI_DETECTED, { clis });
    this._log(`Detected CLIs: ${clis.filter(c => c.healthy).map(c => c.name).join(', ') || 'none'}`);
    return clis;
  }

  /**
   * Generate an execution plan for the loaded PRD.
   */
  generatePlan() {
    if (!this.prd) throw new Error('No PRD loaded');
    this.plan = generateExecutionPlan(this.prd, this.executionMode);
    this._emit(RALPH_EVENTS.PLAN_READY, { plan: this.plan });
    this._log(`Plan generated: ${this.plan.summary.totalStories} stories, est. $${this.plan.summary.estimatedTotalCost.toFixed(2)}`);
    return this.plan;
  }

  /**
   * Start executing stories from the loaded PRD.
   */
  async run() {
    if (this.state === PROCESS_STATES.RUNNING) {
      this._log('Already running');
      return;
    }
    if (!this.prd) {
      this._log('No PRD loaded');
      return;
    }

    this.state = PROCESS_STATES.RUNNING;
    this._stopRequested = false;
    this._emit(RALPH_EVENTS.EXECUTION_STARTED, {});

    // Detect CLI to use
    const cli = await this._selectCli();
    if (!cli) {
      this._log('No healthy AI CLI found. Install claude, aider, or codex.');
      this.state = PROCESS_STATES.IDLE;
      return;
    }
    this.activeCli = cli;
    this._log(`Using CLI: ${cli}`);

    // Execute stories in priority order
    const pendingStories = this.prd.userStories
      .filter(s => !s.passes && !s.skipped)
      .sort((a, b) => (a.priority || 99) - (b.priority || 99));

    for (const story of pendingStories) {
      if (this._stopRequested) break;
      if (this.state === PROCESS_STATES.PAUSED) break;

      await this._executeStory(story);
    }

    if (this._stopRequested) {
      this.state = PROCESS_STATES.IDLE;
      this._emit(RALPH_EVENTS.EXECUTION_STOPPED, {});
      this._log('Execution stopped by user');
    } else {
      this.state = PROCESS_STATES.IDLE;
      this._emit(RALPH_EVENTS.EXECUTION_COMPLETE, {
        costs: costTracker.getSessionCosts()
      });
      this._log('Execution complete');
    }
  }

  /**
   * Pause execution after the current story finishes.
   */
  pause() {
    if (this.state !== PROCESS_STATES.RUNNING) return;
    this.state = PROCESS_STATES.PAUSED;
    this._emit(RALPH_EVENTS.EXECUTION_PAUSED, { storyId: this.currentStoryId });
    this._log('Pausing after current story...');
  }

  /**
   * Stop execution immediately.
   */
  async stop() {
    this._stopRequested = true;
    if (this.currentSessionId) {
      try {
        await invoke('close_terminal', { sessionId: this.currentSessionId });
      } catch { /* may already be closed */ }
      this.currentSessionId = null;
    }
    this.state = PROCESS_STATES.IDLE;
    this._emit(RALPH_EVENTS.EXECUTION_STOPPED, {});
    this._log('Stopped');
  }

  /**
   * Resume from paused state.
   */
  async resume() {
    if (this.state !== PROCESS_STATES.PAUSED) return;
    await this.run();
  }

  /**
   * Get current state.
   */
  getState() {
    return {
      state: this.state,
      projectPath: this.projectPath,
      prd: this.prd,
      plan: this.plan,
      currentStoryId: this.currentStoryId,
      activeCli: this.activeCli,
      executionMode: this.executionMode,
      logLines: this.logLines,
      costs: costTracker.getSessionCosts(),
    };
  }

  /**
   * Run AC tests for a specific story.
   */
  async runAcTests(storyId) {
    if (!this.prd) return null;
    const story = this.prd.userStories.find(s => s.id === storyId);
    if (!story) return null;
    return await this._runAcTests(story);
  }

  // ==================== Internal: Story Execution ====================

  async _executeStory(story) {
    this.currentStoryId = story.id;
    this.retryCount = 0;
    this.iteration = 0;

    const taskType = detectTaskType(story);
    const complexity = story.complexity || 'medium';
    const tokens = TOKEN_ESTIMATES[complexity] || TOKEN_ESTIMATES.medium;
    const model = getRecommendedModel(taskType, null, this.executionMode);
    const estCost = estimateCost(model.modelId, tokens.input, tokens.output);

    this._log(`Starting story ${story.id}: ${story.title} [${taskType}] → ${model.modelId}`);
    this._emit(RALPH_EVENTS.STORY_STARTED, { story, taskType, model });

    costTracker.startStory(story.id, model.modelId, model.provider, estCost, 0);
    const startTime = Date.now();

    let success = false;

    while (this.retryCount < MAX_RETRIES && this.iteration < MAX_ITERATIONS && !this._stopRequested) {
      this.iteration++;

      // Build prompt
      const failedACs = this.retryCount > 0
        ? story.acceptanceCriteria.filter(ac => typeof ac === 'object' && !ac.passes)
        : null;

      const prompt = buildPrompt(story, this.prd, {
        isRetry: this.retryCount > 0,
        failedACs,
      });

      // Write prompt to temp file
      const promptPath = await invoke('ralph_write_temp_file', {
        content: prompt,
        prefix: `ralph-${story.id}`,
      });

      try {
        // Spawn agent in a command PTY session
        const args = buildCliArgs(this.activeCli, promptPath, model.modelId);

        // For claude CLI, we need to use shell to expand $(cat ...)
        const result = await invoke('create_command_session', {
          config: {
            program: 'sh',
            args: ['-c', `${this.activeCli} ${args.join(' ')}`],
            workingDir: this.projectPath,
          }
        });

        this.currentSessionId = result.session_id;
        this._log(`Agent session: ${this.currentSessionId}`);

        // Wait for the agent to finish (listen for terminal-closed event)
        const exitCode = await this._waitForSession(this.currentSessionId);
        this._log(`Agent exited with code ${exitCode}`);

        this.currentSessionId = null;

        // Clean up prompt file
        await invoke('ralph_cleanup_temp_file', { path: promptPath }).catch(() => {});

        // Run AC tests
        const testResults = await this._runAcTests(story);

        if (testResults && testResults.allPassed) {
          success = true;
          story.passes = true;
          this._log(`Story ${story.id} PASSED all ACs`);
          break;
        } else {
          this.retryCount++;
          this._log(`Story ${story.id} failed ACs (retry ${this.retryCount}/${MAX_RETRIES})`);

          if (this.retryCount >= MAX_RETRIES) {
            story.skipped = true;
            this._log(`Story ${story.id} exceeded max retries — skipping`);
          }
        }
      } catch (e) {
        this._log(`Error executing story ${story.id}: ${e}`);
        this.retryCount++;
        // Clean up temp file
        await invoke('ralph_cleanup_temp_file', { path: promptPath }).catch(() => {});

        if (this.retryCount >= MAX_RETRIES) {
          story.skipped = true;
          break;
        }
      }
    }

    // Record cost and learning
    const durationMinutes = (Date.now() - startTime) / 60000;
    const acTotal = story.acceptanceCriteria ? story.acceptanceCriteria.length : 0;
    const acPassed = story.acceptanceCriteria
      ? story.acceptanceCriteria.filter(ac => typeof ac === 'object' && ac.passes).length
      : 0;

    await costTracker.endStory(story.id, estCost, tokens.input, tokens.output, success);

    await learningRecorder.recordRun({
      storyId: story.id,
      storyTitle: story.title,
      taskType,
      complexity,
      provider: model.provider,
      modelId: model.modelId,
      durationMinutes,
      inputTokens: tokens.input,
      outputTokens: tokens.output,
      totalTokens: tokens.input + tokens.output,
      costUSD: estCost,
      success,
      retryCount: this.retryCount,
      acTotal,
      acPassed,
      acPassRate: acTotal > 0 ? acPassed / acTotal : 0,
    });

    // Save updated PRD
    try {
      await invoke('ralph_write_prd', { projectPath: this.projectPath, prd: this.prd });
    } catch (e) {
      this._log(`Failed to save PRD: ${e}`);
    }

    this._emit(success ? RALPH_EVENTS.STORY_COMPLETED : RALPH_EVENTS.STORY_FAILED, {
      story,
      success,
      retryCount: this.retryCount,
      durationMinutes,
    });

    this.currentStoryId = null;
  }

  // ==================== Internal: AC Testing ====================

  async _runAcTests(story) {
    if (!story.acceptanceCriteria || !Array.isArray(story.acceptanceCriteria)) {
      return { results: [], allPassed: true };
    }

    const results = [];
    let allPassed = true;

    for (let i = 0; i < story.acceptanceCriteria.length; i++) {
      const ac = story.acceptanceCriteria[i];
      if (typeof ac === 'string' || !ac.testCommand) {
        results.push({ id: ac.id || `ac-${i}`, passes: true, skipped: true });
        continue;
      }

      try {
        const result = await invoke('ralph_run_ac_test', {
          command: ac.testCommand,
          cwd: this.projectPath,
          timeoutMs: AC_TEST_TIMEOUT_MS,
        });

        ac.passes = result.passes;
        ac.lastRun = new Date().toISOString();
        results.push({ id: ac.id, passes: result.passes, exitCode: result.exitCode });

        if (!result.passes) allPassed = false;

        this._emit(RALPH_EVENTS.AC_PROGRESS, {
          storyId: story.id,
          acId: ac.id,
          passes: result.passes,
          current: i + 1,
          total: story.acceptanceCriteria.length,
        });
      } catch (e) {
        ac.passes = false;
        ac.lastRun = new Date().toISOString();
        results.push({ id: ac.id, passes: false, error: e.toString() });
        allPassed = false;
      }
    }

    story.passes = allPassed;
    return { results, allPassed };
  }

  // ==================== Internal: CLI Selection ====================

  async _selectCli() {
    // 1. Check PRD-level CLI override
    if (this.prd.cli) {
      const healthy = await invoke('ralph_check_cli_health', { cli: this.prd.cli });
      if (healthy) return this.prd.cli;
    }

    // 2. Check PRD fallback order
    if (this.prd.cliFallbackOrder) {
      for (const cli of this.prd.cliFallbackOrder) {
        const healthy = await invoke('ralph_check_cli_health', { cli });
        if (healthy) return cli;
      }
    }

    // 3. Auto-detect
    const clis = await invoke('ralph_detect_clis');
    const healthy = clis.filter(c => c.healthy);
    return healthy.length > 0 ? healthy[0].name : null;
  }

  // ==================== Internal: Session Monitoring ====================

  _waitForSession(sessionId) {
    return new Promise((resolve) => {
      const eventName = `terminal-closed:${sessionId}`;
      let cleanup = null;

      const handler = (event) => {
        const exitCode = event.payload?.exitCode ?? -1;
        if (cleanup) cleanup();
        resolve(exitCode);
      };

      // Use Tauri listen
      if (typeof listen === 'function') {
        listen(eventName, handler).then(unlisten => {
          cleanup = unlisten;
        });
      } else {
        // Fallback: poll
        const interval = setInterval(async () => {
          try {
            const sessions = await invoke('list_terminal_sessions');
            const found = sessions.find(s => s.id === sessionId);
            if (!found) {
              clearInterval(interval);
              resolve(0);
            }
          } catch {
            clearInterval(interval);
            resolve(-1);
          }
        }, 2000);

        cleanup = () => clearInterval(interval);
      }
    });
  }

  // ==================== Internal: Logging & Events ====================

  _log(message) {
    const entry = `[${new Date().toLocaleTimeString()}] ${message}`;
    this.logLines.push(entry);
    // Keep last 500 lines
    if (this.logLines.length > 500) {
      this.logLines = this.logLines.slice(-500);
    }
    console.log(`[Ralph] ${message}`);
    this._emit(RALPH_EVENTS.LOG, { message: entry });
  }

  _emit(eventType, detail) {
    window.dispatchEvent(new CustomEvent(eventType, { detail }));
  }
}

export const orchestrator = new RalphOrchestrator();
