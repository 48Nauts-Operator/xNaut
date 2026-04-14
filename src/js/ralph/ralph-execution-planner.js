// ABOUTME: Execution plan generator — estimates cost, duration, and model allocation per story.
// ABOUTME: Produces plan with mode comparisons (balanced, super-saver, fast-delivery, allClaude, allLocal).

import { TOKEN_ESTIMATES, estimateCost, MODEL_CATALOG } from './ralph-types.js';
import { detectTaskType } from './ralph-task-detector.js';
import { getRecommendedModel } from './ralph-capability-matrix.js';

/**
 * Generate an execution plan for a PRD.
 *
 * @param {object} prd - The PRD with userStories
 * @param {string} [mode='balanced'] - Execution mode
 * @param {object} [quotas] - Provider quotas
 * @returns {object} ExecutionPlan
 */
export function generateExecutionPlan(prd, mode = 'balanced', quotas = null) {
  const stories = [];
  let totalCost = 0;
  let totalDuration = 0;

  for (const story of prd.userStories) {
    if (story.passes || story.skipped) continue; // Skip completed/skipped stories

    const taskType = detectTaskType(story);
    const complexity = story.complexity || 'medium';
    const tokens = TOKEN_ESTIMATES[complexity] || TOKEN_ESTIMATES.medium;

    const recommendation = getRecommendedModel(taskType, quotas, mode);
    const cost = estimateCost(recommendation.modelId, tokens.input, tokens.output);

    const allocation = {
      storyId: story.id,
      title: story.title,
      taskType,
      complexity,
      recommendedModel: {
        provider: recommendation.provider,
        modelId: recommendation.modelId,
        reason: recommendation.reason,
      },
      estimatedTokens: { input: tokens.input, output: tokens.output },
      estimatedCost: cost,
      estimatedDuration: tokens.durationMinutes,
    };

    stories.push(allocation);
    totalCost += cost;
    totalDuration += tokens.durationMinutes;
  }

  // Generate mode comparisons
  const comparisons = generateComparisons(prd, quotas);

  return {
    projectPath: prd.project || '',
    generatedAt: new Date().toISOString(),
    selectedMode: mode,
    stories,
    summary: {
      totalStories: stories.length,
      estimatedTotalCost: totalCost,
      estimatedTotalDuration: totalDuration,
      modelsUsed: [...new Set(stories.map(s => s.recommendedModel.modelId))],
    },
    comparisons,
  };
}

/**
 * Generate cost/duration comparisons across modes.
 */
function generateComparisons(prd, quotas) {
  const modes = ['balanced', 'super-saver', 'fast-delivery'];
  const result = {};

  for (const m of modes) {
    let cost = 0;
    let duration = 0;
    for (const story of prd.userStories) {
      if (story.passes || story.skipped) continue;
      const taskType = detectTaskType(story);
      const complexity = story.complexity || 'medium';
      const tokens = TOKEN_ESTIMATES[complexity] || TOKEN_ESTIMATES.medium;
      const rec = getRecommendedModel(taskType, quotas, m);
      cost += estimateCost(rec.modelId, tokens.input, tokens.output);
      duration += tokens.durationMinutes;
    }
    result[m] = { cost, duration };
  }

  // All-Claude comparison (using Sonnet 4)
  result.allClaude = computeFixedModelCost(prd, 'claude-sonnet-4-20250514');

  // All-local comparison (free)
  result.allLocal = computeFixedModelCost(prd, 'llama-3.1-70b');

  return result;
}

/**
 * Compute cost/duration if all stories used a single model.
 */
function computeFixedModelCost(prd, modelId) {
  let cost = 0;
  let duration = 0;
  for (const story of prd.userStories) {
    if (story.passes || story.skipped) continue;
    const complexity = story.complexity || 'medium';
    const tokens = TOKEN_ESTIMATES[complexity] || TOKEN_ESTIMATES.medium;
    cost += estimateCost(modelId, tokens.input, tokens.output);
    duration += tokens.durationMinutes;
  }
  return { cost, duration };
}
