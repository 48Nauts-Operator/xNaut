// ABOUTME: Model capability matrix and task-to-model recommendation engine.
// ABOUTME: Maps 14 task types to primary/fallback models across 3 execution modes with quota-aware fallback.

import { MODEL_CATALOG } from './ralph-types.js';

// ==================== Model Capabilities ====================

const MODEL_CAPABILITIES = {
  'claude-opus-4-20250514':    ['deep-reasoning', 'mathematical', 'code-generation', 'long-context'],
  'claude-sonnet-4-20250514':  ['code-generation', 'creative', 'deep-reasoning'],
  'claude-3-5-haiku-20241022': ['code-generation', 'fast', 'cheap'],
  'gpt-5.2-codex':            ['code-generation', 'structured-output', 'deep-reasoning'],
  'gpt-5.1-codex-mini':       ['code-generation', 'fast', 'cheap', 'structured-output'],
  'gpt-5.2':                  ['mathematical', 'deep-reasoning'],
  'gemini-2.0-flash':         ['fast', 'cheap', 'creative', 'long-context'],
  'gemini-1.5-pro':           ['long-context', 'multimodal', 'code-generation'],
  'deepseek-coder':           ['code-generation', 'cheap'],
  'llama-3.1-70b':            ['code-generation', 'cheap'],
  'qwen-2.5-coder':           ['code-generation', 'cheap'],
};

// ==================== Balanced Mode Mapping ====================

const BALANCED_MAPPING = {
  'complex-integration': { primary: { modelId: 'claude-opus-4-20250514', provider: 'anthropic' },    fallback: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' } },
  'mathematical':        { primary: { modelId: 'gpt-5.2', provider: 'openai' },                      fallback: { modelId: 'claude-opus-4-20250514', provider: 'anthropic' } },
  'backend-api':         { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'backend-logic':       { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'frontend-ui':         { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gemini-2.0-flash', provider: 'gemini' } },
  'frontend-logic':      { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'database':            { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'testing':             { primary: { modelId: 'gpt-5.2-codex', provider: 'openai' },                 fallback: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' } },
  'documentation':       { primary: { modelId: 'gemini-2.0-flash', provider: 'gemini' },              fallback: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' } },
  'refactoring':         { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'bugfix':              { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'devops':              { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'config':              { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'unknown':             { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
};

// ==================== Super Saver Mode Mapping ====================

const SUPER_SAVER_MAPPING = {
  'complex-integration': { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'mathematical':        { primary: { modelId: 'gpt-5.2', provider: 'openai' },                      fallback: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' } },
  'backend-api':         { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'backend-logic':       { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'frontend-ui':         { primary: { modelId: 'gemini-2.0-flash', provider: 'gemini' },              fallback: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' } },
  'frontend-logic':      { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'database':            { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'testing':             { primary: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' },            fallback: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' } },
  'documentation':       { primary: { modelId: 'gemini-2.0-flash', provider: 'gemini' },              fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'refactoring':         { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'bugfix':              { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'devops':              { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'config':              { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
  'unknown':             { primary: { modelId: 'claude-3-5-haiku-20241022', provider: 'anthropic' },  fallback: { modelId: 'gpt-5.1-codex-mini', provider: 'openai' } },
};

// ==================== Fast Delivery Mode Mapping ====================

const FAST_DELIVERY_MAPPING = {
  'complex-integration': { primary: { modelId: 'claude-opus-4-20250514', provider: 'anthropic' },    fallback: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' } },
  'mathematical':        { primary: { modelId: 'gpt-5.2', provider: 'openai' },                      fallback: { modelId: 'claude-opus-4-20250514', provider: 'anthropic' } },
  'backend-api':         { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'backend-logic':       { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'frontend-ui':         { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'frontend-logic':      { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'database':            { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'testing':             { primary: { modelId: 'gpt-5.2-codex', provider: 'openai' },                 fallback: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' } },
  'documentation':       { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'refactoring':         { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'bugfix':              { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'devops':              { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'config':              { primary: { modelId: 'claude-sonnet-4-20250514', provider: 'anthropic' },   fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
  'unknown':             { primary: { modelId: 'claude-opus-4-20250514', provider: 'anthropic' },     fallback: { modelId: 'gpt-5.2-codex', provider: 'openai' } },
};

const MODE_MAPPINGS = {
  'balanced': BALANCED_MAPPING,
  'super-saver': SUPER_SAVER_MAPPING,
  'fast-delivery': FAST_DELIVERY_MAPPING,
};

// ==================== Recommendation Logic ====================

/**
 * Get recommended model for a task type based on execution mode and available quotas.
 *
 * @param {string} taskType
 * @param {object} [quotas] - { anthropic: { status }, openai: { status }, ... }
 * @param {string} [mode='balanced']
 * @returns {{ modelId: string, provider: string, reason: string }}
 */
export function getRecommendedModel(taskType, quotas, mode = 'balanced') {
  const mapping = (MODE_MAPPINGS[mode] || BALANCED_MAPPING)[taskType] || BALANCED_MAPPING['unknown'];

  // No quotas? Return primary
  if (!quotas) {
    return {
      modelId: mapping.primary.modelId,
      provider: mapping.primary.provider,
      reason: 'Primary model for task type',
    };
  }

  // Check primary provider quota
  const primaryQuota = quotas[mapping.primary.provider];
  if (primaryQuota && isQuotaAvailable(primaryQuota)) {
    return {
      modelId: mapping.primary.modelId,
      provider: mapping.primary.provider,
      reason: 'Primary model with available quota',
    };
  }

  // Check fallback provider quota
  const fallbackQuota = quotas[mapping.fallback.provider];
  if (fallbackQuota && isQuotaAvailable(fallbackQuota)) {
    return {
      modelId: mapping.fallback.modelId,
      provider: mapping.fallback.provider,
      reason: 'Fallback model (primary quota exhausted)',
    };
  }

  // Try any available provider with matching capabilities
  const requiredCaps = MODEL_CAPABILITIES[mapping.primary.modelId] || ['code-generation'];
  for (const [provider, quota] of Object.entries(quotas)) {
    if (isQuotaAvailable(quota)) {
      const model = findModelWithCapabilities(provider, requiredCaps);
      if (model) {
        return { modelId: model, provider, reason: `Alternative (both primary and fallback exhausted)` };
      }
    }
  }

  // Last resort: return primary with warning
  return {
    modelId: mapping.primary.modelId,
    provider: mapping.primary.provider,
    reason: 'Primary model (warning: all quotas may be exhausted)',
  };
}

function isQuotaAvailable(quota) {
  return quota.status === 'available' || quota.status === 'limited';
}

function findModelWithCapabilities(provider, requiredCaps) {
  for (const [modelId, caps] of Object.entries(MODEL_CAPABILITIES)) {
    const info = MODEL_CATALOG[modelId];
    if (info && info.provider === provider) {
      const hasAll = requiredCaps.every(c => caps.includes(c));
      if (hasAll) return modelId;
    }
  }
  // Fallback: any model from this provider
  for (const [modelId, info] of Object.entries(MODEL_CATALOG)) {
    if (info.provider === provider) return modelId;
  }
  return null;
}
