// Preflight: the checks that say why NautBot will not work on THIS machine, at
// boot, before anyone waits for it to fail.
//
// Everything here exists because of a first-run condition no test could see.
// A test builds its own fixture, so the config is correct by construction; the
// conditions that actually cost days were all cases where the code was right
// and the machine was not:
//
//   - PM was disabled with an empty repo_path, so 156 consecutive sweeps did
//     nothing. Nothing said so. The fleet looked idle rather than broken.
//   - `.xnaut/verify.json` named a provider this build does not have
//     ("nautbox-verify", "local"), and verification failed one ticket at a
//     time, hours apart, looking like a flaky sandbox.
//   - Most projects have no verify config at all, so their tickets reach
//     `done` and stop. On this machine that is 18 of the 30 projects with a
//     local checkout, which is the "works out of the box" gap in one line.
//   - agents.toml kept `prompt_injection_mode = flag-prompt` for claude long
//     after the source stopped defaulting to it, so woken runs parked at a
//     composer nobody submits. The fix shipped in code and never reached the
//     file, which is the seed-once trap: correct source, wrong machine.
//
// The rule that makes this worth having: a check names the FIX, not just the
// fault. `pm_enabled: false` is a fact; "Settings > Project Management, set the
// control repo path" is an instruction. `/api/control/doctor` already reports
// the facts, so this reports what to do about them.
//
// Every judgement here is a pure function of values the caller read, so the
// interesting cases are testable without a configured machine, which is exactly
// the machine that cannot run these tests.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    /// Works, but something will behave differently than the owner expects.
    Warn,
    /// A capability is unreachable. Something will refuse every time it is asked.
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub level: Level,
    /// What is true right now, in the owner's terms.
    pub detail: String,
    /// What to do about it. Empty when level is Ok.
    pub fix: &'static str,
}

impl Check {
    fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Check {
            name,
            level: Level::Ok,
            detail: detail.into(),
            fix: "",
        }
    }
    fn warn(name: &'static str, detail: impl Into<String>, fix: &'static str) -> Self {
        Check {
            name,
            level: Level::Warn,
            detail: detail.into(),
            fix,
        }
    }
    fn fail(name: &'static str, detail: impl Into<String>, fix: &'static str) -> Self {
        Check {
            name,
            level: Level::Fail,
            detail: detail.into(),
            fix,
        }
    }
}

/// Can the sweep see a board at all?
///
/// A sweep with no board is not idle, it is blind, and it reports the same
/// thing either way. `configured_repo` already produces a precise reason
/// (disabled / not configured / bad path), so quote it rather than restate it.
pub fn board_check(repo: &Result<std::path::PathBuf, String>) -> Check {
    match repo {
        Ok(path) => Check::ok("board", format!("control repo at {}", path.display())),
        Err(why) => Check::fail(
            "board",
            why.clone(),
            "Settings > Project Management: enable it and set the control repo path. \
             Until then every sweep runs and finds nothing, which looks like an idle fleet.",
        ),
    }
}

/// What a project's verify configuration amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanState {
    /// A plan resolved: either an explicit `.xnaut/verify.json` or a Node
    /// autodetect from package.json.
    Loads,
    /// A `.xnaut/verify.json` exists and does not load. Someone tried.
    Broken(String),
    /// Neither a verify.json nor anything to autodetect from.
    Unconfigured,
}

/// Will each project's verify plan load at all?
///
/// Deliberately NOT a question about `settings.sandboxes`. That list governs
/// where agents LAUNCH; verification resolves its runner from the repo's own
/// `.xnaut/verify.json` and reaches exe.dev over this machine's registered ssh
/// key, consulting no setting at all. The first version of this check asked
/// `launch_env::survey` and would have reported FAIL on the very machine where
/// a verification had just gone green. A preflight that cries wolf is worse
/// than no preflight, so it asks the question verification actually asks.
///
/// The two states are deliberately different levels. A repo whose verify.json
/// is a typo is a mistake someone made and can fix in a minute. A repo with no
/// verify config at all is not misconfigured, it was simply never set up, and
/// on this machine that was true of 18 of 30 projects. Both stop tickets at
/// `done`, but reporting them at the same severity would bury the typo under
/// the backlog, and a preflight nobody reads is the same as no preflight.
///
/// The failure this catches, seen on the rig at 12:00:02Z on 2026-09-03:
/// `provider` was set to "nautbox-verify", which is the VM's NAME, not a
/// provider. `load_verify_plan` rejects it correctly, but only when a
/// verification runs, so it surfaced one ticket at a time, hours apart,
/// looking like a flaky sandbox rather than a typo.
pub fn verify_plans_check(plans: &[(String, PlanState)]) -> Check {
    let broken: Vec<String> = plans
        .iter()
        .filter_map(|(project, state)| match state {
            PlanState::Broken(why) => Some(format!("{project}: {why}")),
            _ => None,
        })
        .collect();
    if !broken.is_empty() {
        return Check::fail(
            "verify plans",
            broken.join("; "),
            "Fix .xnaut/verify.json in the named repo. Until then every verification for that \
             project refuses, so its tickets reach done and can never reach complete.",
        );
    }
    let bare: Vec<&str> = plans
        .iter()
        .filter(|(_, state)| matches!(state, PlanState::Unconfigured))
        .map(|(project, _)| project.as_str())
        .collect();
    if !bare.is_empty() {
        return Check::warn(
            "verify plans",
            format!(
                "{} project(s) have no way to be verified: {}",
                bare.len(),
                bare.join(", ")
            ),
            "Add .xnaut/verify.json to each repo describing install/build/test. A project \
             without one is not broken, but NautBot can never move its tickets past done, \
             because `complete` means tested and nothing can run the tests.",
        );
    }
    Check::ok(
        "verify plans",
        match plans.len() {
            0 => "no project names a local repo to verify".to_string(),
            n => format!("{n} project verify plan(s) load"),
        },
    )
}

/// Does the registry on disk still describe what this build does?
///
/// The seed-once trap: `agents.toml` is written on first run and was, for
/// three months, never revisited, so a fixed default (claude's injection mode)
/// went on being wrong on every existing install while the source read
/// correctly. Reconciliation reports each difference instead of overwriting
/// the owner's file, and those reports are what this surfaces.
/// The revision is in the detail on BOTH branches, and that is the point of
/// XNAUT-278. "Which registry is this machine on" had no answer at all: the
/// file said nothing about which build had reconciled it, so working out why
/// two installs behaved differently meant reading dates off a TOML file. A
/// clean registry that reports its revision is how the next drift gets caught
/// by comparing two machines instead of excavating one.
pub fn registry_check(notes: &[(String, String)], path: &str, seed_revision: u32) -> Check {
    if notes.is_empty() {
        return Check::ok(
            "agent registry",
            format!("{path} matches this build (revision {seed_revision})"),
        );
    }
    let listed: Vec<String> = notes
        .iter()
        .map(|(agent, message)| format!("{agent}: {message}"))
        .collect();
    Check::warn(
        "agent registry",
        format!(
            "revision {seed_revision}: {} field(s) differ from this build's defaults ({})",
            notes.len(),
            listed.join("; ")
        ),
        "Your values are kept, not overwritten. If a runtime misbehaves, compare it against \
         the defaults in the runtime picker, or reset just that one with agent_registry_rollback; \
         the registry it replaces is kept beside it as a .bak.",
    )
}

/// Read the machine and judge it. The only function here that does IO.
pub fn run() -> Vec<Check> {
    let settings = crate::settings::load_or_default();

    let repo = crate::project_management::configured_repo(&settings.project_management);
    let board = board_check(&repo);

    // Only projects that name a repo which exists: a project with no
    // `source_repo` is not misconfigured, it just has nothing to verify.
    // `source_repo` holds a local checkout for some projects and a git remote
    // URL for others (XNAUT's is `ssh://git@cosmos…`), so is_dir is what tells
    // a checkout we can inspect from a remote we cannot.
    let plans: Vec<(String, PlanState)> = repo
        .as_ref()
        .ok()
        .and_then(|control| crate::project_management::list_projects(control).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|project| !project.source_repo.trim().is_empty())
        .map(|project| (project.key, std::path::PathBuf::from(project.source_repo)))
        .filter(|(_, dir)| dir.is_dir())
        .map(|(key, dir)| {
            let state = match crate::sandbox_verify::load_verify_plan(&dir) {
                Ok(_) => PlanState::Loads,
                // An explicit config that will not load is a mistake to fix;
                // no config at all is a project nobody has set up yet.
                Err(why) if dir.join(".xnaut").join("verify.json").is_file() => {
                    PlanState::Broken(why)
                }
                Err(_) => PlanState::Unconfigured,
            };
            (key, state)
        })
        .collect();
    let verification = verify_plans_check(&plans);

    let registry = match crate::agents::load_registry() {
        Ok(loaded) => registry_check(
            &loaded
                .notes
                .into_iter()
                .map(|note| (note.agent_id, note.message))
                .collect::<Vec<_>>(),
            &crate::agents::registry_path_display(),
            loaded.registry.seed_revision,
        ),
        Err(why) => Check::fail(
            "agent registry",
            why,
            "Fix the file by hand. xNAUT will not rewrite a registry it cannot parse, \
             so every agent launch uses built-in defaults until it reads again.",
        ),
    };

    vec![board, verification, registry]
}

/// Run at boot: log every finding and record the bad ones where they survive a
/// restart. A check nobody reads is the same as no check, and the audit log is
/// the one surface that is still there tomorrow when the owner asks why a
/// ticket never moved.
pub fn run_at_boot() {
    let checks = run();
    for check in &checks {
        match check.level {
            Level::Ok => eprintln!("[preflight] ok {}: {}", check.name, check.detail),
            Level::Warn | Level::Fail => eprintln!(
                "[preflight] {} {}: {}. Fix: {}",
                if check.level == Level::Fail {
                    "FAIL"
                } else {
                    "warn"
                },
                check.name,
                check.detail,
                check.fix
            ),
        }
    }
    let bad: Vec<&Check> = checks.iter().filter(|c| c.level != Level::Ok).collect();
    if bad.is_empty() {
        return;
    }
    crate::audit::record(
        "preflight.failed",
        &format!(
            "{} precondition(s) will stop work: {}",
            bad.len(),
            bad.iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
        ),
        serde_json::json!({ "checks": bad }),
    );
}

/// The same checks, for the settings pane and for `/api/control/doctor`.
#[tauri::command]
pub fn preflight_checks() -> Vec<Check> {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blind_sweep_is_a_failure_and_says_where_to_fix_it() {
        // 156 consecutive sweeps did nothing because repo_path was empty, and
        // every surface reported a healthy idle fleet. The point of this check
        // is that the report is actionable, so assert on the instruction too.
        let check = board_check(&Err("Project Management module is disabled".into()));
        assert_eq!(check.level, Level::Fail);
        assert!(check.detail.contains("disabled"));
        assert!(
            check.fix.contains("Project Management"),
            "the fix must name where to go"
        );

        let ok = board_check(&Ok(std::path::PathBuf::from("/tmp/control")));
        assert_eq!(ok.level, Level::Ok);
        assert!(ok.fix.is_empty(), "a passing check must not suggest a fix");
    }

    #[test]
    fn a_broken_verify_plan_is_named_with_its_project() {
        // The rig, 12:00:02Z on 2026-09-03: provider was "nautbox-verify",
        // which is the VM's name rather than a provider. It surfaced one
        // ticket at a time, hours apart. Boot is where it should have said so,
        // and the report is useless unless it names WHICH project to go and fix.
        let plans = vec![
            ("XNAUT".to_string(), PlanState::Loads),
            (
                "RIG".to_string(),
                PlanState::Broken("unknown verify provider \"nautbox-verify\"".to_string()),
            ),
        ];
        let check = verify_plans_check(&plans);
        assert_eq!(check.level, Level::Fail);
        assert!(
            check.detail.contains("RIG"),
            "the broken project must be named"
        );
        assert!(
            check.detail.contains("nautbox-verify"),
            "quote the reason, do not restate it"
        );
        assert!(
            !check.detail.contains("XNAUT"),
            "a working project is not a finding"
        );
        assert!(
            check.fix.contains("complete"),
            "name the consequence: done never reaches complete"
        );
    }

    #[test]
    fn a_typo_outranks_a_backlog_of_unconfigured_projects() {
        // This machine has 18 projects with no verify config, so if both
        // states reported at one level the single fixable typo would arrive
        // buried in a list of eighteen. The typo is the finding; the backlog
        // is context.
        let plans = vec![
            ("A".to_string(), PlanState::Unconfigured),
            ("B".to_string(), PlanState::Unconfigured),
            (
                "RIG".to_string(),
                PlanState::Broken("bad provider".to_string()),
            ),
        ];
        let check = verify_plans_check(&plans);
        assert_eq!(check.level, Level::Fail);
        assert!(check.detail.contains("RIG"));
        assert!(
            !check.detail.contains('A'),
            "the backlog must not crowd out the typo"
        );
    }

    #[test]
    fn a_project_nobody_set_up_is_a_warning_not_a_failure() {
        let check = verify_plans_check(&[
            ("A".to_string(), PlanState::Unconfigured),
            ("B".to_string(), PlanState::Loads),
        ]);
        assert_eq!(
            check.level,
            Level::Warn,
            "never set up is not the same as broken"
        );
        assert!(check.detail.contains('A'));
        assert!(
            check.fix.contains("done"),
            "say where the tickets will stall"
        );
    }

    #[test]
    fn no_project_naming_a_local_repo_is_not_a_failure() {
        // A fresh install has no projects, and XNAUT's own source_repo is a
        // git URL rather than a checkout. Neither is misconfiguration, and
        // reporting them as such is how a preflight teaches people to ignore it.
        assert_eq!(verify_plans_check(&[]).level, Level::Ok);
        let one_good = verify_plans_check(&[("XNAUT".to_string(), PlanState::Loads)]);
        assert_eq!(one_good.level, Level::Ok);
        assert!(one_good.detail.contains('1'));
    }

    #[test]
    fn a_registry_that_disagrees_with_the_build_is_reported_not_silent() {
        // XNAUT-182: agents.toml kept prompt_injection_mode = flag-prompt for
        // three months after the source stopped defaulting to it. The file was
        // right at seed time and wrong ever after, and nothing said so.
        let notes = vec![(
            "claude".to_string(),
            "prompt_injection_mode: yours FlagPrompt, this build Argv".to_string(),
        )];
        let check = registry_check(&notes, "/tmp/agents.toml", 1);
        assert_eq!(
            check.level,
            Level::Warn,
            "the owner's edits are kept, so this is not a failure"
        );
        assert!(check.detail.contains("prompt_injection_mode"));

        let clean = registry_check(&[], "/tmp/agents.toml", 1);
        assert_eq!(clean.level, Level::Ok);
        assert!(
            clean.detail.contains("/tmp/agents.toml"),
            "say which file was checked"
        );
    }

    #[test]
    fn the_registry_version_is_reported_whether_or_not_it_drifted() {
        // XNAUT-278: "which registry is this machine on" needs one answer, and
        // it has to be there on the healthy machine too. A revision reported
        // only when something is already wrong cannot be used to compare a
        // working install against a broken one, which is the whole use.
        let clean = registry_check(&[], "/tmp/agents.toml", 7);
        assert!(
            clean.detail.contains('7'),
            "a clean registry still has to say which revision it is on: {}",
            clean.detail
        );
        let drifted = registry_check(&[("claude".into(), "kept your env".into())], "/tmp/x", 7);
        assert!(
            drifted.detail.contains('7'),
            "a drifted registry has to say which revision drifted: {}",
            drifted.detail
        );
    }

    /// The IO path, against this real machine. Ignored by default for two
    /// reasons: it reads the owner's actual settings, and it mutates a process
    /// -global env var, which is how an earlier fix in this sprint broke a
    /// parallel test. Run it deliberately:
    ///
    ///   cargo test --bin xnaut -- --ignored --nocapture reads_this_machine
    ///
    /// The redirect matters. `load_registry` HEALS a pre-revision file by
    /// rewriting it, so without XNAUT_AGENTS_PATH this test would edit the
    /// owner's agents.toml as a side effect of reporting on it.
    #[test]
    #[ignore]
    fn reads_this_machine_without_touching_it() {
        let temp = std::env::temp_dir().join(format!("preflight-{}.toml", uuid::Uuid::new_v4()));
        let real = dirs::config_dir()
            .map(|p| p.join("xnaut").join("agents.toml"))
            .expect("a config dir");
        if real.is_file() {
            std::fs::copy(&real, &temp).expect("copy the registry aside");
        }
        std::env::set_var("XNAUT_AGENTS_PATH", &temp);
        let checks = run();
        std::env::remove_var("XNAUT_AGENTS_PATH");
        let _ = std::fs::remove_file(&temp);

        for check in &checks {
            println!("{:?} {}: {}", check.level, check.name, check.detail);
            if !check.fix.is_empty() {
                println!("      fix: {}", check.fix);
            }
        }
        assert_eq!(checks.len(), 3, "board, verify plans, agent registry");
    }

    #[test]
    fn every_finding_carries_an_instruction() {
        // The one rule that separates this from doctor: a fault without a fix
        // is a fact, and facts are what we already had too many of.
        let faults = [
            board_check(&Err("nope".into())),
            verify_plans_check(&[("P".into(), PlanState::Broken("bad".into()))]),
            verify_plans_check(&[("P".into(), PlanState::Unconfigured)]),
            registry_check(&[("a".into(), "b".into())], "/tmp/x", 1),
        ];
        for check in faults {
            assert_ne!(check.level, Level::Ok);
            assert!(
                !check.fix.is_empty(),
                "{} reports a fault with no fix",
                check.name
            );
        }
    }
}
