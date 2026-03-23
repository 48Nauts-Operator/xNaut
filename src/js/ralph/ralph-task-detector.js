// ABOUTME: Task type detection via weighted keyword scoring.
// ABOUTME: Analyzes story title (3x weight), description, and AC text to classify into 14 task types.

/**
 * Keywords mapped to task types for detection.
 */
const TASK_KEYWORDS = {
  'complex-integration': [
    'integration', 'multi-system', 'architecture', 'orchestration',
    'microservice', 'end-to-end', 'full-stack', 'cross-cutting',
  ],
  'mathematical': [
    'algorithm', 'calculation', 'formula', 'optimization',
    'compute', 'math', 'statistics', 'probability',
  ],
  'backend-api': [
    'endpoint', 'rest', 'graphql', 'api', 'route',
    'controller', 'request', 'response', 'http',
  ],
  'backend-logic': [
    'service', 'business logic', 'validation', 'processing',
    'workflow', 'domain logic', 'data processing',
  ],
  'frontend-ui': [
    'component', 'ui', 'style', 'css', 'layout', 'design',
    'visual', 'responsive', 'theme', 'button', 'form', 'modal', 'dashboard',
  ],
  'frontend-logic': [
    'hook', 'state', 'context', 'reducer', 'effect',
    'react', 'vue', 'store', 'state management',
  ],
  'database': [
    'schema', 'migration', 'query', 'database', 'sql',
    'table', 'index', 'relation', 'model',
  ],
  'testing': [
    'test', 'spec', 'mock', 'jest', 'vitest',
    'cypress', 'e2e', 'unit test', 'integration test', 'coverage',
  ],
  'documentation': [
    'documentation', 'readme', 'docs', 'guide',
    'tutorial', 'comment', 'jsdoc', 'api docs',
  ],
  'refactoring': [
    'refactor', 'cleanup', 'reorganize', 'restructure',
    'simplify', 'optimize', 'improve',
  ],
  'bugfix': [
    'fix', 'bug', 'issue', 'error', 'crash',
    'defect', 'problem', 'broken',
  ],
  'devops': [
    'docker', 'ci/cd', 'pipeline', 'deploy', 'deployment',
    'kubernetes', 'container', 'build',
  ],
  'config': [
    'configuration', 'config', 'setup', 'environment',
    'settings', 'env', 'dotenv',
  ],
};

/**
 * Detect the task type from a user story by keyword scoring.
 * Title matches are weighted 3x higher than description/AC matches.
 *
 * @param {object} story - { title, description, acceptanceCriteria }
 * @returns {string} detected task type
 */
export function detectTaskType(story) {
  const title = story.title.toLowerCase();
  const description = (story.description || '').toLowerCase();

  const acText = Array.isArray(story.acceptanceCriteria)
    ? story.acceptanceCriteria
        .map(ac => (typeof ac === 'string' ? ac : ac.text || ''))
        .join(' ')
        .toLowerCase()
    : '';

  const combinedText = `${title} ${description} ${acText}`;

  let maxScore = 0;
  let detectedType = 'unknown';

  for (const [taskType, keywords] of Object.entries(TASK_KEYWORDS)) {
    let score = 0;

    for (const keyword of keywords) {
      const escaped = keyword.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const regex = new RegExp(`\\b${escaped}\\b`, 'gi');
      const matches = combinedText.match(regex);

      if (matches) {
        if (title.includes(keyword.toLowerCase())) {
          score += 3 * matches.length; // Title match weighted 3x
        } else {
          score += matches.length;
        }
      }
    }

    if (score > maxScore) {
      maxScore = score;
      detectedType = taskType;
    }
  }

  return detectedType;
}
