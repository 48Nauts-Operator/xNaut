// ABOUTME: Learning recorder — tracks model performance for future recommendations.
// ABOUTME: Calculates efficiency/speed/reliability scores and aggregates per model+taskType.

import { invoke } from './tauri-bridge.js';

/**
 * Learning recorder for model performance data.
 * Persists to ~/.config/ralph-ultra/learning.json via Tauri backend.
 */
class LearningRecorder {
  constructor() {
    this.db = null; // loaded lazily
  }

  async _ensureLoaded() {
    if (this.db) return;
    try {
      const data = await invoke('ralph_read_config', { filename: 'learning.json' });
      this.db = (data && data.version) ? data : this._emptyDB();
    } catch {
      this.db = this._emptyDB();
    }
  }

  _emptyDB() {
    return {
      version: '1.0',
      lastUpdated: new Date().toISOString(),
      runs: [],
      learnings: {},
    };
  }

  async _save() {
    try {
      this.db.lastUpdated = new Date().toISOString();
      await invoke('ralph_write_config', { filename: 'learning.json', data: this.db });
    } catch (e) {
      console.error('LearningRecorder: failed to save:', e);
    }
  }

  // ==================== Scoring ====================

  _efficiencyScore(record) {
    if (record.costUSD === 0) return 100;
    if (record.acPassRate === 0) return 0;
    return Math.min(100, Math.max(0, (record.acPassRate * 100) / (record.costUSD * 100)));
  }

  _speedScore(record) {
    if (record.durationMinutes <= 0) return 100;
    return Math.min(100, Math.max(0, 100 / record.durationMinutes));
  }

  _reliabilityScore(record) {
    const successWeight = record.success ? 1.0 : 0.5;
    const retryPenalty = Math.max(0, 1 - record.retryCount * 0.1);
    return record.acPassRate * 100 * successWeight * retryPenalty;
  }

  // ==================== Recording ====================

  /**
   * Record a completed story run.
   * @param {object} record - { storyId, storyTitle, taskType, complexity, provider, modelId,
   *   durationMinutes, inputTokens, outputTokens, totalTokens, costUSD,
   *   success, retryCount, acTotal, acPassed, acPassRate }
   */
  async recordRun(record) {
    await this._ensureLoaded();

    const complete = {
      ...record,
      id: `${record.storyId}-${Date.now()}`,
      timestamp: new Date().toISOString(),
      efficiencyScore: this._efficiencyScore(record),
      speedScore: this._speedScore(record),
      reliabilityScore: this._reliabilityScore(record),
    };

    this.db.runs.push(complete);
    this._updateLearnings(complete);
    await this._save();

    return complete;
  }

  /**
   * Update aggregated stats for model+taskType.
   */
  _updateLearnings(record) {
    const key = `${record.provider}:${record.modelId}`;
    if (!this.db.learnings[key]) this.db.learnings[key] = {};

    const relevant = this.db.runs.filter(
      r => r.provider === record.provider && r.modelId === record.modelId && r.taskType === record.taskType
    );
    if (relevant.length === 0) return;

    const n = relevant.length;
    const avg = (arr, fn) => arr.reduce((s, r) => s + fn(r), 0) / n;

    const successfulRuns = relevant.filter(r => r.success).length;
    const avgReliability = avg(relevant, r => r.reliabilityScore);
    const avgEfficiency = avg(relevant, r => r.efficiencyScore);
    const avgSpeed = avg(relevant, r => r.speedScore);

    this.db.learnings[key][record.taskType] = {
      modelId: record.modelId,
      provider: record.provider,
      taskType: record.taskType,
      totalRuns: n,
      successfulRuns,
      successRate: successfulRuns / n,
      avgDurationMinutes: avg(relevant, r => r.durationMinutes),
      avgCostUSD: avg(relevant, r => r.costUSD),
      avgTokens: avg(relevant, r => r.totalTokens),
      avgAcPassRate: avg(relevant, r => r.acPassRate),
      efficiencyScore: avgEfficiency,
      speedScore: avgSpeed,
      reliabilityScore: avgReliability,
      overallScore: avgReliability * 0.4 + avgEfficiency * 0.35 + avgSpeed * 0.25,
      lastUpdated: new Date().toISOString(),
    };
  }

  // ==================== Queries ====================

  /**
   * Get best performing model for a task type.
   * @param {string} taskType
   * @param {number} [minRuns=3]
   * @returns {object|null}
   */
  async getBestModelForTask(taskType, minRuns = 3) {
    await this._ensureLoaded();

    const candidates = [];
    for (const modelKey in this.db.learnings) {
      const learning = this.db.learnings[modelKey]?.[taskType];
      if (learning && learning.totalRuns >= minRuns) {
        candidates.push(learning);
      }
    }

    if (candidates.length === 0) return null;
    candidates.sort((a, b) => b.overallScore - a.overallScore);
    return candidates[0];
  }

  /**
   * Get stats for a specific model+taskType.
   */
  async getModelStats(provider, modelId, taskType) {
    await this._ensureLoaded();
    const key = `${provider}:${modelId}`;
    return this.db.learnings[key]?.[taskType] || null;
  }

  async getAllLearnings() {
    await this._ensureLoaded();
    return this.db;
  }
}

export const learningRecorder = new LearningRecorder();
