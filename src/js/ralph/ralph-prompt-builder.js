// ABOUTME: Prompt builder for AI agent dispatch.
// ABOUTME: Constructs prompts with coding principles, story details, and AC criteria.

import { CODING_PRINCIPLES } from './ralph-types.js';

/**
 * Build a prompt for an AI CLI agent to work on a user story.
 *
 * @param {object} story - The user story from the PRD
 * @param {object} prd - The full PRD (for project context)
 * @param {object} [options] - { customPrinciples, isRetry, failedACs, previousSessionId }
 * @returns {string} The prompt text
 */
export function buildPrompt(story, prd, options = {}) {
  const { customPrinciples, isRetry, failedACs, previousSessionId } = options;
  const lines = [];

  // Header
  lines.push(`# Task: ${story.id} — ${story.title}`);
  lines.push('');

  // Project context
  lines.push(`## Project: ${prd.project || 'Unknown'}`);
  if (prd.description) {
    lines.push(`${prd.description}`);
  }
  if (prd.branchName) {
    lines.push(`Branch: ${prd.branchName}`);
  }
  lines.push('');

  // Story details
  lines.push('## Story');
  lines.push(`**Title:** ${story.title}`);
  lines.push(`**ID:** ${story.id}`);
  lines.push(`**Complexity:** ${story.complexity || 'medium'}`);
  if (story.description) {
    lines.push(`**Description:** ${story.description}`);
  }
  lines.push('');

  // Acceptance Criteria
  lines.push('## Acceptance Criteria');
  if (Array.isArray(story.acceptanceCriteria)) {
    story.acceptanceCriteria.forEach((ac, i) => {
      if (typeof ac === 'string') {
        lines.push(`${i + 1}. ${ac}`);
      } else {
        const status = ac.passes ? '[PASS]' : '[PENDING]';
        lines.push(`${i + 1}. ${status} ${ac.text}`);
        if (ac.testCommand) {
          lines.push(`   Test: \`${ac.testCommand}\``);
        }
      }
    });
  }
  lines.push('');

  // Retry context
  if (isRetry && failedACs && failedACs.length > 0) {
    lines.push('## Retry Context');
    lines.push('Previous attempt failed. Focus on these failing acceptance criteria:');
    failedACs.forEach(ac => {
      lines.push(`- ${ac.text}`);
      if (ac.testCommand) {
        lines.push(`  Test command: \`${ac.testCommand}\``);
      }
    });
    lines.push('');
    lines.push('The passing ACs should NOT be broken. Only fix the failing ones.');
    lines.push('');
  }

  // Coding principles
  lines.push('## Coding Principles');
  CODING_PRINCIPLES.forEach(p => {
    lines.push(`- ${p}`);
  });
  lines.push('');

  // Custom principles (from config)
  if (customPrinciples) {
    lines.push('## Project-Specific Principles');
    lines.push(customPrinciples);
    lines.push('');
  }

  // Instructions
  lines.push('## Instructions');
  lines.push('1. Read and understand the codebase before making changes.');
  lines.push('2. Implement the requirements described above.');
  lines.push('3. Make sure all acceptance criteria pass.');
  lines.push('4. Run the test commands to verify your work.');
  lines.push('5. Keep changes minimal and focused on the task.');
  lines.push('');

  return lines.join('\n');
}

/**
 * Build CLI arguments for an agent invocation.
 *
 * @param {string} cli - CLI name (claude, aider, codex, etc.)
 * @param {string} promptFilePath - Path to the prompt temp file
 * @param {string} [model] - Model to use (optional)
 * @param {string} [previousSessionId] - Session ID for resume
 * @returns {string[]} args array
 */
export function buildCliArgs(cli, promptFilePath, model, previousSessionId) {
  const args = [];

  switch (cli) {
    case 'claude':
      args.push('--print', '--verbose');
      if (model) args.push('--model', model);
      if (previousSessionId) {
        args.push('--resume', previousSessionId);
      }
      // Read prompt from file via shell substitution
      args.push(`$(cat ${promptFilePath})`);
      break;

    case 'aider':
      args.push('--yes-always', '--no-auto-commits');
      if (model) args.push('--model', model);
      args.push('--message-file', promptFilePath);
      break;

    case 'codex':
      args.push('--approval-mode', 'full-auto');
      args.push('--prompt-file', promptFilePath);
      break;

    default:
      // Generic: pass prompt file as argument
      args.push(promptFilePath);
      break;
  }

  return args;
}
