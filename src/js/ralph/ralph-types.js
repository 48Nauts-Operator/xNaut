// ABOUTME: Ralph Ultra type constants and data catalogs.
// ABOUTME: Defines task types, model catalog with pricing, execution modes, and keyword maps.

// ==================== Task Types ====================

export const TASK_TYPES = [
  'complex-integration', 'mathematical', 'backend-api', 'backend-logic',
  'frontend-ui', 'frontend-logic', 'database', 'testing',
  'documentation', 'refactoring', 'bugfix', 'devops', 'config', 'unknown'
];

// ==================== Providers ====================

export const PROVIDERS = ['anthropic', 'openai', 'gemini', 'openrouter', 'local'];

// ==================== Process States ====================

export const PROCESS_STATES = {
  IDLE: 'idle',
  RUNNING: 'running',
  STOPPING: 'stopping',
  PAUSED: 'paused',
};

// ==================== Execution Modes ====================

export const EXECUTION_MODES = {
  BALANCED: 'balanced',
  SUPER_SAVER: 'super-saver',
  FAST_DELIVERY: 'fast-delivery',
};

// ==================== Complexity Levels ====================

export const COMPLEXITY = {
  SIMPLE: 'simple',
  MEDIUM: 'medium',
  COMPLEX: 'complex',
};

// ==================== Token Estimates by Complexity ====================

export const TOKEN_ESTIMATES = {
  simple:  { input: 5000,  output: 2000,  durationMinutes: 15 },
  medium:  { input: 15000, output: 6000,  durationMinutes: 30 },
  complex: { input: 40000, output: 15000, durationMinutes: 60 },
};

// ==================== Model Catalog with Pricing ====================

export const MODEL_CATALOG = {
  // Anthropic
  'claude-opus-4-20250514': {
    provider: 'anthropic',
    name: 'Claude Opus 4',
    inputPrice: 15.00,    // per 1M tokens
    outputPrice: 75.00,
    contextWindow: 200000,
    capabilities: ['deep-reasoning', 'mathematical', 'code-generation', 'long-context'],
  },
  'claude-sonnet-4-20250514': {
    provider: 'anthropic',
    name: 'Claude Sonnet 4',
    inputPrice: 3.00,
    outputPrice: 15.00,
    contextWindow: 200000,
    capabilities: ['code-generation', 'creative', 'deep-reasoning'],
  },
  'claude-3-5-haiku-20241022': {
    provider: 'anthropic',
    name: 'Claude Haiku 3.5',
    inputPrice: 0.25,
    outputPrice: 1.25,
    contextWindow: 200000,
    capabilities: ['code-generation', 'fast', 'cheap'],
  },

  // OpenAI
  'gpt-5.2-codex': {
    provider: 'openai',
    name: 'GPT-5.2 Codex',
    inputPrice: 2.50,
    outputPrice: 10.00,
    contextWindow: 128000,
    capabilities: ['code-generation', 'structured-output', 'deep-reasoning'],
  },
  'gpt-5.1-codex-mini': {
    provider: 'openai',
    name: 'GPT-5.1 Codex Mini',
    inputPrice: 0.15,
    outputPrice: 0.60,
    contextWindow: 128000,
    capabilities: ['code-generation', 'fast', 'cheap', 'structured-output'],
  },
  'gpt-5.2': {
    provider: 'openai',
    name: 'GPT-5.2',
    inputPrice: 2.50,
    outputPrice: 10.00,
    contextWindow: 128000,
    capabilities: ['mathematical', 'deep-reasoning'],
  },

  // Google
  'gemini-2.0-flash': {
    provider: 'gemini',
    name: 'Gemini 2.0 Flash',
    inputPrice: 0.10,
    outputPrice: 0.40,
    contextWindow: 1000000,
    capabilities: ['fast', 'cheap', 'creative', 'long-context'],
  },
  'gemini-1.5-pro': {
    provider: 'gemini',
    name: 'Gemini 1.5 Pro',
    inputPrice: 1.25,
    outputPrice: 5.00,
    contextWindow: 2000000,
    capabilities: ['long-context', 'multimodal', 'code-generation'],
  },

  // OpenRouter
  'deepseek-coder': {
    provider: 'openrouter',
    name: 'DeepSeek Coder',
    inputPrice: 0.14,
    outputPrice: 0.28,
    contextWindow: 128000,
    capabilities: ['code-generation', 'cheap'],
  },

  // Local
  'llama-3.1-70b': {
    provider: 'local',
    name: 'Llama 3.1 70B',
    inputPrice: 0,
    outputPrice: 0,
    contextWindow: 128000,
    capabilities: ['code-generation', 'cheap'],
  },
  'qwen-2.5-coder': {
    provider: 'local',
    name: 'Qwen 2.5 Coder',
    inputPrice: 0,
    outputPrice: 0,
    contextWindow: 128000,
    capabilities: ['code-generation', 'cheap'],
  },
};

// ==================== Known AI CLIs ====================

export const KNOWN_CLIS = ['claude', 'opencode', 'codex', 'gemini', 'aider', 'cody'];

// ==================== Orchestrator Constants ====================

export const MAX_RETRIES = 3;
export const MAX_ITERATIONS = 10;
export const AC_TEST_TIMEOUT_MS = 30000;
export const CLI_HEALTH_TIMEOUT_MS = 3000;
export const MONITOR_POLL_MS = 500;

// ==================== Core Coding Principles ====================

export const CODING_PRINCIPLES = [
  'DRY (Don\'t Repeat Yourself) — Extract common logic into reusable functions.',
  'ETC (Easy To Change) — Write code that\'s easy to modify later.',
  'Tracer Bullets — Build end-to-end thin slices first, then iterate.',
  'Orthogonality — Keep components independent and interchangeable.',
  'Reversibility — Avoid decisions that are hard to reverse.',
  'Single Responsibility — Each module/function does one thing well.',
  'YAGNI (You Aren\'t Gonna Need It) — Don\'t build features speculatively.',
  'Fail Fast — Detect and report errors as early as possible.',
  'Small Functions — Keep functions under 20 lines when practical.',
  'Meaningful Names — Variables, functions, and files should be self-documenting.',
  'Test Coverage — Write tests for critical paths and edge cases.',
  'Security First — Never trust user input; validate at boundaries.',
];

// ==================== Event Types ====================

export const RALPH_EVENTS = {
  STORY_STARTED: 'ralph:story-started',
  STORY_COMPLETED: 'ralph:story-completed',
  STORY_FAILED: 'ralph:story-failed',
  STORY_PROGRESS: 'ralph:story-progress',
  AC_PROGRESS: 'ralph:ac-progress',
  EXECUTION_STARTED: 'ralph:execution-started',
  EXECUTION_PAUSED: 'ralph:execution-paused',
  EXECUTION_STOPPED: 'ralph:execution-stopped',
  EXECUTION_COMPLETE: 'ralph:execution-complete',
  LOG: 'ralph:log',
  CLI_DETECTED: 'ralph:cli-detected',
  PLAN_READY: 'ralph:plan-ready',
  COST_UPDATE: 'ralph:cost-update',
};

// ==================== Helper: Estimate Cost ====================

/**
 * Estimate cost for a model given token counts.
 * @param {string} modelId
 * @param {number} inputTokens
 * @param {number} outputTokens
 * @returns {number} cost in USD
 */
export function estimateCost(modelId, inputTokens, outputTokens) {
  const model = MODEL_CATALOG[modelId];
  if (!model) return 0;
  return (inputTokens / 1_000_000) * model.inputPrice +
         (outputTokens / 1_000_000) * model.outputPrice;
}

/**
 * Check if acceptance criteria are testable (have testCommand fields).
 * @param {Array} ac
 * @returns {boolean}
 */
export function isTestableAC(ac) {
  return ac.length > 0 && typeof ac[0] === 'object' && ac[0].testCommand;
}
