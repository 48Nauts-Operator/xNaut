// ABOUTME: Ralph Ultra module entry point — exports all public API.
// ABOUTME: Import this file to access the orchestrator and all supporting modules.

export { RALPH_EVENTS, PROCESS_STATES, EXECUTION_MODES, MODEL_CATALOG, KNOWN_CLIS, estimateCost } from './ralph-types.js';
export { detectTaskType } from './ralph-task-detector.js';
export { getRecommendedModel } from './ralph-capability-matrix.js';
export { costTracker } from './ralph-cost-tracker.js';
export { learningRecorder } from './ralph-learning.js';
export { generateExecutionPlan } from './ralph-execution-planner.js';
export { buildPrompt, buildCliArgs } from './ralph-prompt-builder.js';
export { orchestrator } from './ralph-orchestrator.js';
