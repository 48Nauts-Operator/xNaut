// Required behavior evidence, shared by PR checks and release builds.
// Native tests exercise real persistence/admission with fixture launch backends;
// browser tests exercise the UI/IPC contract. Neither claims a live cloud run.
import { spawnSync } from 'node:child_process';
import { closeSync, mkdirSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const nativeSuites = [
  'project_wiki::tests::enumerated_document_paths_round_trip_without_weakening_path_guards',
  'project_wiki::journal::',
  'cloud_model::tests::',
  'handback::tests::',
  'agent_profiles::compute_choice_tests::remote_',
  'repository_transfer::tests::repository_admission_',
  ...(process.platform === 'win32' ? [] : ['repository_transfer::tests::repository_publisher_protocol_suite']),
  'swarm_plan::tests::', 'dispatch::tests::', 'instance::tests::',
  'sandbox::launch_env::tests::', 'agent_tools::tests::swarm_tools_',
  'project_management::mutation_recovery::tests::', 'project_management::write_guard_tests::',
  'project_continuity::tests::', 'agent_work::tests::', 'repository_review::tests::',
  'run_control::tests::', 'settings::tests::', 'ticket_triage::tests::',
  'gitops::tests::',
  'durable_turn::tests::', 'agent_tools::tests::durable_',
];
export const requiredNative = [
  'project_wiki::tests::enumerated_document_paths_round_trip_without_weakening_path_guards',
  ...(process.platform === 'win32' ? [] : ['repository_transfer::tests::repository_publisher_protocol_suite']),
  'project_wiki::journal::tests::journal_capture_waits_for_an_active_wiki_writer_without_losing_an_entry',
  'project_wiki::journal::console::tests::journal_actions_scope_before_limit_and_keep_old_dates',
  'handback::tests::command_result_verification_evidence_preserves_the_pi_handback_contract',
  'run_control::tests::explicit_conversations_do_not_consume_legacy_worker_capacity',
  'cloud_model::tests::shared_cloud_choice_is_identical_across_destinations_and_preserves_local_profiles',
  'cloud_model::tests::cloud_launch_overrides_worker_defaults_without_putting_credentials_in_argv',
  ...(process.platform === 'win32' ? [] : ['cloud_model::tests::worker_model_protocol_suite']),
  'agent_profiles::compute_choice_tests::remote_prompt_modes_refuse_undeliverable_tasks_and_preserve_explicit_carriers',
  ...(process.platform === 'win32' ? [] : [
    'agent_profiles::compute_choice_tests::remote_pi_process_receives_the_exact_task_model_provider_env_and_identity',
    'agent_profiles::compute_choice_tests::remote_runtime_probe_checks_the_actual_command_and_pi_auth_without_leaking_output',
  ]),
  'swarm_plan::tests::runtime_outage_preserves_five_continuations_until_worker_readiness_recovers',
  'swarm_plan::tests::completed_continuation_tracks_its_successor_instead_of_lexically_later_failed_history',
  'repository_transfer::tests::repository_admission_classifies_git_failures_without_leaking_credentials',
  'repository_transfer::tests::repository_admission_real_git_reports_desktop_stage_and_missing_repository',
  'swarm_plan::tests::replanning_keeps_approved_remote_destination_despite_local_preview_or_profile',
  'swarm_plan::tests::repository_outage_blocks_five_members_without_run_attempts_and_recovers_continuations',
  'swarm_plan::tests::group_requeues_only_proven_prelaunch_lineage_and_bounds_repeated_failures',
  'durable_turn::tests::process_kill_at_every_effect_boundary_preserves_results_and_prevents_duplicate_writes',
  'durable_turn::tests::changed_provider_repository_and_tools_fail_before_replay',
  'durable_turn::tests::failed_result_commit_leaves_recoverable_uncertainty',
  'agent_tools::tests::durable_model_tool_loop_resumes_with_committed_results_and_no_repeated_read',
  'gitops::tests::ticket_history_queries_leave_the_ui_executor_free_and_bound_parallel_work',
  'swarm_plan::tests::overlapping_approvals_recover_under_latest_group_and_keep_its_dispatched_scope',
  'swarm_plan::tests::overlapping_approvals_respect_newest_stop_and_unapproved_partial_plans',
  'run_control::tests::admission_refusal_proof_requires_native_refusal_and_no_execution_evidence',
  'swarm_plan::tests::refused_continuations_recover_five_retained_runs_without_duplicates_or_destination_changes',
  'swarm_plan::tests::refused_continuations_still_obey_scope_profile_read_only_and_stop',
  'swarm_plan::tests::refused_continuation_conflicts_are_blocked_with_the_recovery_reason',
  'instance::tests::workstation_coordinates_remote_results_without_local_or_unattended_work',
  'swarm_plan::tests::workstation_remote_swarm_approval_restart_refill_and_duplicate_confirmation',
  'swarm_plan::tests::workstation_swarm_rejects_local_mixed_missing_and_unknown_destinations_before_approval',
  'swarm_plan::tests::swarm_read_only_sandbox_and_stop_preserve_queued_work_without_launching',
  'swarm_plan::tests::swarm_changed_ticket_scope_blocks_only_affected_member',
  'swarm_plan::tests::swarm_model_confirmation_failure_cannot_partially_approve_or_launch',
  'swarm_plan::tests::explicit_swarm_destination_survives_storage_and_overrides_local_profile',
  'swarm_plan::tests::five_queued_members_are_reported_as_five_and_remain_approved',
  'swarm_plan::tests::persisted_approval_pins_repository_runtime_and_resolved_environment',
  'swarm_plan::tests::refill_keeps_shared_native_registry_corruption_fail_closed',
  'swarm_plan::tests::preview_persists_during_sweep_without_granting_dispatch',
  'swarm_plan::tests::concurrent_previews_cannot_replace_an_approval',
  'agent_tools::tests::swarm_tools_expose_destination_and_durable_queue_contract',
];
export const browserFiles = [
  'journal-console.spec.mjs', 'actions-rows.spec.mjs',
  'cloud-agent-model.spec.mjs',
  'update-banner.spec.mjs',
  'durable-agent-turns.spec.mjs',
  'nautbot-swarm.spec.mjs', 'dispatch-from-ticket.spec.mjs', 'approval-inbox.spec.mjs',
  'console-clean.spec.mjs', 'settings-hidden-lifecycle.spec.mjs',
  'observatory-sessions.spec.mjs', 'sidebar-sessions.spec.mjs',
  'sessions-band.spec.mjs', 'ticket-links.spec.mjs',
  'workspace-project-context.spec.mjs', 'vault-related-tickets.spec.mjs', 'vault-layout.spec.mjs', 'project-journal.spec.mjs',
];

export function checkNative(output, suites = nativeSuites, required = requiredNative) {
  const passed = [...output.matchAll(/^test (\S+) \.\.\. ok\s*$/gm)].map(match => match[1]);
  if (!passed.length || /^test \S+ \.\.\. (FAILED|ignored)/m.test(output)) {
    throw new Error('Native behavior evidence is empty, failed or skipped');
  }
  for (const suite of suites) {
    if (!passed.some(name => name.startsWith(suite))) throw new Error(`Missing native suite: ${suite}`);
  }
  for (const name of required) {
    if (!passed.includes(name)) throw new Error(`Missing required behavior: ${name}`);
  }
  return passed;
}

export function checkBrowser(report, files = browserFiles) {
  if (report.errors?.length) throw new Error('Browser runner reported errors');
  const passed = [];
  function visit(suites) {
    for (const suite of suites || []) {
      for (const spec of suite.specs || []) {
        if (!spec.tests?.length) throw new Error(`Browser behavior has no execution: ${spec.title}`);
        for (const test of spec.tests) {
          if (test.status !== 'expected' || test.expectedStatus !== 'passed'
              || !test.results?.length || test.results.some(r => r.status !== 'passed')) {
            throw new Error(`Browser behavior failed, skipped or retried: ${spec.title}`);
          }
          passed.push({ file: spec.file || suite.file, title: spec.title });
        }
      }
      visit(suite.suites);
    }
  }
  visit(report.suites);
  if (!passed.length) throw new Error('Browser behavior evidence is empty');
  for (const file of files) {
    if (!passed.some(test => test.file?.replaceAll('\\', '/').endsWith(file))) {
      throw new Error(`Missing browser suite: ${file}`);
    }
  }
  return passed;
}

export function checkReceipt(receipt, sourceCommit) {
  if (receipt.status !== 'passed' || receipt.scope !== 'native-and-browser'
      || receipt.source_commit !== sourceCommit || receipt.working_tree_changes !== '') {
    throw new Error('Release behavior evidence must pass both layers on the exact clean release commit');
  }
  const start = Date.parse(receipt.started_at), finish = Date.parse(receipt.finished_at);
  if (!Number.isFinite(start) || !Number.isFinite(finish) || finish < start || finish > Date.now()) {
    throw new Error('Release behavior evidence has invalid completion times');
  }
  checkNative((receipt.native || []).map(name => `test ${name} ... ok`).join('\n'));
  for (const file of browserFiles) {
    if (!receipt.browser?.some(test => test.file?.replaceAll('\\', '/').endsWith(file))) {
      throw new Error(`Missing browser evidence: ${file}`);
    }
  }
}

function main() {
  const args = process.argv.slice(2);
  if (args[0] === '--verify') {
    if (args.length !== 3) throw new Error('Usage: node scripts/release-behavior.mjs --verify <result.json> <release-sha>');
    checkReceipt(JSON.parse(readFileSync(args[1], 'utf8')), args[2]);
    console.log(`Release behavior evidence verified for ${args[2]}.`);
    return;
  }
  if (args.some(arg => arg !== '--native-only')) throw new Error('Usage: node scripts/release-behavior.mjs [--native-only]');
  const nativeOnly = args.includes('--native-only');
  const directory = resolve('artifacts/release-behavior');
  mkdirSync(directory, { recursive: true });
  const git = (...args) => {
    const result = spawnSync('git', args, { encoding: 'utf8' });
    if (result.status !== 0) throw new Error('Cannot establish candidate source identity');
    return result.stdout.trim();
  };
  const receipt = {
    source_commit: git('rev-parse', 'HEAD'), working_tree_changes: git('status', '--porcelain'),
    started_at: new Date().toISOString(), scope: nativeOnly ? 'native-only' : 'native-and-browser',
    status: 'running', native: [], browser: [],
  };
  const save = () => writeFileSync(resolve(directory, 'result.json'), JSON.stringify(receipt, null, 2));
  function run(name, command, args, env = process.env) {
    console.log(`Checking ${name}…`);
    const stdout = resolve(directory, `${name}.stdout.log`);
    const out = openSync(stdout, 'w');
    const err = openSync(resolve(directory, `${name}.stderr.log`), 'w');
    let result;
    try {
      result = spawnSync(command, args, { env, timeout: 30 * 60_000, stdio: ['ignore', out, err] });
    } finally { closeSync(out); closeSync(err); }
    if (result.error || result.status !== 0) {
      throw new Error(`${name} unavailable or failed: ${result.error?.message || `exit ${result.status}`}; see ${directory}`);
    }
    return readFileSync(stdout, 'utf8');
  }
  save();
  try {
    run('evidence-checks', process.execPath, ['--test', 'tests/release-behavior.test.mjs']);
    if (process.platform !== 'win32') run('manual-gate-checks', 'bash', ['tests/release-gate.test.sh']);
    receipt.native = checkNative(run('native', 'cargo', [
      'test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--bin', 'xnaut',
      '--', '--test-threads=1', '--color=never', ...nativeSuites,
    ]));
    console.log(`${receipt.native.length} native behavior tests passed.`);
    if (!nativeOnly) {
      receipt.browser = checkBrowser(JSON.parse(run('browser', process.execPath, [
        'node_modules/@playwright/test/cli.js', 'test', '--config', 'playwright.release.config.mjs',
        '--workers=1', '--retries=0', '--reporter=json', ...browserFiles.map(file => `tests/${file}`),
      ], { ...process.env, XNAUT_TEST_PORT: process.env.XNAUT_TEST_PORT || '4297' })));
      console.log(`${receipt.browser.length} browser behavior tests passed.`);
    }
    receipt.status = nativeOnly ? 'partial' : 'passed';
    console.log(nativeOnly ? 'Native evidence only; this does not pass the release gate.' : 'Release behavior gate passed.');
  } catch (error) {
    receipt.status = 'failed';
    receipt.error = error.message;
    throw error;
  } finally {
    receipt.finished_at = new Date().toISOString();
    save();
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
