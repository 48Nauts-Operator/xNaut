// ABOUTME: Per-story cost tracking with estimated vs actual cost recording.
// ABOUTME: Persists cost history to ~/.config/ralph-ultra/cost-history.json via Tauri backend.

import { RALPH_EVENTS } from './ralph-types.js';
import { invoke } from './tauri-bridge.js';

/**
 * Cost tracker for story executions.
 * Uses Tauri invoke for persistence (ralph_read_config / ralph_write_config).
 */
class CostTracker {
  constructor() {
    this.inProgress = new Map(); // storyId → { storyId, modelId, provider, startTime, estimatedCost, retryCount }
    this.sessionRecords = [];     // StoryExecutionRecord[]
  }

  /**
   * Start tracking a story execution.
   */
  startStory(storyId, modelId, provider, estimatedCost, retryCount = 0) {
    this.inProgress.set(storyId, {
      storyId,
      modelId,
      provider,
      startTime: new Date().toISOString(),
      estimatedCost,
      retryCount,
    });
  }

  /**
   * End tracking for a story. Returns the completed record or null.
   */
  async endStory(storyId, actualCost, inputTokens, outputTokens, success) {
    const inProg = this.inProgress.get(storyId);
    if (!inProg) {
      console.warn(`CostTracker: no in-progress story for ${storyId}`);
      return null;
    }

    const record = {
      storyId: inProg.storyId,
      modelId: inProg.modelId,
      provider: inProg.provider,
      startTime: inProg.startTime,
      endTime: new Date().toISOString(),
      estimatedCost: inProg.estimatedCost,
      actualCost,
      inputTokens,
      outputTokens,
      success,
      retryCount: inProg.retryCount,
    };

    this.sessionRecords.push(record);
    this.inProgress.delete(storyId);

    // Persist to disk
    await this._persistRecord(record);

    // Emit event
    window.dispatchEvent(new CustomEvent(RALPH_EVENTS.COST_UPDATE, {
      detail: this.getSessionCosts()
    }));

    return record;
  }

  /**
   * Get cost summary for the current session.
   */
  getSessionCosts() {
    const totalEstimated = this.sessionRecords.reduce((s, r) => s + r.estimatedCost, 0);
    const totalActual = this.sessionRecords.reduce((s, r) => s + r.actualCost, 0);
    const storiesSuccessful = this.sessionRecords.filter(r => r.success).length;

    return {
      totalEstimated,
      totalActual,
      storiesCompleted: this.sessionRecords.length,
      storiesSuccessful,
      records: [...this.sessionRecords],
    };
  }

  /**
   * Load all historical cost records from disk.
   */
  async getAllHistory() {
    try {
      const data = await invoke('ralph_read_config', { filename: 'cost-history.json' });
      return (data && data.records) ? data.records : [];
    } catch {
      return [];
    }
  }

  /**
   * Clear session records (doesn't affect persisted history).
   */
  clearSession() {
    this.sessionRecords = [];
    this.inProgress.clear();
  }

  /**
   * Persist a record to cost-history.json.
   */
  async _persistRecord(record) {
    try {
      const data = await invoke('ralph_read_config', { filename: 'cost-history.json' });
      const history = {
        version: '1.0',
        lastUpdated: new Date().toISOString(),
        records: (data && data.records) ? data.records : [],
      };
      history.records.push(record);
      await invoke('ralph_write_config', { filename: 'cost-history.json', data: history });
    } catch (e) {
      console.error('CostTracker: failed to persist record:', e);
    }
  }
}

export const costTracker = new CostTracker();
