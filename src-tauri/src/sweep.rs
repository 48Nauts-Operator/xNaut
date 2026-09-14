// The durable sweep (XNAUT-239): the board gets worked whether or not anyone
// is talking to NautBot.
//
// This is the keystone the whole "Close the Loops" sprint pointed at. Every
// other loop in the app already had its forward edge — dispatch, run, record,
// verify — and every one of them ended at a human turn: NautBot only acted
// inside a chat message, so "work the board" died with the conversation. The
// sweep is that missing clock.
//
// What it does, once per interval:
//
//   1. Reads the ticket board (the same PM repo everything else uses).
//   2. HANDBACKS FIRST: a ticket sitting in done/review owned by NautBot is
//      work someone finished and nobody looked at. It gets a verification.
//   3. Then dispatch: ready tickets with an owner get their owner woken, one
//      per tick, oldest first.
//   4. Every action lands in the ledger, so the Agent timeline shows what the
//      sweep did while nobody was watching.
//
// What it deliberately does NOT do:
//
//   - It never sets `complete`. That is NautBot's word after review, and a
//     sweep that marks its own homework is exactly the failure the rails
//     exist to prevent.
//   - It never launches more than the spend ceiling admits: it calls the same
//     nudge path the chat tool uses, so the concurrent cap, the daily cap and
//     the read_only kill-switch all apply unchanged.
//   - It does nothing at all when the read_only switch is engaged, and says so
//     once rather than every tick.
//
// FLEET (XNAUT-265). Everything above was built and proven one piece at a time
// and had never once run together: more than one agent, working more than one
// ticket, to completion, with nobody driving each step. Reading the tick for
// what stopped that found one line rather than a missing feature.
//
// `tick` used to `return Ok(())` the moment it started a verification. That was
// written as "only one verify per tick, starting a verify is expensive", and it
// reads that way, but it is not what it does: it also skips the DISPATCH half
// entirely. So a board with any unreviewed ticket on it woke no agent at all,
// ever, and a fleet could not begin. The bound that was supposed to be about
// verification cost silently became a rule that unreviewed work starves new
// work. See `MAX_VERIFIES_IN_FLIGHT` for what replaces it and why that is the
// question a ceiling actually asks.

use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

/// Tickets whose verification died with a previous app, filled at startup by
/// `reap_orphaned_runs` and drained before any new work (XNAUT-264). The
/// safety net: a crash costs a restart, not a lost verification.
static RETRY_QUEUE: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Hand the sweep the tickets a dead app left mid-verification.
pub fn queue_retries(tickets: Vec<String>) {
    if tickets.is_empty() {
        return;
    }
    if let Ok(mut queue) = RETRY_QUEUE.lock() {
        for ticket in tickets {
            if !queue.contains(&ticket) {
                queue.push(ticket);
            }
        }
    }
}

fn next_retry() -> Option<String> {
    RETRY_QUEUE.lock().ok()?.pop()
}

/// How often the board is read. Long enough that a busy fleet is not
/// re-examined constantly, short enough that a handback is picked up while the
/// owner is still awake.
const TICK: Duration = Duration::from_secs(180);

/// Conditions the sweep has already reported, so a standing state is said once
/// rather than every tick. Both reset when the condition clears, so the next
/// occurrence is announced again.
#[derive(Default)]
struct Announced {
    read_only: bool,
    no_repo: bool,
    /// Tickets whose give-up has already been recorded.
    gave_up: std::collections::HashSet<String>,
    /// The last triage list NautBot was woken for, and when.
    triage: Option<String>,
    triage_at: Option<i64>,
    /// The stale-unowned ticket ids in the last notice the owner actually
    /// received. Empty until one has been posted.
    stale_unowned: std::collections::BTreeSet<String>,
    /// The last refusal recorded for an `owner:ticket`, so a standing one is
    /// not written down every three minutes.
    ///
    /// This exists BECAUSE of the fleet. Waking one owner per tick meant at most
    /// one refusal row per tick; waking every owner means one per owner, and a
    /// ready ticket stays ready until its agent moves it, so a fleet of four
    /// working agents writes four `sweep_refused: skipped_busy` rows every 180
    /// seconds forever. That is the exact shape of the 145 identical rows the
    /// rig found on 2026-09-01, which buried every real entry between them, and
    /// shipping the fleet without this would have re-created it four times over.
    ///
    /// Keyed by reason, not just by ticket: a refusal that CHANGES (busy, then
    /// off the roster) is news again. Cleared by a delivery, so the next refusal
    /// after real work is news again too.
    refused: std::collections::HashMap<String, String>,
}

/// The ticket ids of a stale list, as a set: what the notice is ABOUT, with
/// the titles and the ordering dropped because neither changes what it says.
fn ids_of(tickets: &[(String, String)]) -> std::collections::BTreeSet<String> {
    tickets.iter().map(|(id, _)| id.clone()).collect()
}

impl Announced {
    /// Is this stale list worth a notice, or has the owner already read it?
    ///
    /// The comparison is over the id SET of the last notice that was
    /// DELIVERED. A notice says one thing — these tickets need a decision — so
    /// it is news exactly when that list of tickets changes: one entering or
    /// leaving is a change and is said once, the same list in a different
    /// order is not a change at all.
    ///
    /// An empty list says nothing, so it is never posted AND never erases the
    /// memory. That second half is the fix (XNAUT-310). The list is scoped to
    /// the fleet's projects, read fresh from the board every tick with
    /// `unwrap_or_default()`; a board that is momentarily unreadable therefore
    /// produces an empty list, and forgetting on it re-posted the identical
    /// notice at 12:27, 13:28 and 14:11 on 2026-09-09. A ticket disappearing
    /// for one pass is not the owner deciding anything.
    fn stale_unowned_is_news(&self, tickets: &[(String, String)]) -> bool {
        !tickets.is_empty() && ids_of(tickets) != self.stale_unowned
    }

    /// Remember a notice the owner actually received. Called only after the
    /// inbox write succeeds, so a failed delivery is retried next pass.
    fn stale_unowned_delivered(&mut self, tickets: &[(String, String)]) {
        self.stale_unowned = ids_of(tickets);
    }

    /// Is this dispatch outcome worth a ledger row?
    ///
    /// A delivery always is: it is an event, and the elapsed clock in
    /// `ledger_recent` starts from one. A refusal is news the first time, and
    /// again whenever the REASON changes, and never in between.
    ///
    /// ponytail: the rule is tested, the one line in `run_action` that calls it
    /// is not, because that needs an `AppHandle` and a live roster. Deleting the
    /// call would not turn a test red. Same ceiling as `dispatch_kind` below and
    /// the same fix if it ever matters: give `run_action` a seam for the nudge.
    fn dispatch_is_news(&mut self, key: String, kind: &str, reason: &str) -> bool {
        if kind != "sweep_refused" {
            self.refused.remove(&key);
            return true;
        }
        if self.refused.get(&key).map(String::as_str) == Some(reason) {
            return false;
        }
        self.refused.insert(key, reason.to_string());
        true
    }
}

/// Statuses that mean "a human or an agent finished something and it needs
/// checking". `review` and `done` are the same claim from an agent's side
/// (project_management.rs makes both hand back to NautBot).
/// `pub(crate)` so `sandbox_verify` can assert the link that closes the rail:
/// the status a green run leaves a ticket in must NOT be one this offers, or a
/// passing verification feeds the ticket straight back into the queue it just
/// came out of.
pub(crate) fn awaits_review(status: &str) -> bool {
    matches!(status, "done" | "review")
}

/// Why this instance will not do `what`, in one line meant for the ledger.
///
/// It names the role AND the build, because the failure this ticket exists to
/// make visible is two machines on two versions disagreeing about the same
/// board: "the sweep refused" is not an answer to that, and "the workstation on
/// 1.10.1 refused" is.
///
/// Deterministic on purpose. `Announced::dispatch_is_news` suppresses a refusal
/// whose reason has not changed, so a line carrying anything that varies per
/// tick — a timestamp, a count — would defeat the "says so once" rule and put
/// a row in the ledger every three minutes for as long as the machine is up.
pub(crate) fn refusal(what: &str, role: crate::instance::Role) -> String {
    format!(
        "this instance is a {} on xNAUT {}; it does not {what} — the ticket waits for an \
         instance that does",
        role.as_str(),
        env!("CARGO_PKG_VERSION"),
    )
}

pub fn spawn_sweep_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Let setup finish, then reconcile at startup as well as every tick.
        // Run-specific startup grace protects launches, not a three-minute
        // delay before the registry can recover a previous app's failures.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut announced = Announced::default();
        // A give-up already in the ledger is not news to this process either:
        // two restarts on the morning of 2026-09-06 re-announced 147 of them
        // twice, 294 rows for nothing.
        announced.gave_up = crate::ledger::ledger_recent(Some(5000))
            .into_iter()
            .filter(|e| e.kind == "sweep_gave_up")
            .map(|e| e.ticket)
            .collect();
        loop {
            match tick(&app, &mut announced).await {
                Ok(()) => {}
                // A sweep that dies silently is the very failure mode this
                // sprint exists to kill, so its own errors go in the ledger.
                Err(error) => {
                    crate::ledger::record("sweep_failed", "nautbot", "", &error);
                }
            }
            // Stamped on every tick, including the ones that did nothing at
            // all. The ledger only records ACTIONS, so a quiet tick and a dead
            // loop leave the same trace there; /api/control/doctor reads this
            // instead and can tell them apart. Recording only, no behaviour.
            crate::heartbeat::SWEEP.beat();
            // A stop attempt needs observations during its grace window. Keep
            // the ordinary fleet cadence otherwise; never sleep under a lock.
            let retiring = crate::agents::registry_dir().and_then(|dir| {
                Ok(crate::run_control::list_ids_in(&dir)?.iter().any(|id|
                    crate::run_control::load_manifest_in(&dir, id).is_ok_and(|r|
                        r.state == crate::run_control::RunState::Retiring)))
            }).unwrap_or(false);
            tokio::time::sleep(if retiring { Duration::from_secs(5) } else { TICK }).await;
        }
    });
}

async fn tick(app: &AppHandle, announced: &mut Announced) -> Result<(), String> {
    // Every tick, not only at startup: a run the previous app died on is a
    // ghost until something says so, and it blocks its whole project.
    queue_retries(crate::sandbox_verify::reap_orphaned_runs());
    let switches = crate::switches::load();
    if switches.read_only {
        if !announced.read_only {
            crate::ledger::record(
                "sweep_paused",
                "nautbot",
                "",
                "read_only kill-switch engaged; the sweep is idle until the owner lifts it",
            );
            announced.read_only = true;
        }
        return Ok(());
    }
    announced.read_only = false;

    // A machine with no control repo configured is not a failure, it is an
    // unconfigured install: Project Management is opt-in (`enabled` defaults to
    // false, `repo_path` empty). Saying so once is information. Saying it every
    // three minutes forever is what the rig's ledger actually contained on
    // 2026-09-01: 145 identical rows, burying every real entry between them.
    let registry = crate::agents::registry_dir()?;
    let leases = crate::writer_lease::lease_dir()?;
    let live = tokio::task::spawn_blocking(crate::zellij::live_sessions).await.map_err(|e| e.to_string())?;
    let repo = match crate::project_management::repo_now() {
        Ok(repo) => {
            announced.no_repo = false;
            repo
        }
        Err(error) => {
            registry_tick_in(&registry,&leases,None,&crate::ledger::path(),crate::run_control::now_ms(),
                |r| if matches!(r.state, crate::run_control::RunState::Retiring | crate::run_control::RunState::Degraded | crate::run_control::RunState::Blocked) { crate::run_control::observe_swap_in(&registry,r) } else { crate::run_control::observe_in(&registry,r,&live) })?;
            if !announced.no_repo {
                crate::ledger::record("sweep_idle", "nautbot", "", &error);
                announced.no_repo = true;
            }
            return Ok(());
        }
    };
    let tickets = registry_tick_in(&registry,&leases,Some(&repo),&crate::ledger::path(),crate::run_control::now_ms(),
        |r| if matches!(r.state, crate::run_control::RunState::Retiring | crate::run_control::RunState::Degraded | crate::run_control::RunState::Blocked) { crate::run_control::observe_swap_in(&registry,r) } else { crate::run_control::observe_in(&registry,r,&live) })?;
    announce_undead(app, &registry)?;
    core_team_beat(app, &registry).await;
    core_team_wall_clock(app, &registry, &leases).await;
    core_team_judge(app, &tickets).await;
    let records = crate::sandbox_verify::sandbox_verify_records()
        .await
        .unwrap_or_default();
    let jury_root=registry.join("jury");
    let jury_app=app.clone(); let jury_repo=repo.clone(); let jury_registry=registry.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e)=crate::jury_runtime::reconcile(Some(&jury_app),&jury_repo,&jury_registry,&jury_root) { eprintln!("jury reconcile: {e}"); }
    });
    for record in records.iter().filter(|r|r.status=="passed" && !r.not_evidence) {
        // A record with no commit is not reviewable: there is nothing to diff
        // and no evidence rule to apply. Records from before the registry
        // carry `commit_sha: null`, and scheduling on one produced
        // "git diff --name-only  : ambiguous argument ''" as an owner
        // escalation, every tick, on tickets closed weeks ago (XNAUT-252,
        // 2026-09-09).
        if crate::jury_signoff::nothing_to_sign(record).is_some() { continue; }
        // Two lanes, one decision (XNAUT-319). The audited arm is the
        // condition this loop carried inline; the swarm arm merges its own
        // green build and opens no jury job.
        match crate::swarm::route(&tickets, record) {
            crate::swarm::Route::Jury => crate::jury_signoff::schedule(app, record.clone()),
            crate::swarm::Route::Swarm => crate::swarm::schedule(record.clone()),
            crate::swarm::Route::Nothing => {}
        }
    }
    // Verification runs for every project (a handback is a handback), but
    // the WORK-STARTING half, triage and dispatch, only for projects that
    // opted in. The list is read from the board each tick, so flipping a
    // project on needs no restart.
    let projects = crate::project_management::list_projects(&repo).unwrap_or_default();
    let fleet: std::collections::HashSet<String> =
        projects.iter().filter(|p| p.fleet).map(|p| p.key.clone()).collect();
    // Verification needs the project's checkout ON THIS MACHINE. The board is
    // shared, the disks are not: on tron the sweep offered PLOUGH, 48NAUTS and
    // NAUTTUTOR verifications that all died at "repo path does not exist" and
    // took a strike each (2026-09-06). A project with no local checkout is
    // somebody else's machine's to verify.
    let here: std::collections::HashSet<String> = projects
        .iter()
        .filter(|p| {
            let local = crate::project_management::local_source_path(p);
            !local.trim().is_empty() && std::path::Path::new(local.trim()).is_dir()
        })
        .map(|p| p.key.clone())
        .collect();

    let plan: Vec<Action> = plan_fleet_for(
        &tickets,
        &records,
        next_retry().as_deref(),
        chrono::Utc::now(),
        &Fleet::Only(fleet),
    )
    .into_iter()
    .filter(|a| match a {
        Action::Verify { ticket, .. } | Action::Retry { ticket, .. } => here.contains(project_of(ticket)),
        _ => true,
    })
    .collect();
    // No forgetting here. A pass that plans no stale notice used to clear the
    // memory, which made every gap in the list — a project dropping out of the
    // fleet for a tick, an unreadable board — re-post the notice the owner had
    // already read. `stale_unowned_is_news` now decides on the set alone
    // (XNAUT-310).
    for action in plan {
        run_action(app, announced, action).await;
    }
    Ok(())
}

/// The core team's weekly look outside (XNAUT-357).
///
/// It rides the sweep rather than owning a timer, because the sweep is already
/// the app's clock and a second one would be a second thing that can be off.
/// The DUE CHECK is durable — a file beside the registry — while every other
/// "already said that" in this module is in-memory `Announced` state: those
/// guard repeated NOISE, and re-announcing after a restart costs a duplicate
/// line. This one guards a paid search, and re-running it after every restart
/// would turn a weekly beat into a per-launch one.
///
/// Never fails the tick. A scan that cannot run is a quiet week, not a broken
/// sweep, and the reason lands in the ledger where the rest of the tick's
/// decisions are.
async fn core_team_beat(app: &AppHandle, registry: &std::path::Path) {
    let state = crate::core_team::read_beat_state(registry);
    let core = {
        let app_state = tauri::Manager::state::<crate::state::AppState>(app);
        let guard = app_state.settings.lock().await;
        guard.core_team.clone()
    };
    if !core.enabled {
        return;
    }
    if !crate::core_team::beat_due(
        state.last_beat_ms,
        crate::run_control::now_ms(),
        core.beat_days,
    ) {
        return;
    }
    // Stamped BEFORE the scan, not after. A scan that dies half way through
    // has still spent the search; stamping on success would re-run it on the
    // next tick, three minutes later, and again, for as long as it kept
    // failing. The reason is recorded so a week of silence is readable.
    let mut next = crate::core_team::BeatState {
        last_beat_ms: crate::run_control::now_ms(),
        last_filed: 0,
        last_reason: String::new(),
    };
    let result = crate::core_team::core_team_scan(
        tauri::Manager::state::<crate::state::AppState>(app),
        None,
    )
    .await;
    let line = match &result {
        Err(error) => {
            next.last_reason = error.clone();
            format!("core team scan failed: {error}")
        }
        Ok(scan) if !scan.blocked.is_empty() => {
            next.last_reason = scan.blocked.clone();
            format!("core team idle: {}", scan.blocked)
        }
        Ok(scan) => {
            next.last_filed = scan.filed.len();
            let skipped = scan
                .skipped
                .iter()
                .map(|item| format!("{} ({})", item.repo_url, item.reason))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "core team filed {} finding(s){}",
                scan.filed.len(),
                match skipped.is_empty() {
                    true => String::new(),
                    false => format!("; skipped {skipped}"),
                }
            )
        }
    };
    let _ = crate::core_team::write_beat_state(registry, &next);
    crate::ledger::record("core_team_beat", "researcher", "", &line);

    // The Reviewer ran inside the scan, on exactly what the scan filed. Its
    // verdicts get their own ledger rows: "filed 6" and "6 shelved, 1 to PoC"
    // are different news, and the second is the one that costs money next.
    let Ok(scan) = result else { return };
    for item in scan.weighed {
        crate::ledger::record(
            "core_team_review",
            "reviewer",
            &item.ticket,
            &match item.error.is_empty() {
                true => format!("{}/100 → {:?}", item.score, item.verdict),
                false => format!("could not be weighed: {}", item.error),
            },
        );
    }
}

/// Judge every finished PoC nobody has judged.
///
/// This is the last edge of the loop, and without it the whole thing stops one
/// step short: the Researcher finds, the Reviewer weighs, the PoC builds, and
/// then a finished prototype sits in `done` forever because judging it was
/// something a person had to remember to ask for.
///
/// One per tick. A jury round is two agent runs, and the tick is three minutes;
/// judging a backlog of six at once would put twelve reviewers on the machine
/// in one pass. The rest wait for the next tick, which costs minutes on work
/// that took a PoC an hour.
async fn core_team_judge(app: &AppHandle, tickets: &[crate::project_management::TicketRecord]) {
    {
        let app_state = tauri::Manager::state::<crate::state::AppState>(app);
        if !app_state.settings.lock().await.core_team.enabled {
            return;
        }
    }
    let Some(ticket) = crate::core_team::judgeable(tickets).first().map(|t| t.id.clone()) else {
        return;
    };
    let outcome = crate::core_team::core_team_judge(
        app.clone(),
        tauri::Manager::state::<crate::state::AppState>(app),
        ticket.clone(),
    )
    .await;
    crate::ledger::record(
        "core_team_verdict",
        "judge",
        &ticket,
        &match outcome {
            Ok(result) if result.decision == "returned" => {
                format!("returned unjudged: {}", result.returned.join("; "))
            }
            Ok(result) => format!("{}: {}", result.decision, result.why),
            Err(error) => format!("could not be judged: {error}"),
        },
    );
}

/// Stop a PoC that outlived its wall clock, and say so on its ticket.
///
/// The budget is enforced HERE rather than left to the PoC's own prompt. A
/// brief saying "at most 90 minutes" is an instruction to a model, and a model
/// that is three hours into a port is not the thing to ask whether it should
/// stop. The clock is the app's.
///
/// It reuses `jury_signoff::stop_then_release`, which is the one proven way in
/// this codebase to end a run: TERM the process, delete the session, and prove
/// all three of pid, session and capture file are quiet before releasing the
/// writer lease. A second stop path here would be a second way to strand a
/// lease.
async fn core_team_wall_clock(
    app: &AppHandle,
    registry: &std::path::Path,
    leases: &std::path::Path,
) {
    let minutes = {
        let app_state = tauri::Manager::state::<crate::state::AppState>(app);
        let guard = app_state.settings.lock().await;
        if !guard.core_team.enabled {
            return;
        }
        guard.core_team.poc_minutes
    };
    let runs: Vec<crate::run_control::RunManifest> =
        crate::run_control::list_ids_in(registry)
            .unwrap_or_default()
            .iter()
            .filter_map(|id| crate::run_control::load_manifest_in(registry, id).ok())
            .collect();
    let over = crate::core_team::over_budget(&runs, crate::run_control::now_ms(), minutes);
    if over.is_empty() {
        return;
    }
    let repo = crate::project_management::repo_now().ok();
    for run in over {
        let stopped =
            crate::jury_signoff::stop_then_release(registry, leases, &run.run_id, 2_000);
        let note = crate::core_team::too_big_note(minutes, &run.branch);
        crate::ledger::record(
            "core_team_over_budget",
            &run.agent_handle,
            run.ticket.as_deref().unwrap_or(""),
            &match &stopped {
                Ok(()) => format!("{} after {minutes} minutes on {}", crate::core_team::TOO_BIG, run.branch),
                Err(error) => format!("over its {minutes}-minute budget and would not stop: {error}"),
            },
        );
        // The ticket gets the phrase whether or not the process died quietly:
        // a run that refuses to stop is still over budget, and the ticket is
        // where a person looks. Written once — the second tick finds the
        // sentence already there and leaves it alone.
        let (Some(repo), Some(id)) = (repo.as_ref(), run.ticket.as_deref()) else {
            continue;
        };
        let Ok(ticket) = crate::project_management::ticket_list_in(repo, None)
            .map(|all| all.into_iter().find(|item| item.id == id))
        else {
            continue;
        };
        let Some(ticket) = ticket else { continue };
        if ticket.body.contains(crate::core_team::TOO_BIG) {
            continue;
        }
        let _ = crate::project_management::ticket_update_in(
            repo,
            crate::project_management::TicketUpdateRequest {
                model_requirement: None,
                caller: None,
                id: ticket.id.clone(),
                expected_revision: ticket.revision,
                title: None,
                ticket_type: None,
                status: Some("blocked".into()),
                priority: None,
                owner: None,
                clear_owner: false,
                documentation: None,
                body: Some(format!("{}{note}", ticket.body)),
            },
        );
    }
}

/// Carry out one planned action. Every arm ends in a ledger row, including the
/// ones that did nothing, because a decision with no trace is the failure this
/// whole sprint exists to kill.
///
/// Verifications are SPAWNED and dispatches are AWAITED, and the asymmetry is
/// load-bearing. A verification is minutes long and holding the tick open for it
/// would mean one per tick again by the back door. A dispatch must not be
/// spawned: `spend::admit_launch` counts the sessions that are live AT THE
/// MOMENT IT ASKS (agent_profiles.rs), so N launches racing each other would
/// each read a count from before the others landed and the concurrent cap would
/// admit all N. Awaiting them in turn is what makes the ceiling arithmetic true.
async fn run_action(app: &AppHandle, announced: &mut Announced, action: Action) {
    match action {
        Action::Retry { ticket, project } => {
            crate::ledger::record(
                "sweep_retry",
                "nautbot",
                &ticket,
                "re-running a verification the last app died during",
            );
            spawn_verify(app, ticket, project, "sweep_retry_failed");
        }
        Action::RetryDropped { ticket } => {
            // The ticket is gone from the board (deleted, or another project's).
            // Dropping it is right; saying so is what keeps the queue honest.
            crate::ledger::record(
                "sweep_retry_dropped",
                "nautbot",
                &ticket,
                "orphaned verification's ticket is no longer on the board",
            );
        }
        Action::Verify {
            ticket,
            project,
            status,
        } => {
            // A verification is a RUN: it warms a sandbox, starts a process and
            // costs money. The whole reason the workstation role exists is that
            // runs must not start on the owner's desk, so the same gate that
            // stops a dispatch stops this. A fleet or sandbox instance picks the
            // ticket up; it stays in `done`/`review` until one does.
            let role = crate::instance::role();
            if !role.verifies() {
                let why = refusal("verify", role);
                if announced.dispatch_is_news("verify-here".into(), "sweep_refused", &why) {
                    crate::ledger::record("sweep_refused", "nautbot", &ticket, &why);
                }
                return;
            }
            crate::ledger::record(
                "sweep_verify",
                "nautbot",
                &ticket,
                &format!("{ticket} sat in {status} unreviewed"),
            );
            spawn_verify(app, ticket, project, "sweep_verify_failed");
        }
        Action::GaveUp { ticket, failures } => {
            // Said once per ticket, not once per tick: giving up is news the
            // first time and noise every three minutes after that.
            if announced.gave_up.insert(ticket.clone()) {
                crate::ledger::record(
                    "sweep_gave_up",
                    "nautbot",
                    &ticket,
                    &format!(
                        "{ticket} failed verification {failures} times in a row; the sweep will \
                         stop offering it until one passes or the records are cleared"
                    ),
                );
            }
        }
        Action::Dispatch {
            ticket,
            owner,
            title,
        } => {
            // DISPATCH, never a keystroke into a live session. A
            // ready ticket with an owner is started fresh: its own worktree on
            // the ticket's branch, a new process of the owner's runtime, the
            // ticket moved to in_progress. Typing the wake into whatever
            // session the owner already had is what bound a task to one
            // harness and let a four-day-old session pick up XNAUT-295 from the
            // wrong worktree. The move to in_progress is also what stops the
            // re-poke: a dispatched ticket is no longer `ready`, so this arm
            // does not see it again next tick (the wake_skipped_busy /
            // sweep_refused / nudged churn on 2026-09-06).
            // Below the launch floor the ticket is left exactly as it is:
            // owner kept, status kept. A full disk is not the runtime's
            // fault, and handing the ticket back would clear its owner for
            // a reason the next triage cannot fix (XNAUT-318).
            if let Some(why) = crate::housekeeper::launch_floor() {
                if announced.dispatch_is_news("floor".into(), "sweep_refused", &why) {
                    crate::ledger::record("sweep_refused", "housekeeper", &ticket, &why);
                }
                return;
            }
            // Per machine, not per fleet: the ticket stays ready and owned so
            // the machine that does dispatch takes it (XNAUT-358, now the role
            // rather than the boolean — XNAUT-370).
            let role = crate::instance::role();
            if !role.dispatches() {
                let why = refusal("dispatch", role);
                if announced.dispatch_is_news("here".into(), "sweep_refused", &why) {
                    crate::ledger::record("sweep_refused", "nautbot", &ticket, &why);
                }
                return;
            }
            let project = project_of(&ticket).to_string();
            let (kind, reason) = match crate::dispatch::pm_ticket_dispatch(
                app.clone(),
                ticket.clone(),
                project,
            )
            .await
            {
                Ok(result) => (
                    "sweep_dispatch",
                    format!("launched {} on {}: {title}", result.handle, result.branch),
                ),
                // The runtime could not start (binary missing, login expired,
                // provider down): the ticket is handed back to triage so a
                // runtime that CAN start takes it. That is the outage failover.
                Err(error) => {
                    hand_back_for_reassignment(app, &ticket, &owner, &error).await;
                    ("sweep_dispatch_failed", error)
                }
            };
            if announced.dispatch_is_news(format!("{owner}:{ticket}"), kind, &reason) {
                crate::ledger::record(kind, &owner, &ticket, &reason);
            }
        }
        Action::NotifyStaleUnowned { tickets } => {
            if !announced.stale_unowned_is_news(&tickets) { return; }
            // The notice carries the WHOLE current list, not the newcomers:
            // it is a standing state the owner has to decide about, and a
            // notice listing one of three stale tickets reads as if the other
            // two had been dealt with.
            let req = stale_unowned_notice(&tickets);
            match crate::inbox::create_and_announce(app, "notify", req, None) {
                Ok(_) => {
                    announced.stale_unowned_delivered(&tickets);
                    crate::ledger::record("sweep_stale_unowned", "nautbot", "",
                        &format!("{} stale unowned ticket(s) surfaced for a person", tickets.len()));
                }
                Err(error) => crate::ledger::record("sweep_stale_unowned_failed", "nautbot", "", &error),
            }
        }
        Action::Triage { tickets } => {
            // Once per distinct list, and never more than every thirty
            // minutes: NautBot assigned five of eight at 10:36 and the sweep
            // handed it the next eight at 10:37, mid-work. A triage list is
            // a batch, not a stream.
            let key = tickets.join("|");
            let now_ms = chrono::Utc::now().timestamp_millis();
            if !triage_due(announced, &key, now_ms) {
                return;
            }
            announced.triage = Some(key);
            announced.triage_at = Some(now_ms);
            // Only owners whose runtime is installed HERE. On 2026-09-06 a
            // triage on the Studio assigned grok, whose CLI was not
            // authenticated, and on tron gemini launched with a shape its
            // CLI refused; a ticket assigned to a handle that cannot start
            // sits in_progress with nobody on it.
            let assignable = tokio::task::spawn_blocking(assignable_owners).await.unwrap_or_default();
            let per_ticket = crate::project_management::repo_now().and_then(|repo|
                crate::project_management::ticket_list_in(&repo, None)).unwrap_or_default();
            let eligibility = per_ticket.iter().filter(|t| tickets.contains(&t.id)).map(|t| {
                let owners = assignable_owners_for(&t.model_requirement);
                format!("- {}: required model {:?}; eligible owners: {}. Assign only from this list; if empty leave unassigned.",
                    t.id, t.model_requirement, if owners.is_empty() { "none".into() } else { owners.join(", ") })
            }).collect::<Vec<_>>().join("\n");
            let message = format!(
                "Triage: these ready or in_progress tickets have no owner and were touched in the last {FRESH_DAYS} days. For \
                 each one: first check whether the work already shipped (git log, the vault); if it did, set \
                 it to complete with the commit. Otherwise assign an owner and dispatch it with \
                 dispatch_ticket (not a wake), or set it back to inbox with one line saying why it is not \
                 ready. Owners that can run on this machine: {}. Assign no one else. Do not start the work \
                 yourself.\n{}",
                if assignable.is_empty() { "none".to_string() } else { assignable.join(", ") },
                eligibility
            );
            let nautbot = crate::agent_profiles::RESERVED_NAUTBOT_HANDLE;
            let outcome = match crate::nudge::nudge_agent(app, nautbot, &message).await {
                Ok(value) => value.get("delivery").and_then(|d| d.as_str()).unwrap_or("unknown").to_string(),
                Err(error) => format!("refused: {error}"),
            };
            crate::ledger::record(
                "sweep_triage",
                nautbot,
                "",
                &format!("{outcome}: {} unowned ready/in_progress ticket(s)", tickets.len()),
            );
        }
    }
}

fn spawn_verify(app: &AppHandle, ticket: String, project: String, failure_kind: &'static str) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) =
            crate::sandbox_verify::sandbox_verify_start_inner(app, ticket.clone(), project).await
        {
            crate::ledger::record(failure_kind, "nautbot", &ticket, &error);
        }
    });
}

/// One thing the sweep decided to do this tick.
///
/// Splitting the decision from the doing is what makes a fleet run assertable at
/// all. `tick` needs an `AppHandle` and the owner's configured control repo;
/// `plan_fleet` needs a board, some verify records and a clock, all three of
/// which a test can build in a temp directory. Same reasoning as `run_verify`
/// taking an `Option<AppHandle>` and `settle_ticket_in` taking a repo path:
/// nothing about deciding what to do needs a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Re-run a verification the last app died during.
    Retry { ticket: String, project: String },
    /// That orphan's ticket is no longer on the board.
    RetryDropped { ticket: String },
    /// Verify a ticket someone handed back and nobody looked at.
    Verify {
        ticket: String,
        project: String,
        status: String,
    },
    /// Announce that a ticket has failed its way out of the queue.
    GaveUp { ticket: String, failures: usize },
    /// Wake an owner and hand them a ticket.
    Dispatch {
        ticket: String,
        owner: String,
        title: String,
    },
    /// Wake NautBot to assign or dispatch ready tickets nobody owns. The board
    /// stalled on this exact step on the night of 2026-09-05: 97 ready
    /// tickets, none with an owner, one NautBot idling until it was reaped.
    Triage { tickets: Vec<String> },
    /// Stale work needs a person, never an automatic assignment.
    NotifyStaleUnowned { tickets: Vec<(String, String)> },
}

/// How many verifications the sweep is willing to have running at once.
///
/// This REPLACES a bound that never existed. The old rule read "one verification
/// started per tick", which bounds a rate and not a population: at one every 180
/// seconds against runs that take minutes, the number actually in flight was
/// held down by the 30-minute cooldown and by nothing else, and on a board of
/// twenty handbacks it would have climbed all day. So the bound moves from "how
/// often may one start" to "how many may be live", which is the question a
/// ceiling actually asks.
///
/// It has to be stated here because nothing else states it. A verification warms
/// a GitVM or exe.dev sandbox; it never touches `spend::admit_launch`, whose
/// concurrent and daily caps count AGENT SESSIONS and say nothing about sandbox
/// spend. This constant is the only ceiling verifications have, which is why it
/// is small.
const MAX_VERIFIES_IN_FLIGHT: usize = 3;

/// Everything the sweep will do about this board, right now.
///
/// Three rules, in the order they matter:
///
///   1. Unfinished business first. A verification that died with the last app is
///      work already decided on, so it goes ahead of anything new and is never
///      squeezed out by the in-flight budget.
///   2. Handbacks up to the budget, oldest first, walking PAST held tickets
///      rather than stopping at them, and still announcing every give-up it
///      passes. The rig measured what stopping costs: RIG-4 drew nothing for 25
///      minutes across eight ticks because a held ticket sat ahead of it.
///   3. Dispatch, and this is the fleet: EVERY owner with ready work is woken,
///      not just the one holding the oldest ticket. At most one ticket per owner
///      per tick, because a second message typed into the same session lands on
///      an agent that is mid-turn on the first.
///
/// Nothing here bounds dispatch by a number, on purpose. The bound is the
/// roster: `admit_launch` refuses each cold launch past the concurrent cap and
/// engages `read_only` at the daily cap, and a wake to an ALREADY LIVE session
/// costs nothing to admit because there is nothing to launch. Inventing a second
/// number here would be a brake that looks like the ceiling and is not one.
fn plan_fleet(
    tickets: &[crate::project_management::TicketRecord],
    records: &[crate::sandbox_verify::VerifyRecord],
    retry: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<Action> {
    plan_fleet_for(tickets, records, retry, now, &Fleet::Every)
}

/// Which projects the fleet may START work in. Verification is never scoped:
/// a handback is a handback wherever it comes from.
pub(crate) enum Fleet {
    Every,
    Only(std::collections::HashSet<String>),
}

impl Fleet {
    fn allows(&self, ticket_id: &str) -> bool {
        match self {
            Fleet::Every => true,
            Fleet::Only(keys) => keys.contains(project_of(ticket_id)),
        }
    }
}

fn plan_fleet_for(
    tickets: &[crate::project_management::TicketRecord],
    records: &[crate::sandbox_verify::VerifyRecord],
    retry: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
    fleet: &Fleet,
) -> Vec<Action> {
    let mut actions = Vec::new();
    let mut budget = MAX_VERIFIES_IN_FLIGHT.saturating_sub(verifies_in_flight(records, now));
    let mut busy: std::collections::HashSet<String> = records
        .iter()
        .filter(|record| is_live_run(record, now))
        .map(|record| record.project.clone())
        .collect();

    if let Some(id) = retry {
        match tickets.iter().find(|t| t.id == id) {
            Some(ticket) => {
                actions.push(Action::Retry {
                    ticket: ticket.id.clone(),
                    project: ticket.project.clone(),
                });
                budget = budget.saturating_sub(1);
                busy.insert(ticket.project.clone());
            }
            None => actions.push(Action::RetryDropped {
                ticket: id.to_string(),
            }),
        }
    }

    // One verification per PROJECT per pass, and none for a project that has
    // one running (XNAUT-287). gitvm is directory-scoped and every ticket
    // without a worktree of its own verifies in the project's source_path, so
    // two offered together collide: the second died at "already warm" 47
    // seconds after the first started, on 2026-09-05, and was counted as a
    // failure against a ticket that had done nothing wrong. This plan cannot
    // resolve directories (that needs git and settings), and the project is
    // the honest approximation at this altitude.
    for ticket in awaiting_review(tickets) {
        let hold = hold_for(records, &ticket.id, now);
        if let Hold::GaveUp(failures) = hold {
            actions.push(Action::GaveUp {
                ticket: ticket.id.clone(),
                failures,
            });
        }
        // Keep walking either way. A held ticket must not hide the one behind
        // it, and a spent budget must not silence the give-ups further down.
        if hold != Hold::None || budget == 0 || busy.contains(&ticket.project) {
            continue;
        }
        busy.insert(ticket.project.clone());
        actions.push(Action::Verify {
            ticket: ticket.id.clone(),
            project: ticket.project.clone(),
            status: ticket.status.clone(),
        });
        budget -= 1;
    }

    let stale: Vec<(String, String)> = tickets.iter()
        .filter(|t| unowned_triage_status(t) && !is_fresh(t, now) && fleet.allows(&t.id))
        .map(|t| (t.id.clone(), t.title.clone()))
        .collect();
    if !stale.is_empty() {
        actions.push(Action::NotifyStaleUnowned { tickets: stale });
    }

    let unowned: Vec<String> = ready_unowned_urgent(tickets, now)
        .iter()
        .filter(|t| fleet.allows(&t.id))
        .map(|t| format!("{}: {}", t.id, t.title))
        .collect();
    if !unowned.is_empty() {
        actions.push(Action::Triage { tickets: unowned });
    }

    let mut woken: std::collections::HashSet<String> = std::collections::HashSet::new();
    for ticket in ready_with_owner(tickets)
        .into_iter()
        .filter(|t| fleet.allows(&t.id) && is_fresh(t, now))
    {
        let owner = ticket.owner.clone().unwrap_or_default();
        if woken.insert(owner.clone()) {
            actions.push(Action::Dispatch {
                ticket: ticket.id.clone(),
                owner,
                title: ticket.title.clone(),
            });
        }
    }
    actions
}

/// How many distinct tickets have a verification running right now.
///
/// Shares `is_live_run` with `hold_for` rather than re-deriving the freshness
/// rule: two copies of "a `running` record younger than an hour" would drift,
/// and the drift would show up as a fleet that starts a fourth verification
/// while calling the third one dead.
fn verifies_in_flight(
    records: &[crate::sandbox_verify::VerifyRecord],
    now: chrono::DateTime<chrono::Utc>,
) -> usize {
    records
        .iter()
        .filter(|record| is_live_run(record, now))
        .map(|record| record.ticket_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len()
}

/// Which ledger kind a completed nudge earns: did it actually hand the ticket
/// to an agent, or was it turned away?
///
/// `Ok` is not the same as delivered, and reading it that way is what made
/// `sweep_refused` look like dead code. The rig found it in the binary with zero
/// rows behind it across a whole 500-row ledger, and it was right that nothing
/// had ever emitted it: the refusals that matter do not come back as errors.
///
/// `nudge_agent` reports what became of a wake as DATA (nudge.rs `Delivery`), so
/// three non-deliveries return `Ok`:
///
///   - `no_session`: no live session, and the cold launch was declined or died.
///     This is the SPEND CEILING's path. `admit_launch` refuses in
///     agent_profiles.rs, `cold_launch` propagates it, and nudge.rs turns that
///     error into `Delivery::NoSession`. The comment at the error arm below used
///     to claim the ceiling's refusal landed there; it never could.
///   - `skipped_busy`: the agent is mid-turn and typing would garble its input.
///   - `unknown`: a payload with no `delivery` field, which is not a dispatch
///     either.
///
/// All three were recorded as `sweep_dispatch`, so the ledger asserted the sweep
/// had handed a ticket to an owner when nothing had been handed anywhere. That
/// is the failure scheduler.rs `record_reap` already names in as many words: a
/// row claiming work happened when none did is worse than no row at all.
///
/// ponytail: the classification is pure and tested; the one line that calls it
/// is not, because `tick` needs an AppHandle and a control repo. Deleting the
/// call would not turn a test red.
/// A dispatch that could not start hands the ticket back: owner cleared, back
/// to `ready`, the reason on the body. Next triage reassigns it to a runtime
/// that can run here. This is the model-agnostic failover: the task is not
/// bound to a harness, so if Anthropic is down the ticket moves to Codex.
async fn hand_back_for_reassignment(app: &AppHandle, ticket: &str, owner: &str, why: &str) {
    let project = project_of(ticket).to_string();
    let state = tauri::Manager::state::<crate::state::AppState>(app);
    let Ok(tickets) = crate::project_management::pm_ticket_list(state.clone(), Some(project)).await
    else {
        return;
    };
    let Some(t) = tickets.into_iter().find(|t| t.id == ticket) else {
        return;
    };
    let note = format!(
        "\n\n---\nDispatch to @{owner} could not start: {why}. Owner cleared; triage reassigns to a runtime that can run here."
    );
    let _ = crate::project_management::pm_ticket_update(
        state,
        crate::project_management::TicketUpdateRequest {
            model_requirement: None,
            caller: None,
            id: t.id.clone(),
            expected_revision: t.revision,
            title: None,
            ticket_type: None,
            status: Some("ready".into()),
            priority: None,
            owner: None,
            clear_owner: true,
            documentation: None,
            body: Some(format!("{}{note}", t.body)),
        },
    )
    .await;
}

fn dispatch_kind(delivery: &str) -> &'static str {
    if matches!(delivery, "launched" | "typed") {
        "sweep_dispatch"
    } else {
        "sweep_refused"
    }
}

/// Every ticket awaiting review, oldest first. A list rather than a single
/// pick, because the caller has to be able to walk past one that is held.
fn awaiting_review(
    tickets: &[crate::project_management::TicketRecord],
) -> Vec<&crate::project_management::TicketRecord> {
    let mut out: Vec<&crate::project_management::TicketRecord> = tickets
        .iter()
        .filter(|t| awaits_review(&t.status))
        // A finished PoC is JUDGED, not verified (XNAUT-357). It has no
        // bundle, no handback and no sandbox record to be green about, so the
        // verify lane would spend a sandbox on it and then report it red for
        // lacking evidence it was never asked to produce. `core_team_judgeable`
        // is what picks these up instead.
        .filter(|t| !t.ticket_type.eq_ignore_ascii_case(crate::core_team::FINDING_TYPE))
        .collect();
    // Freshest EVIDENCE first, not oldest ticket (XNAUT-298). A handback naming
    // commits is an agent saying "this is finished, here is the proof", which is
    // the strongest reason to verify something; a `done` ticket from the Loops
    // era with no handback is a status somebody set months ago.
    //
    // Oldest-first came from "the ticket that has waited longest", the exact
    // reasoning XNAUT-247 corrected for wakes and never for verification. On
    // 2026-09-06 it cost XNAUT-295 over seventy minutes behind XNAUT-6 through
    // 10, each of which failed three times on the way past.
    out.sort_by(|a, b| {
        let evidence = |t: &crate::project_management::TicketRecord| {
            t.handback.as_ref().is_some_and(|h| !h.commits.is_empty())
        };
        evidence(b)
            .cmp(&evidence(a))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    out
}

/// Every ready ticket that names an owner, oldest first.
///
/// A list rather than the single oldest, because a fleet wakes every owner with
/// work and not just the one whose ticket has been waiting longest. Sorted so
/// that when an owner holds several, the one they are handed is the oldest.
/// Ready or in_progress, unowned, and urgent: what NautBot is woken to triage. Oldest first
/// and capped, so one wake is a list a person could read too.
fn ready_unowned_urgent(
    tickets: &[crate::project_management::TicketRecord],
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<&crate::project_management::TicketRecord> {
    let mut out: Vec<&crate::project_management::TicketRecord> = tickets
        .iter()
        .filter(|t| unowned_triage_status(t))
        .filter(|t| matches!(t.priority.as_str(), "high" | "critical"))
        .filter(|t| is_fresh(t, now))
        .collect();
    out.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    out.truncate(MAX_TRIAGE_PER_WAKE);
    out
}

fn unowned_triage_status(t: &crate::project_management::TicketRecord) -> bool {
    matches!(t.status.as_str(), "ready" | "in_progress")
        && t.owner.as_deref().map(str::trim).unwrap_or("").is_empty()
}

fn stale_unowned_notice(tickets: &[(String, String)]) -> crate::inbox::PostRequest {
    crate::inbox::PostRequest {
        from: "@nautbot".into(),
        title: "Stale unowned tickets need a person's decision".into(),
        body: format!(
            "These unowned ready or in_progress tickets were not touched in the last {FRESH_DAYS} days \
             (or have an unreadable update time). They have not been dispatched or assigned. \
             Please decide whether the work is still wanted.\n{}",
            tickets.iter().map(|(id, title)| format!("- {id}: {title}")).collect::<Vec<_>>().join("\n")
        ),
        ..Default::default()
    }
}

const MAX_TRIAGE_PER_WAKE: usize = 8;

/// A ticket nobody has touched in two weeks is not evidence of wanted work:
/// every ticket the first triage handed out (2026-09-06) was from July, and
/// one of them had shipped in August. Stale tickets need a person, not an
/// agent; the triage prompt says so.
const FRESH_DAYS: i64 = 14;

fn is_fresh(t: &crate::project_management::TicketRecord, now: chrono::DateTime<chrono::Utc>) -> bool {
    chrono::DateTime::parse_from_rfc3339(&t.updated_at)
        .map(|at| now - at.with_timezone(&chrono::Utc) <= chrono::Duration::days(FRESH_DAYS))
        .unwrap_or(false)
}

/// `XNAUT-44` -> `XNAUT`.
fn project_of(ticket_id: &str) -> &str {
    ticket_id.rsplit_once('-').map(|(p, _)| p).unwrap_or(ticket_id)
}
const TRIAGE_EVERY_MS: i64 = 30 * 60 * 1000;

/// How long the SAME list waits before it is put to NautBot again.
///
/// It used to wait forever. The guard compared the list to the last one
/// triaged and returned on a match with no time in the comparison at all, so a
/// board that stopped changing was triaged once per app process and never
/// again. On 2026-09-10 tron had ticked 433 times over 21 hours with three
/// fresh unowned tickets in front of it and had not triaged since the 8th.
///
/// Long, because re-asking a question NautBot has already answered is its own
/// kind of noise, and that is what the equality check was there to prevent.
/// Not infinite, because a list that has not changed after six hours is the
/// definition of stuck.
const TRIAGE_REPEAT_MS: i64 = 6 * 60 * 60 * 1000;

/// May this triage list go to NautBot now? A list nobody has seen waits only
/// for the ordinary rate limit; one already triaged waits much longer.
fn triage_due(announced: &Announced, key: &str, now_ms: i64) -> bool {
    let wait = if announced.triage.as_deref() == Some(key) {
        TRIAGE_REPEAT_MS
    } else {
        TRIAGE_EVERY_MS
    };
    !announced.triage_at.is_some_and(|at| now_ms - at < wait)
}

/// Profile handles whose runtime binary is on this machine's PATH, NautBot
/// excluded (it does not assign to itself). Read at triage time, so a CLI
/// installed after the app started counts.
pub(crate) fn assignable_owners() -> Vec<String> { assignable_owners_for("") }

fn assignable_owners_for(requirement: &str) -> Vec<String> {
    let Ok(registry) = crate::agents::load_or_seed_registry() else {
        return Vec::new();
    };
    let installed: std::collections::HashSet<String> = registry
        .agents
        .iter()
        .filter(|r| crate::agents::binary_on_path(&r.detect_cmd))
        .map(|r| r.id.clone())
        .collect();
    let mut out: Vec<String> = crate::agent_profiles::agent_profile_list()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.handle != crate::agent_profiles::RESERVED_NAUTBOT_HANDLE)
        .filter(|p| installed.contains(&p.runtime_id))
        .filter(|p| crate::agents::registry_dir().and_then(|dir|
            crate::run_control::runtime_meets_in(&dir, &p.runtime_id, &p.model, requirement)).unwrap_or(false))
        .map(|p| format!("@{}", p.handle))
        .collect();
    out.sort();
    out
}
const WOKEN_RECENTLY_MINUTES: i64 = 10;

/// Was this owner woken or dispatched, by the sweep or by an agent's tool, in
/// the last ten minutes? Read from the ledger, so NautBot's wakes count too.
fn recently_woken(owner: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    let cutoff = now - chrono::Duration::minutes(WOKEN_RECENTLY_MINUTES);
    crate::ledger::ledger_recent(Some(400)).into_iter().any(|e| {
        e.agent == owner
            && matches!(e.kind.as_str(), "dispatched" | "nudged" | "sweep_dispatch")
            && chrono::DateTime::parse_from_rfc3339(&e.at)
                .map(|t| t.with_timezone(&chrono::Utc) > cutoff)
                .unwrap_or(false)
    })
}

fn ready_with_owner(
    tickets: &[crate::project_management::TicketRecord],
) -> Vec<&crate::project_management::TicketRecord> {
    let mut out: Vec<&crate::project_management::TicketRecord> = tickets
        .iter()
        .filter(|t| t.status == "ready")
        .filter(|t| {
            t.owner
                .as_deref()
                .map(|o| !o.trim().is_empty())
                .unwrap_or(false)
        })
        .collect();
    out.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    out
}

/// Why the sweep is not verifying this ticket right now.
#[derive(Debug, PartialEq)]
enum Hold {
    /// Nothing in the way. Verify it.
    None,
    /// A verification is already in flight.
    InFlight,
    /// One ran recently. Retrying immediately buys nothing.
    Cooling,
    /// It has failed its way out of the queue. Say so once, then stop asking.
    GaveUp(usize),
    /// Its last run was green about a tree that holds none of its work
    /// (XNAUT-294). Not a strike, and not worth running the same tree again:
    /// the ticket changes hands or the tree changes before this clears.
    NotEvidence,
}

/// How long after a finished verification before the same ticket may draw
/// another. Long enough that a broken verify cannot spin; short enough that a
/// real fix lands within a coffee break.
const VERIFY_COOLDOWN: chrono::Duration = chrono::Duration::minutes(30);

/// Consecutive failures before the sweep stops offering this ticket.
const MAX_VERIFY_ATTEMPTS: usize = 3;

/// Is this record a verification that is actually running?
///
/// A record stuck at `running` because its app died is NOT in flight after an
/// hour. Without that the zombie record of 2026-08-31 would have meant a ticket
/// that could never be verified again, and a fleet whose in-flight budget was
/// permanently spent on runs that ended weeks ago.
fn is_live_run(
    record: &crate::sandbox_verify::VerifyRecord,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    record.status == "running"
        && chrono::DateTime::parse_from_rfc3339(&record.updated_at)
            .map(|at| at.with_timezone(&chrono::Utc) > now - chrono::Duration::hours(1))
            .unwrap_or(false)
}

/// Should the sweep start a verification for this ticket?
///
/// The first version of this asked only "is one running?", which is a question
/// about the present and not about the past. A verification that FAILED left no
/// running record, so the next tick started another, and the next: on the rig
/// (2026-09-01) that produced 105 identical failed records for RIG-2 in one
/// hour, every one failing for the same permanent reason (no gitvm on the
/// machine). The sweep was working exactly as written and was still a bug.
///
/// Three holds now, in order of how long they last: something is in flight, or
/// something finished recently, or it has failed enough times that asking again
/// is noise rather than diligence.
///
/// Pure so the policy is testable without a disk or a clock.
fn hold_for(records: &[crate::sandbox_verify::VerifyRecord], ticket_id: &str, now: chrono::DateTime<chrono::Utc>) -> Hold {
    let at = |record: &crate::sandbox_verify::VerifyRecord| {
        chrono::DateTime::parse_from_rfc3339(&record.updated_at)
            .map(|t| t.with_timezone(&chrono::Utc))
            .ok()
    };
    let mine: Vec<&crate::sandbox_verify::VerifyRecord> =
        records.iter().filter(|r| r.ticket_id == ticket_id).collect();

    if mine.iter().any(|r| is_live_run(r, now)) {
        return Hold::InFlight;
    }

    // Consecutive failures since the last pass. A ticket that has ever been
    // verified green starts its count again, so fixing the repo re-opens it.
    let mut failures = 0usize;
    let mut ordered: Vec<&crate::sandbox_verify::VerifyRecord> = mine.clone();
    ordered.sort_by_key(|r| r.updated_at.clone());
    if ordered.last().is_some_and(|r| r.status == "passed" && r.not_evidence) {
        return Hold::NotEvidence;
    }
    for record in ordered.iter().rev() {
        match record.status.as_str() {
            "failed" => failures += 1,
            "passed" => break,
            _ => {}
        }
    }
    if failures >= MAX_VERIFY_ATTEMPTS {
        return Hold::GaveUp(failures);
    }

    let cooling = now - VERIFY_COOLDOWN;
    if mine
        .iter()
        .any(|r| at(r).map(|t| t > cooling).unwrap_or(false))
    {
        return Hold::Cooling;
    }
    Hold::None
}

// ─── What happened ───────────────────────────────────────────────────────────
//
// A fleet run that nobody can read afterwards is not a fleet run, it is a
// rumour. The pieces of the answer already exist and had never been joined: the
// ledger knows which agent was handed which ticket, the verify records know
// which runs went green, and the board knows where each ticket ended up. Three
// files, three shapes, and the question "what happened" needs all three at once.
//
// So: one row per ticket the sweep touched, joining them.
//
// The field that matters is `verified`. The failure this exists to catch is not
// "nothing happened", it is a board that LOOKS finished: a ticket sitting at
// `complete` with no green verification behind it. Only `settle_ticket_in` is
// supposed to write that status and only on the strength of a passing record, so
// a `complete` with `verdict` anything other than `passed` means something moved
// a ticket that nothing verified. That is the exact shape of "it looks like it
// worked", and it is now a boolean rather than an impression.

/// What became of one ticket in a sweep's run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TicketOutcome {
    pub ticket: String,
    /// Where the ticket sits now, read off the board.
    pub status: String,
    /// The agents the sweep handed it to, in order.
    pub owners: Vec<String>,
    /// Every sweep decision about this ticket, oldest first, as ledger kinds.
    pub trail: Vec<String>,
    /// The verdict of its most recent verification: passed, failed, orphaned,
    /// running; empty when none has ever run.
    pub verdict: String,
    /// The ticket reached a closed status on the strength of a green run.
    pub verified: bool,
    /// The ticket is closed and NOTHING verified it. The one row a reader has to
    /// look at before believing a fleet run worked.
    pub unverified_close: bool,
}

/// What the sweep has been doing to the board, one row per ticket it touched.
///
/// `limit` is how far back through the ledger to read; the default covers a few
/// hours of a busy fleet.
#[tauri::command]
pub async fn sweep_fleet_report(limit: Option<usize>) -> Result<Vec<TicketOutcome>, String> {
    let repo = crate::project_management::repo_now()?;
    let tickets = crate::project_management::ticket_list_in(&repo, None)?;
    let records = crate::sandbox_verify::sandbox_verify_records()
        .await
        .unwrap_or_default();
    Ok(fleet_report(
        &tickets,
        &crate::ledger::ledger_recent(Some(limit.unwrap_or(500))),
        &records,
    ))
}

/// Join the ledger, the verify records and the board into one answer per ticket.
///
/// Pure over the three inputs so the join can be asserted without a control repo
/// or a config directory, the same reason `plan_fleet` is pure.
///
/// ponytail: the caller supplies the ledger slice. The window a reader means by
/// "this run" is theirs to choose (since the app started, since a given time),
/// and inventing a run id here would mean threading one through every writer for
/// a question that a timestamp already answers.
pub fn fleet_report(
    tickets: &[crate::project_management::TicketRecord],
    entries: &[crate::ledger::Entry],
    records: &[crate::sandbox_verify::VerifyRecord],
) -> Vec<TicketOutcome> {
    let mut out: Vec<TicketOutcome> = Vec::new();
    for ticket in tickets {
        // Ledger ids are stored upper-cased (`ledger::record`), so compare that
        // way or every join silently comes back empty.
        let id = ticket.id.to_uppercase();
        let mine: Vec<&crate::ledger::Entry> =
            entries.iter().filter(|e| e.ticket == id).collect();
        if mine.is_empty() {
            continue; // The sweep never touched it; it is not part of this run.
        }
        let verdict = records
            .iter()
            .filter(|r| r.ticket_id.to_uppercase() == id)
            .max_by(|a, b| a.updated_at.cmp(&b.updated_at))
            .map(|r| r.status.clone())
            .unwrap_or_default();
        let closed = ticket.status == crate::sandbox_verify::PASSED_STATUS;
        out.push(TicketOutcome {
            ticket: ticket.id.clone(),
            status: ticket.status.clone(),
            owners: {
                let mut seen: Vec<String> = Vec::new();
                for entry in &mine {
                    if entry.kind == "sweep_dispatch" && !seen.contains(&entry.agent) {
                        seen.push(entry.agent.clone());
                    }
                }
                seen
            },
            trail: mine.iter().map(|e| e.kind.clone()).collect(),
            verified: closed && verdict == "passed",
            unverified_close: closed && verdict != "passed",
            verdict,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_management::TicketRecord;

    fn ticket(id: &str, status: &str, owner: Option<&str>, updated: &str) -> TicketRecord {
        let mut t: TicketRecord = serde_json::from_str(
            r#"{"id":"X","project":"XNAUT","title":"t","type":"bug","status":"ready","priority":"medium","revision":1,"created_at":"2026-08-01","updated_at":"2026-08-01","source_id":""}"#,
        )
        .expect("fixture parses");
        t.id = id.to_string();
        t.status = status.to_string();
        t.owner = owner.map(str::to_string);
        t.updated_at = updated.to_string();
        t
    }

    /// A finished PoC is judged, not verified (XNAUT-357). The verify lane
    /// would warm a sandbox for it and then report it red for lacking a bundle
    /// and a handback it was never asked to produce.
    #[test]
    fn a_finished_finding_never_enters_the_verify_lane() {
        let mut finding = ticket("CORE-1", "done", Some("nautbot"), "2026-09-14");
        finding.ticket_type = crate::core_team::FINDING_TYPE.into();
        let ordinary = ticket("XNAUT-1", "done", Some("nautbot"), "2026-09-14");
        let board = vec![finding, ordinary];
        let queued: Vec<&str> = awaiting_review(&board).iter().map(|t| t.id.as_str()).collect();
        assert_eq!(queued, vec!["XNAUT-1"]);
        // And it is not simply invisible: the Judge's own queue picks it up.
        assert_eq!(
            crate::core_team::judgeable(&[{
                let mut poc = board[0].clone();
                poc.tags = vec![crate::core_team::POC_TAG.into()];
                poc
            }])
            .len(),
            1
        );
    }

    fn record(ticket: &str, status: &str, updated: &str) -> crate::sandbox_verify::VerifyRecord {
        crate::sandbox_verify::VerifyRecord {
            id: format!("{ticket}-{updated}"),
            run_id: "r".into(),
            ticket_id: ticket.into(),
            project: "RIG".into(),
            not_evidence: false,
            repo_path: String::new(),
            commit_sha: String::new(),
            provider_kind: "gitvm-cli".into(),
            sandbox_id: String::new(),
            public_url: String::new(),
            error: String::new(),
            status: status.into(),
            steps: vec![],
            checks: vec![],
            log_dir: String::new(),
            video_path: None,
            screenshot_path: None,
            capture_at: String::new(),
            capture_note: String::new(),
            created_at: updated.into(),
            updated_at: updated.into(),
        }
    }

    fn at(hhmm: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(&format!("2026-09-01T{hhmm}:00+00:00"))
            .expect("fixture parses")
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn the_same_triage_list_is_asked_again_eventually_not_never() {
        // The guard used to compare the list to the last one triaged and
        // return on a match, with no time in the comparison. A board that
        // stopped changing was therefore triaged once per app process and
        // never again: on 2026-09-10 tron had ticked 433 times across 21 hours
        // with three fresh unowned tickets in front of it, and had not triaged
        // since the 8th.
        let mut announced = Announced::default();
        let list = "XNAUT-311|XNAUT-312|XNAUT-313";
        let other = "XNAUT-400";

        // Nothing triaged yet: anything goes.
        assert!(triage_due(&announced, list, 0));

        announced.triage = Some(list.to_string());
        announced.triage_at = Some(0);

        // The same list, straight away and an hour later: not yet.
        assert!(!triage_due(&announced, list, 1));
        assert!(!triage_due(&announced, list, 60 * 60 * 1000));

        // A DIFFERENT list still only waits the ordinary rate limit, so new
        // work is not held behind the long repeat window.
        assert!(!triage_due(&announced, other, TRIAGE_EVERY_MS - 1));
        assert!(triage_due(&announced, other, TRIAGE_EVERY_MS + 1));

        // The same list, once the repeat window has passed: ask again. This
        // is the assertion that was false before, and it is the whole stall.
        assert!(!triage_due(&announced, list, TRIAGE_REPEAT_MS - 1));
        assert!(
            triage_due(&announced, list, TRIAGE_REPEAT_MS + 1),
            "a board that has not changed in six hours is stuck, not settled"
        );
    }

    #[test]
    fn a_held_ticket_does_not_hide_the_one_behind_it() {
        // The rig, 2026-09-01: RIG-4 sat in done with zero failures and drew
        // nothing for 25 minutes across eight ticks, because a given-up ticket
        // was ahead of it and the pass stopped rather than skipping.
        let tickets = vec![
            ticket("HELD", "done", Some("nautbot"), "2026-08-01"),
            ticket("FREE", "done", Some("nautbot"), "2026-08-02"),
        ];
        let candidates = awaiting_review(&tickets);
        assert_eq!(
            candidates.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["FREE", "HELD"],
            "both candidates are offered; neither hides the other (order is \
             freshest first since XNAUT-298, which this test does not depend on)"
        );

        // Walk them the way tick does: skip anything held, take the first free.
        let held: std::collections::HashSet<&str> = ["HELD"].into_iter().collect();
        let picked = candidates
            .iter()
            .find(|t| !held.contains(t.id.as_str()))
            .map(|t| t.id.as_str());
        assert_eq!(
            picked,
            Some("FREE"),
            "a held ticket must be skipped, not treated as the end of the queue"
        );
    }

    #[test]
    fn every_held_ticket_is_reachable_so_each_can_announce() {
        // A second held ticket never got its sweep_gave_up line, because the
        // pass never reached it.
        let tickets = vec![
            ticket("HELD1", "done", Some("nautbot"), "2026-08-01"),
            ticket("HELD2", "review", Some("nautbot"), "2026-08-02"),
            ticket("FREE", "done", Some("nautbot"), "2026-08-03"),
        ];
        let seen: Vec<&str> = awaiting_review(&tickets)
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        // Freshest first since XNAUT-298; what this test is about is that all
        // three are REACHED, not the order they are reached in.
        assert_eq!(seen.len(), 3, "every held ticket is walked: {seen:?}");
        for id in ["HELD1", "HELD2", "FREE"] {
            assert!(seen.contains(&id), "{id} is unreachable: {seen:?}");
        }
    }

    #[test]
    fn a_failed_verification_is_not_retried_every_tick() {
        // The rig's 105 identical failed records for RIG-2 in one hour. The old
        // guard asked only "is one running?", so a failure held nothing back.
        let records = vec![record("RIG-2", "failed", "2026-09-01T10:12:57+00:00")];
        assert_eq!(
            hold_for(&records, "RIG-2", at("10:15")),
            Hold::Cooling,
            "a verification that failed three minutes ago must not draw another"
        );
    }

    #[test]
    fn a_verification_that_keeps_failing_is_eventually_left_alone() {
        let records = vec![
            record("RIG-2", "failed", "2026-09-01T09:00:00+00:00"),
            record("RIG-2", "failed", "2026-09-01T09:31:00+00:00"),
            record("RIG-2", "failed", "2026-09-01T10:02:00+00:00"),
        ];
        // Past the cooldown, so only the failure count can hold it.
        assert_eq!(hold_for(&records, "RIG-2", at("10:40")), Hold::GaveUp(3));
    }

    #[test]
    fn a_green_run_re_opens_the_queue_and_a_cold_ticket_is_offered() {
        // A pass resets the count: fixing the repo must make the ticket
        // verifiable again rather than banning it forever.
        let records = vec![
            record("RIG-2", "failed", "2026-09-01T08:00:00+00:00"),
            record("RIG-2", "failed", "2026-09-01T08:31:00+00:00"),
            record("RIG-2", "passed", "2026-09-01T09:02:00+00:00"),
            record("RIG-2", "failed", "2026-09-01T09:33:00+00:00"),
        ];
        assert_eq!(hold_for(&records, "RIG-2", at("10:40")), Hold::None);
        assert_eq!(hold_for(&[], "RIG-2", at("10:40")), Hold::None, "no history, no hold");
    }

    #[test]
    fn a_running_verification_still_holds_and_a_zombie_stops_holding() {
        let fresh = vec![record("RIG-2", "running", "2026-09-01T10:14:00+00:00")];
        assert_eq!(hold_for(&fresh, "RIG-2", at("10:15")), Hold::InFlight);
        // Stuck at running because its app died: after an hour it must not
        // block the ticket forever (the zombie record of 2026-08-31).
        let zombie = vec![record("RIG-2", "running", "2026-09-01T08:00:00+00:00")];
        assert_eq!(hold_for(&zombie, "RIG-2", at("10:15")), Hold::None);
    }

    #[test]
    fn another_tickets_failures_are_not_this_ticket_s_problem() {
        let records = vec![
            record("RIG-9", "failed", "2026-09-01T10:12:00+00:00"),
            record("RIG-9", "failed", "2026-09-01T10:13:00+00:00"),
            record("RIG-9", "failed", "2026-09-01T10:14:00+00:00"),
        ];
        assert_eq!(hold_for(&records, "RIG-2", at("10:15")), Hold::None);
    }

    #[test]
    fn handbacks_are_picked_freshest_first_and_ready_needs_an_owner() {
        let tickets = vec![
            ticket("A", "ready", None, "2026-08-01"),          // no owner: skipped
            ticket("B", "ready", Some("claude"), "2026-08-03"),
            ticket("C", "ready", Some("codex"), "2026-08-02"),  // older: wins a WAKE
            ticket("D", "done", Some("nautbot"), "2026-08-05"), // newest: wins VERIFICATION
            ticket("E", "review", Some("nautbot"), "2026-08-04"),
            ticket("F", "complete", Some("nautbot"), "2026-07-01"),
        ];
        // The two halves order oppositely, on purpose. A wake goes to the owner
        // who has waited longest; a verification goes to the freshest evidence
        // (XNAUT-298), because a `done` nobody has touched since July is a
        // status somebody set, not a claim somebody just made.
        assert_eq!(awaiting_review(&tickets).first().map(|t| t.id.as_str()), Some("D"));
        assert_eq!(
            ready_with_owner(&tickets)
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["C", "B"],
            "the unowned ticket is skipped and the rest come oldest first"
        );
    }

    #[test]
    fn the_retry_queue_drains_and_dedups() {
        // The safety net's own contract: what goes in comes out once, and an
        // empty queue asks for nothing.
        assert_eq!(next_retry(), None, "starts empty");
        queue_retries(vec!["XNAUT-1".into(), "XNAUT-2".into(), "XNAUT-1".into()]);
        let first = next_retry().expect("one");
        let second = next_retry().expect("two");
        let mut got = vec![first, second];
        got.sort();
        assert_eq!(got, vec!["XNAUT-1".to_string(), "XNAUT-2".to_string()]);
        assert_eq!(next_retry(), None, "drained");
    }

    #[test]
    fn a_nudge_that_delivered_nothing_is_not_recorded_as_a_dispatch() {
        // The rig, 2026-09-01: `sweep_refused` was in the binary with zero rows
        // behind it in a 500-row ledger, because every non-delivery came back
        // as an Ok and was written down as `sweep_dispatch`. The spend ceiling
        // refusing a launch was logged as the sweep dispatching the ticket.
        //
        // The variants are serialized here rather than spelled out, because the
        // classification reads a wire string. Renaming `Launched` or `Typed` in
        // nudge.rs would otherwise silently reclassify every real dispatch as a
        // refusal; this turns red instead. A rename on the other two is safe by
        // construction, since anything unrecognised already reads as refused.
        for (delivery, expected) in [
            (crate::nudge::Delivery::Launched, "sweep_dispatch"),
            (crate::nudge::Delivery::Typed, "sweep_dispatch"),
            (crate::nudge::Delivery::SkippedBusy, "sweep_refused"),
            (crate::nudge::Delivery::NoSession, "sweep_refused"),
        ] {
            let wire = serde_json::to_value(&delivery).expect("Delivery serializes");
            let wire = wire.as_str().expect("as a plain string");
            assert_eq!(
                dispatch_kind(wire),
                expected,
                "{delivery:?} goes over the wire as {wire:?}"
            );
        }
        // A payload with no `delivery` field is not evidence of a dispatch.
        assert_eq!(dispatch_kind("unknown"), "sweep_refused");
    }

    #[test]
    fn an_empty_board_asks_for_nothing() {
        assert!(awaiting_review(&[]).is_empty());
        assert!(ready_with_owner(&[]).is_empty());
        assert!(plan_fleet(&[], &[], None, at("10:00")).is_empty());
    }

    #[test]
    fn complete_is_never_a_handback() {
        // The sweep must not re-verify finished work forever.
        assert!(!awaits_review("complete"));
        assert!(!awaits_review("in_progress"));
        assert!(awaits_review("done"));
        assert!(awaits_review("review"));
    }

    // ─── A fleet run ────────────────────────────────────────────────────────
    //
    // Several tickets, several owners, ONE board, on disk, read and written by
    // the same functions the app uses. Never ~/.xnaut-control: that is the
    // owner's real board.

    /// A scratch control repo laid out the way the real one is, seeded with a
    /// whole board rather than a single ticket. Every read below goes through
    /// `ticket_list_in` and every write through `ticket_update_in`, so what is
    /// being asserted is the real board and not a fixture of one.
    fn fleet_board(rows: &[(&str, &str, Option<&str>, &str)]) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "xnaut-fleet-{}-{n}/board",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("projects/FLEET/tickets")).unwrap();
        std::fs::create_dir_all(root.join("events")).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            std::process::Command::new("git")
                .args(&args)
                .current_dir(&root)
                .output()
                .unwrap();
        }
        // The manifest matters: `tick` reads the whole board with
        // `ticket_list_in(repo, None)`, and that enumerates PROJECTS, not
        // directories. Without a project.json the board reads as empty and the
        // sweep does nothing, which is worth knowing about a real install too.
        std::fs::write(
            root.join("projects/FLEET/project.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "key": "FLEET", "name": "Fleet rig", "revision": 1,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
            }))
            .unwrap(),
        )
        .unwrap();
        for (id, status, owner, updated) in rows {
            let body = serde_json::json!({
                "id": id, "project": "FLEET", "title": format!("work on {id}"),
                "type": "task", "status": status, "priority": "medium",
                "owner": owner, "documentation": [], "body": "the work",
                "source_id": "", "revision": 1,
                "created_at": "2026-01-01T00:00:00Z", "updated_at": updated,
            });
            std::fs::write(
                root.join(format!("projects/FLEET/tickets/{id}.json")),
                serde_json::to_string_pretty(&body).unwrap(),
            )
            .unwrap();
        }
        root
    }

    fn on_board(repo: &std::path::Path, id: &str) -> TicketRecord {
        crate::project_management::ticket_list_in(repo, Some("FLEET".into()))
            .unwrap()
            .into_iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("{id} is on the board"))
    }

    /// The whole point of the sprint: several agents working one board at once,
    /// unattended, and the board afterwards saying what happened.
    ///
    /// What is real here: the board (on disk, read by `ticket_list_in`), the
    /// planner, the ledger (a real jsonl file), the ticket writes (through
    /// `ticket_update_in`, revision checks, git commits and all), and the
    /// report. What is NOT real is the sandbox: `run_verify` warms a GitVM or
    /// exe.dev VM, so the verdicts are constructed `VerifyRecord`s rather than
    /// runs. `live_fleet_run_closes_two_tickets_at_once` below drives the same
    /// path with real ones and is ignored by default; the shape of the record is
    /// asserted there.
    ///
    /// Before this change the same board produced ONE action per tick and never
    /// a dispatch at all, because the first handback returned out of `tick`.
    #[test]
    fn a_fleet_works_several_tickets_for_several_owners_on_one_board() {
        let _guard = crate::ledger::scratch("fleet-run");
        let repo = fleet_board(&[
            // Ready work for three owners; two of the tickets share an owner.
            ("FLEET-1", "ready", Some("claude"), "2026-09-01T09:00:00Z"),
            ("FLEET-2", "ready", Some("codex"), "2026-09-01T09:01:00Z"),
            ("FLEET-3", "ready", Some("claude"), "2026-09-01T09:02:00Z"),
            ("FLEET-4", "ready", Some("ralph"), "2026-09-01T09:03:00Z"),
            // Nobody owns this one, so nobody can be woken for it.
            ("FLEET-5", "ready", None, "2026-09-01T08:00:00Z"),
            // Two handbacks nobody has looked at.
            ("FLEET-6", "done", Some("nautbot"), "2026-09-01T07:00:00Z"),
            ("FLEET-7", "review", Some("nautbot"), "2026-09-01T07:30:00Z"),
            // Finished work: never offered again.
            ("FLEET-8", "complete", Some("nautbot"), "2026-09-01T06:00:00Z"),
        ]);

        // The real board read, not a fixture.
        let tickets = crate::project_management::ticket_list_in(&repo, None).unwrap();
        assert_eq!(tickets.len(), 8, "the whole board is there");

        let plan = plan_fleet(&tickets, &[], None, at("10:00"));

        // Every owner with ready work is woken, each exactly once, oldest
        // ticket first. This is the fleet: three agents in one tick.
        let dispatched: Vec<(&str, &str)> = plan
            .iter()
            .filter_map(|a| match a {
                Action::Dispatch { ticket, owner, .. } => {
                    Some((owner.as_str(), ticket.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            dispatched,
            vec![
                ("claude", "FLEET-1"),
                ("codex", "FLEET-2"),
                ("ralph", "FLEET-4"),
            ],
            "three owners woken, claude gets the older of its two, the unowned \
             ticket wakes nobody: {plan:?}"
        );

        // The older handback is verified in this tick and dispatch still
        // happens beside it. FLEET-7 is the same project, so it waits for the
        // next pass rather than colliding in FLEET's one directory
        // (XNAUT-287); it is not starved, and the dispatches below prove the
        // wait does not block the other half of the board.
        let verifying: Vec<&str> = plan
            .iter()
            .filter_map(|a| match a {
                Action::Verify { ticket, .. } => Some(ticket.as_str()),
                _ => None,
            })
            .collect();
        // FLEET-7 is the fresher handback of the two (XNAUT-298); FLEET-6
        // waits for the next pass, which the second tick below proves.
        assert_eq!(verifying, vec!["FLEET-7"], "{plan:?}");
        assert!(
            !plan.iter().any(|a| matches!(
                a,
                Action::Verify { ticket, .. } | Action::Dispatch { ticket, .. } if ticket == "FLEET-8"
            )),
            "finished work is never picked up again"
        );

        // Now run the plan's verification half against the real board: FLEET-6
        // comes back green, FLEET-7 red. The ledger gets the rows `run_action`
        // writes, so the report has something to join.
        for action in &plan {
            match action {
                Action::Verify { ticket, status, .. } => crate::ledger::record(
                    "sweep_verify",
                    "nautbot",
                    ticket,
                    &format!("{ticket} sat in {status} unreviewed"),
                ),
                Action::Dispatch { ticket, owner, .. } => {
                    crate::ledger::record("sweep_dispatch", owner, ticket, "launched")
                }
                other => panic!("unexpected action in this plan: {other:?}"),
            }
        }
        let green = verdict("FLEET-7", "passed", 0);
        // The next pass, with FLEET-7's run finished, picks up FLEET-6: the
        // one-per-project rule (XNAUT-287) delays the second handback by a
        // tick, it does not drop it.
        let second = plan_fleet(&tickets, std::slice::from_ref(&green), None, at("10:03"));
        let verifying: Vec<&str> = second
            .iter()
            .filter_map(|a| match a {
                Action::Verify { ticket, .. } => Some(ticket.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(verifying, vec!["FLEET-6"], "{second:?}");
        crate::ledger::record("sweep_verify", "nautbot", "FLEET-6", "FLEET-6 sat in done unreviewed");
        let red = verdict("FLEET-6", "failed", 1);
        crate::sandbox_verify::settle_ticket_in(&repo, &green)
            .unwrap()
            .expect("a green run closes its ticket");
        assert!(
            crate::sandbox_verify::settle_ticket_in(&repo, &red)
                .unwrap()
                .is_none(),
            "a red run moves nothing"
        );

        // THE BOARD AFTERWARDS. Read back off disk, because that is what the
        // next tick and the owner's panel both read.
        assert_eq!(
            on_board(&repo, "FLEET-7").status,
            "complete",
            "the verified ticket closed"
        );
        assert!(
            on_board(&repo, "FLEET-7")
                .body
                .contains(&format!("Record: `{}`", green.id)),
            "and carries the evidence that closed it"
        );
        assert_eq!(
            on_board(&repo, "FLEET-6").status,
            "done",
            "the ticket whose verification failed stayed exactly where it was"
        );
        for id in ["FLEET-1", "FLEET-2", "FLEET-3", "FLEET-4", "FLEET-5"] {
            assert_eq!(
                on_board(&repo, id).status,
                "ready",
                "{id}: dispatching a ticket does not move it; the agent does"
            );
        }

        // AND THE RUN IS LEGIBLE. One row per ticket the sweep touched, saying
        // who got it, what ran, and whether anything verified the close.
        let report = fleet_report(
            &crate::project_management::ticket_list_in(&repo, None).unwrap(),
            &crate::ledger::ledger_recent(Some(500)),
            &[green.clone(), red.clone()],
        );
        let by_id = |id: &str| {
            report
                .iter()
                .find(|r| r.ticket == id)
                .unwrap_or_else(|| panic!("{id} is in the report: {report:?}"))
                .clone()
        };
        assert_eq!(
            report.len(),
            5,
            "the five tickets the sweep acted on, and only those: {report:?}"
        );
        assert_eq!(by_id("FLEET-1").owners, vec!["claude".to_string()]);
        assert_eq!(by_id("FLEET-7").verdict, "passed");
        assert!(by_id("FLEET-7").verified, "closed on the strength of a run");
        assert!(!by_id("FLEET-7").unverified_close);
        assert_eq!(by_id("FLEET-6").verdict, "failed");
        assert!(
            !by_id("FLEET-6").verified && !by_id("FLEET-6").unverified_close,
            "an open ticket is neither verified nor a bad close"
        );
        assert!(
            report.iter().all(|r| !r.unverified_close),
            "nothing closed that nothing verified: {report:?}"
        );

        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    /// The failure the whole thing is aimed at: a board that LOOKS finished.
    ///
    /// A ticket at `complete` with no green run behind it is what "it looks like
    /// it worked" looks like on disk, and it is indistinguishable from a real
    /// close unless someone joins the board to the records. The report has to
    /// name it, or a fleet that completed nothing reads exactly like one that
    /// completed everything.
    #[test]
    fn a_close_that_nothing_verified_is_called_out() {
        let _guard = crate::ledger::scratch("fleet-unverified");
        let repo = fleet_board(&[
            ("FLEET-1", "complete", Some("nautbot"), "2026-09-01T09:00:00Z"),
            ("FLEET-2", "complete", Some("nautbot"), "2026-09-01T09:01:00Z"),
        ]);
        crate::ledger::record("sweep_verify", "nautbot", "FLEET-1", "sat in done");
        crate::ledger::record("sweep_verify", "nautbot", "FLEET-2", "sat in done");

        let report = fleet_report(
            &crate::project_management::ticket_list_in(&repo, None).unwrap(),
            &crate::ledger::ledger_recent(Some(500)),
            // FLEET-1 has a green run behind it. FLEET-2 has a RED one and is
            // sitting at complete anyway: something closed a ticket that failed.
            &[verdict("FLEET-1", "passed", 0), verdict("FLEET-2", "failed", 1)],
        );
        let row = |id: &str| report.iter().find(|r| r.ticket == id).unwrap();
        assert!(row("FLEET-1").verified && !row("FLEET-1").unverified_close);
        assert!(
            row("FLEET-2").unverified_close && !row("FLEET-2").verified,
            "a complete with a red verdict behind it is a bad close: {report:?}"
        );

        // And the case with no record at all, which is the commoner one: a
        // ticket somebody moved to complete by hand while the fleet was running.
        let none = fleet_report(
            &crate::project_management::ticket_list_in(&repo, None).unwrap(),
            &crate::ledger::ledger_recent(Some(500)),
            &[],
        );
        assert!(
            none.iter().all(|r| r.unverified_close && r.verdict.is_empty()),
            "no verification ran at all, so neither close is backed: {none:?}"
        );

        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    /// The same fleet run with the sandbox real: two handbacks on one board,
    /// both verified against a live exe.dev VM in the same tick, one green and
    /// one red, and the board afterwards showing exactly one close.
    ///
    /// This is the half `a_fleet_works_several_tickets_for_several_owners_on_one_board`
    /// cannot reach from `cargo test` without a network, and it is what makes
    /// the constructed verdicts there legitimate rather than a mock: the record
    /// shape those assert on is produced HERE by `run_verify` itself.
    ///
    /// Ignored because it needs the owner's registered exe.dev ssh key. Run it:
    ///   cargo test --bin xnaut live_fleet_run -- --ignored --nocapture
    ///
    /// The dispatch half is still not exercised: waking an agent needs an
    /// `AppHandle` and a live zellij, neither of which exists under `cargo
    /// test`. What that half's ceiling does is asserted separately, against the
    /// real `spend` module.
    #[tokio::test]
    #[ignore]
    async fn live_fleet_run_verifies_two_tickets_in_one_tick() {
        let _guard = crate::ledger::scratch("live-fleet");
        // Two checkouts, because the two tickets are different work: one whose
        // assertion holds and one whose does not.
        let repo_for = |expected: &str| {
            let dir = std::env::temp_dir().join(format!(
                "xnaut-live-fleet-{}-{expected}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join(".xnaut")).unwrap();
            std::fs::write(
                dir.join(".xnaut/verify.json"),
                r#"{"provider":"exe-dev","install":"chmod +x sum.sh test.sh","test":"sh ./test.sh"}"#,
            )
            .unwrap();
            std::fs::write(dir.join("sum.sh"), "#!/bin/sh\necho $(( $1 + $2 ))\n").unwrap();
            std::fs::write(
                dir.join("test.sh"),
                format!(
                    "#!/bin/sh\nset -e\ngot=$(sh ./sum.sh 2 3)\n[ \"$got\" = \"{expected}\" ] || \
                     {{ echo \"FAIL: got $got want {expected}\"; exit 1; }}\necho PASS\n"
                ),
            )
            .unwrap();
            dir
        };
        let good = repo_for("5");
        let bad = repo_for("6");
        // FLEET-1's own branch: without it the green is a run about a tree
        // nobody tied to the ticket, and it settles nothing (XNAUT-294).
        std::process::Command::new("git")
            .args(["init", "-b", "fix/fleet-1-live"])
            .current_dir(&good)
            .output()
            .unwrap();

        let board = fleet_board(&[
            ("FLEET-1", "done", Some("nautbot"), "2026-09-01T07:00:00Z"),
            ("FLEET-2", "review", Some("nautbot"), "2026-09-01T07:30:00Z"),
        ]);
        let tickets = crate::project_management::ticket_list_in(&board, None).unwrap();
        let plan = plan_fleet(&tickets, &[], None, chrono::Utc::now());
        let verifying: Vec<&str> = plan
            .iter()
            .filter_map(|a| match a {
                Action::Verify { ticket, .. } => Some(ticket.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            verifying,
            vec!["FLEET-1", "FLEET-2"],
            "one tick, both handbacks: {plan:?}"
        );

        // Run them CONCURRENTLY, which is what `run_action` spawning does.
        let run = |dir: std::path::PathBuf, id: &'static str| async move {
            let (config, steps) = crate::sandbox_verify::load_verify_plan(&dir).unwrap();
            crate::sandbox_verify::run_verify(None, &dir, id, "FLEET", "live", &config, &steps)
                .await
                .expect("the run produced a verdict")
        };
        let (green, red) = tokio::join!(run(good.clone(), "FLEET-1"), run(bad.clone(), "FLEET-2"));
        println!("{}", serde_json::to_string_pretty(&green).unwrap());
        println!("{}", serde_json::to_string_pretty(&red).unwrap());
        assert_eq!(green.status, "passed", "error={:?}", green.error);
        assert_eq!(red.status, "failed", "a broken assertion must be red");

        crate::sandbox_verify::settle_ticket_in(&board, &green)
            .unwrap()
            .expect("the green one closes");
        assert!(crate::sandbox_verify::settle_ticket_in(&board, &red)
            .unwrap()
            .is_none());
        crate::ledger::record("sweep_verify", "nautbot", "FLEET-1", "sat in done");
        crate::ledger::record("sweep_verify", "nautbot", "FLEET-2", "sat in review");

        assert_eq!(on_board(&board, "FLEET-1").status, "complete");
        assert_eq!(on_board(&board, "FLEET-2").status, "review");

        let report = fleet_report(
            &crate::project_management::ticket_list_in(&board, None).unwrap(),
            &crate::ledger::ledger_recent(Some(500)),
            &[green, red],
        );
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        assert!(
            report.iter().filter(|r| r.verified).count() == 1
                && report.iter().all(|r| !r.unverified_close),
            "one close, and a real run behind it: {report:?}"
        );

        for dir in [good, bad] {
            let _ = std::fs::remove_dir_all(dir);
        }
        let _ = std::fs::remove_dir_all(board.parent().unwrap());
    }

    /// A checkout on a branch that names the ticket. A green run only settles
    /// anything when the tree it ran is evidence about that ticket
    /// (XNAUT-294), and the branch is the cheapest evidence there is.
    fn tree_for(ticket: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xnaut-fleet-tree-{}-{ticket}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .args(["init", "-b", &format!("fix/{}-live", ticket.to_lowercase())])
            .current_dir(&dir)
            .output()
            .unwrap();
        dir
    }

    fn verdict(ticket: &str, status: &str, exit: i32) -> crate::sandbox_verify::VerifyRecord {
        crate::sandbox_verify::VerifyRecord {
            id: format!("rec-{ticket}-{status}"),
            project: "FLEET".into(),
            not_evidence: false,
            repo_path: tree_for(ticket).to_string_lossy().into_owned(),
            steps: vec![crate::sandbox_verify::VerifyStep {
                name: "test".into(),
                command: "sh ./test.sh".into(),
                exit_code: Some(exit),
                log_tail: String::new(),
                started_at: String::new(),
                duration_ms: 0,
            }],
            ..record(ticket, status, "2026-09-01T10:00:00+00:00")
        }
    }

    // ─── The ceiling still holds ────────────────────────────────────────────

    /// A fleet plan is a list of intentions, not a licence to spend.
    ///
    /// The plan above wanted three agents woken. `spend::admit_launch` is what
    /// decides how many actually launch, and it is asked ONCE PER LAUNCH with
    /// the live count as it stands at that moment, which is why `run_action`
    /// awaits its dispatches in turn rather than spawning them. This drives the
    /// real spend module against a scratch config dir: a fleet of five is
    /// admitted twice and refused three times, in the ceiling's own words.
    #[test]
    fn the_concurrent_cap_still_refuses_a_fleet_that_wants_more() {
        // The spend store is reached through a process-global env var; this is
        // the lock spend's own tests queue on.
        let (_guard, dir) = crate::spend::scratch("fleet-concurrent");
        crate::spend::spend_ceiling_set(crate::spend::SpendCeiling {
            max_concurrent: 2,
            max_daily_launches: 20,
        ..Default::default()
        })
        .unwrap();

        // Five owners, five cold launches. `live` grows only when one is
        // admitted, exactly as the session map grows behind an awaited launch.
        let mut live = 0usize;
        let mut refusals = Vec::new();
        for _ in 0..5 {
            match crate::spend::admit_launch(live) {
                Ok(()) => live += 1,
                Err(why) => refusals.push(why),
            }
        }
        assert_eq!(live, 2, "the cap admitted two and no more");
        assert_eq!(refusals.len(), 3);
        assert!(
            refusals.iter().all(|r| r.contains("concurrent cap is 2")),
            "and said why each time: {refusals:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other brake, and the one that matters for an unattended overnight
    /// run: crossing the daily cap engages `read_only`, and `tick` does nothing
    /// whatsoever while that switch is up. Raising the fleet's dispatch count
    /// must not have loosened either.
    #[test]
    fn the_daily_cap_stops_the_fleet_and_engages_the_kill_switch() {
        let (_guard, dir) = crate::spend::scratch("fleet-daily");
        crate::spend::spend_ceiling_set(crate::spend::SpendCeiling {
            max_concurrent: 50, // out of the way; the DAILY cap is under test
            max_daily_launches: 2,
        ..Default::default()
        })
        .unwrap();
        assert!(!crate::switches::load().read_only, "starts lifted");

        assert!(crate::spend::admit_launch(0).is_ok());
        assert!(crate::spend::admit_launch(0).is_ok());
        let stopped = crate::spend::admit_launch(0).unwrap_err();
        assert!(stopped.contains("daily cap of 2"), "{stopped}");
        assert!(
            crate::switches::load().read_only,
            "crossing the daily cap engages the kill switch, which is what \
             `tick` checks before it reads the board at all"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Verifications have their own ceiling because nothing else gives them one:
    /// they warm sandboxes, not agent sessions, so `admit_launch` never sees
    /// them. Three in flight means a fourth waits, however long the queue is.
    #[test]
    fn verifications_are_capped_by_how_many_are_live_not_by_the_tick() {
        // Six projects, one handback each: the ceiling is what limits them, not
        // the one-per-project rule (XNAUT-287), which has its own test.
        let tickets: Vec<TicketRecord> = (1..=6)
            .map(|n| {
                let mut t = ticket(
                    &format!("F-{n}"),
                    "done",
                    Some("nautbot"),
                    &format!("2026-09-01T0{n}:00:00Z"),
                );
                t.project = format!("P{n}");
                t
            })
            .collect();

        // Nothing running: the budget starts full and stops at it.
        let plan = plan_fleet(&tickets, &[], None, at("10:00"));
        let verifying: Vec<&str> = plan
            .iter()
            .filter_map(|a| match a {
                Action::Verify { ticket, .. } => Some(ticket.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            verifying.len(),
            MAX_VERIFIES_IN_FLIGHT,
            "six handbacks, three started: {plan:?}"
        );
        // Freshest first since XNAUT-298; the subject here is that exactly
        // MAX_VERIFIES_IN_FLIGHT are started, not which three.
        assert_eq!(verifying, vec!["F-6", "F-5", "F-4"], "freshest first");

        // Two already running: only one more may start, and never the ones that
        // are already going.
        let running = vec![
            record("F-1", "running", "2026-09-01T09:59:00+00:00"),
            record("F-2", "running", "2026-09-01T09:58:00+00:00"),
        ];
        let plan = plan_fleet(&tickets, &running, None, at("10:00"));
        let verifying: Vec<&str> = plan
            .iter()
            .filter_map(|a| match a {
                Action::Verify { ticket, .. } => Some(ticket.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(verifying, vec!["F-6"], "one slot left, freshest takes it: {plan:?}");

        // Full: nothing starts. A tick with no room is not a tick that gives up.
        let running = vec![
            record("F-1", "running", "2026-09-01T09:59:00+00:00"),
            record("F-2", "running", "2026-09-01T09:58:00+00:00"),
            record("F-3", "running", "2026-09-01T09:57:00+00:00"),
        ];
        let plan = plan_fleet(&tickets, &running, None, at("10:00"));
        assert!(
            !plan.iter().any(|a| matches!(a, Action::Verify { .. })),
            "the budget is spent: {plan:?}"
        );

        // Three ZOMBIE records, stuck at running since an hour ago, must not
        // hold the budget hostage; that is the 2026-08-31 record, multiplied.
        let zombies = vec![
            record("F-1", "running", "2026-09-01T06:00:00+00:00"),
            record("F-2", "running", "2026-09-01T06:00:00+00:00"),
            record("F-3", "running", "2026-09-01T06:00:00+00:00"),
        ];
        let plan = plan_fleet(&tickets, &zombies, None, at("10:00"));
        assert_eq!(
            plan.iter()
                .filter(|a| matches!(a, Action::Verify { .. }))
                .count(),
            MAX_VERIFIES_IN_FLIGHT,
            "a dead run holds nothing: {plan:?}"
        );
    }

    /// Two things the old early-return conflated, separated.
    ///
    /// A held ticket must not stop the fleet from being dispatched, and a spent
    /// verification budget must not silence the give-ups behind it. Both used to
    /// be true only by accident of where `return Ok(())` sat.
    #[test]
    fn unreviewed_work_no_longer_starves_the_dispatch_half() {
        let tickets = vec![
            ticket("HELD", "done", Some("nautbot"), "2026-09-01T07:00:00Z"),
            ticket("READY", "ready", Some("claude"), "2026-09-01T08:00:00Z"),
        ];
        // HELD has failed its way out of the queue, so it draws nothing but an
        // announcement. Before, `tick` reached the dispatch half only when the
        // handback loop fell through, and it did here; the real starvation was
        // any handback that was FREE, which returned before dispatch every time.
        let held = vec![
            record("HELD", "failed", "2026-09-01T06:00:00+00:00"),
            record("HELD", "failed", "2026-09-01T06:31:00+00:00"),
            record("HELD", "failed", "2026-09-01T07:02:00+00:00"),
        ];
        let plan = plan_fleet(&tickets, &held, None, at("10:00"));
        assert!(plan.contains(&Action::GaveUp {
            ticket: "HELD".into(),
            failures: 3
        }));
        assert!(
            plan.contains(&Action::Dispatch {
                ticket: "READY".into(),
                owner: "claude".into(),
                title: "t".into()
            }),
            "the agent is woken anyway: {plan:?}"
        );

        // Now the case that really starved: a FREE handback plus ready work.
        let plan = plan_fleet(&tickets, &[], None, at("10:00"));
        assert!(
            plan.iter().any(|a| matches!(a, Action::Verify { .. })),
            "the handback is verified"
        );
        assert!(
            plan.iter().any(|a| matches!(a, Action::Dispatch { .. })),
            "AND the agent is dispatched in the same tick. This is the line that \
             made a fleet run impossible: {plan:?}"
        );
    }

    /// Give-ups are announced even when there is no room to act on anything.
    #[test]
    fn a_spent_budget_does_not_silence_the_announcements() {
        let tickets: Vec<TicketRecord> = (1..=4)
            .map(|n| {
                ticket(
                    &format!("F-{n}"),
                    "done",
                    Some("nautbot"),
                    &format!("2026-09-01T0{n}:00:00Z"),
                )
            })
            .collect();
        let mut records = vec![
            record("F-1", "running", "2026-09-01T09:59:00+00:00"),
            record("F-2", "running", "2026-09-01T09:58:00+00:00"),
            record("F-3", "running", "2026-09-01T09:57:00+00:00"),
        ];
        for hhmm in ["06:00", "06:31", "07:02"] {
            records.push(record("F-4", "failed", &format!("2026-09-01T{hhmm}:00+00:00")));
        }
        let plan = plan_fleet(&tickets, &records, None, at("10:00"));
        assert_eq!(
            plan,
            vec![Action::GaveUp {
                ticket: "F-4".into(),
                failures: 3
            }],
            "no room to verify anything, and the news still gets out: {plan:?}"
        );
    }

    /// A fleet that wakes four owners must not write four refusals every tick.
    ///
    /// This is a cost the fleet introduced and the rig had already measured the
    /// shape of: one owner per tick meant one refusal row per tick, and a ready
    /// ticket stays ready until its agent moves it, so four working agents would
    /// write four `skipped_busy` rows every 180 seconds forever. 145 identical
    /// rows burying every real entry is what that looks like after a night.
    #[test]
    fn a_standing_refusal_is_recorded_once_and_a_delivery_is_always_news() {
        let mut announced = Announced::default();
        let key = || "claude:FLEET-1".to_string();

        // Twenty ticks of the same agent being mid-turn: one row.
        assert!(
            announced.dispatch_is_news(key(), "sweep_refused", "skipped_busy: work on FLEET-1"),
            "the first refusal is news"
        );
        for _ in 0..20 {
            assert!(
                !announced.dispatch_is_news(key(), "sweep_refused", "skipped_busy: work on FLEET-1"),
                "and the twenty after it are not"
            );
        }

        // A DIFFERENT refusal is news again: busy and off-the-roster are not the
        // same fact, and collapsing them would hide the one worth acting on.
        assert!(announced.dispatch_is_news(key(), "sweep_refused", "claude is not on the roster"));
        assert!(!announced.dispatch_is_news(key(), "sweep_refused", "claude is not on the roster"));

        // A delivery is ALWAYS written down; the elapsed clock starts from one.
        assert!(announced.dispatch_is_news(key(), "sweep_dispatch", "launched: work on FLEET-1"));
        assert!(announced.dispatch_is_news(key(), "sweep_dispatch", "launched: work on FLEET-1"));

        // And after real work moved, the SAME refusal that was standing before
        // it is news again rather than being swallowed by an hour-old row.
        // Asserted with the reason that was stored last, on purpose: a different
        // reason would be news anyway and would prove nothing about the clear.
        assert!(
            announced.dispatch_is_news(key(), "sweep_refused", "claude is not on the roster"),
            "a delivery clears the standing refusal"
        );

        // Another owner's refusal is not this one's.
        assert!(announced.dispatch_is_news(
            "codex:FLEET-2".into(),
            "sweep_refused",
            "skipped_busy: work on FLEET-1"
        ));
    }

    /// A workstation never dispatches, and SAYS SO ONCE (XNAUT-370).
    ///
    /// Both halves matter and they pull against each other. The line has to
    /// name the role and the build, or it does not answer the question the
    /// ticket is about — which of two machines, on which version, refused. And
    /// it has to be byte-identical from one tick to the next, or the "once"
    /// rule above cannot suppress it and the ledger grows a row every three
    /// minutes for as long as the owner's Mac is awake.
    #[test]
    fn a_workstation_refuses_in_the_same_words_every_tick_and_names_which_machine() {
        use crate::instance::Role;
        let why = refusal("dispatch", Role::Workstation);
        assert!(why.contains("workstation"), "the line names the role: {why}");
        assert!(
            why.contains(env!("CARGO_PKG_VERSION")),
            "and the build, which is the other half of the drift: {why}"
        );
        assert_eq!(why, refusal("dispatch", Role::Workstation), "no clock, no counter");

        let mut announced = Announced::default();
        assert!(announced.dispatch_is_news("here".into(), "sweep_refused", &why));
        for _ in 0..20 {
            assert!(
                !announced.dispatch_is_news("here".into(), "sweep_refused", &why),
                "said once, not once a tick"
            );
        }

        // Refusing to verify is a SEPARATE standing fact under its own key, so
        // a machine that does neither says both once rather than one of them
        // silencing the other.
        let no_verify = refusal("verify", Role::Workstation);
        assert_ne!(no_verify, why);
        assert!(announced.dispatch_is_news("verify-here".into(), "sweep_refused", &no_verify));

        // A sandbox box refuses to dispatch and says nothing about verifying,
        // because it does verify.
        assert!(!Role::Sandbox.dispatches() && Role::Sandbox.verifies());
    }

    /// gitvm is directory-scoped and every handback without its own worktree
    /// verifies in its project's source_path. Two offered in one pass collide
    /// (XNAUT-287, 2026-09-05: the second died at "already warm" and was
    /// charged to a ticket that did nothing wrong). One per project per pass,
    /// none while the project has a run live, and a retry counts as one.
    #[test]
    fn one_verification_per_project_per_pass() {
        let mut tickets = vec![
            ticket("A-1", "done", Some("nautbot"), "2026-09-01T01:00:00Z"),
            ticket("A-2", "done", Some("nautbot"), "2026-09-01T02:00:00Z"),
            ticket("B-1", "done", Some("nautbot"), "2026-09-01T03:00:00Z"),
        ];
        tickets[2].project = "B".into();
        let names = |plan: &[Action]| -> Vec<String> {
            plan.iter()
                .filter_map(|a| match a {
                    Action::Verify { ticket, .. } | Action::Retry { ticket, .. } => Some(ticket.clone()),
                    _ => None,
                })
                .collect()
        };

        let plan = plan_fleet(&tickets, &[], None, at("10:00"));
        // A-2 is the fresher of the two XNAUT tickets (XNAUT-298), so it goes
        // and A-1 waits for the next pass. Which of the two is picked is not
        // this test's subject; that ONE of them is, alongside B, is.
        assert_eq!(names(&plan), vec!["B-1", "A-2"], "one per project: {plan:?}");

        let mut live = record("A-1", "running", "2026-09-01T09:59:00+00:00");
        live.project = "XNAUT".into();
        let plan = plan_fleet(&tickets, &[live], None, at("10:00"));
        assert_eq!(names(&plan), vec!["B-1"], "XNAUT is busy, B is not: {plan:?}");

        let plan = plan_fleet(&tickets, &[], Some("A-2"), at("10:00"));
        assert_eq!(names(&plan), vec!["A-2", "B-1"], "a retry holds its project: {plan:?}");
    }

    fn handback_with(commits: Vec<String>) -> crate::handback::Handback {
        crate::handback::Handback {
            run_id: None,
            ticket: "NEW-1".into(),
            summary: "did the thing".into(),
            files_changed: vec!["src/lib.rs".into()],
            commits,
            how_verified: "cargo test: 900 passed".into(),
            verify_record_id: None,
            not_finished: Some("nothing".into()),
            confidence: crate::handback::Confidence::High,
            from: "claude".into(),
            submitted_at: "2026-09-01T09:30:00+00:00".into(),
        }
    }

    /// A handback naming commits is verified before a years-old `done` with
    /// none, however long that one has waited (XNAUT-298).
    #[test]
    fn a_fresh_handback_is_verified_before_an_ancient_done() {
        let mut ancient = ticket("OLD-1", "done", Some("nautbot"), "2026-07-12T09:00:00Z");
        let mut older = ticket("OLD-2", "done", Some("nautbot"), "2026-07-01T09:00:00Z");
        let mut fresh = ticket("NEW-1", "done", Some("claude"), "2026-09-01T09:30:00Z");
        let mut fresh_no_commits = ticket("NEW-2", "done", Some("claude"), "2026-09-01T09:40:00Z");
        fresh.handback = Some(handback_with(vec!["d95e14c".into()]));
        // A handback with no commits is not evidence, so it sorts by date only.
        fresh_no_commits.handback = Some(handback_with(vec![]));
        ancient.handback = None;
        older.handback = None;
        let board = vec![older, ancient, fresh_no_commits, fresh];
        let order: Vec<&str> = awaiting_review(&board).iter().map(|t| t.id.as_str()).collect();
        assert_eq!(order[0], "NEW-1", "the handback with commits leads: {order:?}");
        assert_eq!(
            order,
            vec!["NEW-1", "NEW-2", "OLD-1", "OLD-2"],
            "then most recently updated, newest first: {order:?}"
        );
    }

    /// A green run that was not evidence about the ticket (XNAUT-294) is not a
    /// strike and is not re-run: the same tree next pass would say the same.
    #[test]
    fn a_not_evidence_pass_holds_the_ticket_without_a_strike() {
        let mut r = record("A-1", "passed", "2026-09-01T09:00:00+00:00");
        r.not_evidence = true;
        let failed = [
            record("A-1", "failed", "2026-09-01T06:00:00+00:00"),
            record("A-1", "failed", "2026-09-01T07:00:00+00:00"),
            record("A-1", "failed", "2026-09-01T08:00:00+00:00"),
            r,
        ];
        assert_eq!(hold_for(&failed, "A-1", at("10:00")), Hold::NotEvidence, "held, not given up");
        let tickets = vec![ticket("A-1", "done", Some("nautbot"), "2026-09-01T01:00:00Z")];
        let plan = plan_fleet(&tickets, &failed, None, at("10:00"));
        assert!(
            !plan.iter().any(|a| matches!(a, Action::Verify { .. } | Action::GaveUp { .. })),
            "neither re-verified nor abandoned: {plan:?}"
        );
    }

    /// The step the board stalled on: ready tickets nobody owns. Urgent ones
    /// wake NautBot to triage, oldest first, capped; the rest wait.
    #[test]
    fn unowned_urgent_ready_tickets_wake_nautbot_to_triage() {
        let mut tickets = vec![
            ticket("U-1", "ready", None, "2026-09-01T01:00:00Z"),
            ticket("U-2", "ready", None, "2026-09-01T02:00:00Z"),
            ticket("U-3", "ready", None, "2026-09-01T03:00:00Z"),
            ticket("O-1", "ready", Some("claude"), "2026-09-01T04:00:00Z"),
        ];
        tickets[0].priority = "critical".into();
        tickets[1].priority = "low".into();
        tickets[2].priority = "high".into();
        let plan = plan_fleet(&tickets, &[], None, at("10:00"));
        let triage = plan.iter().find_map(|a| match a { Action::Triage { tickets } => Some(tickets.clone()), _ => None }).expect("a triage wake");
        assert_eq!(triage.len(), 2, "U-2 is low priority and waits: {triage:?}");
        assert!(triage[0].starts_with("U-1:") && triage[1].starts_with("U-3:"), "oldest first: {triage:?}");
        assert!(plan.iter().any(|a| matches!(a, Action::Dispatch { ticket, .. } if ticket == "O-1")), "owned work still dispatches");
        let none = plan_fleet(&[ticket("O-1", "ready", Some("claude"), "2026-09-01T04:00:00Z")], &[], None, at("10:00"));
        assert!(!none.iter().any(|a| matches!(a, Action::Triage { .. })), "nothing to triage, no wake");
    }

    #[test]
    fn fresh_unowned_in_progress_urgent_tickets_are_triaged() {
        for priority in ["high", "critical"] {
            for owner in [None, Some(""), Some("  ")] {
                let mut t = ticket("XNAUT-306", "in_progress", owner, "2026-09-01T09:00:00Z");
                t.priority = priority.into();
                let plan = plan_fleet(&[t], &[], None, at("10:00"));
                assert_eq!(plan, vec![Action::Triage { tickets: vec!["XNAUT-306: t".into()] }]);
            }
        }
    }

    #[test]
    fn stale_unowned_in_progress_is_notified_and_never_dispatched() {
        let now = at("10:00");
        let mut stale = ticket("XNAUT-306", "in_progress", None,
            &(now - chrono::Duration::weeks(6)).to_rfc3339());
        stale.priority = "high".into();
        stale.title = "An abandoned ticket".into();
        let plan = plan_fleet(&[stale], &[], None, now);
        let expected = vec![("XNAUT-306".into(), "An abandoned ticket".into())];
        assert_eq!(plan, vec![Action::NotifyStaleUnowned { tickets: expected.clone() }]);
        let notice = stale_unowned_notice(&expected);
        assert!(notice.body.contains("- XNAUT-306: An abandoned ticket"));
        assert!(notice.body.contains("not been dispatched or assigned"));
    }

    #[test]
    fn owned_in_progress_is_untouched_and_fleet_scope_is_preserved() {
        for age in [1, 42] {
            let mut t = ticket("XNAUT-306", "in_progress", Some("codex"),
                &(at("10:00") - chrono::Duration::days(age)).to_rfc3339());
            t.priority = "critical".into();
            assert!(plan_fleet(&[t.clone()], &[], None, at("10:00")).is_empty());
            t.owner = None;
            assert!(plan_fleet_for(&[t], &[], None, at("10:00"),
                &Fleet::Only(std::collections::HashSet::new())).is_empty());
        }
    }

    #[test]
    fn ready_and_in_progress_share_the_existing_freshness_boundary() {
        let now = at("10:00");
        for status in ["ready", "in_progress"] {
            let mut t = ticket("XNAUT-306", status, None,
                &(now - chrono::Duration::days(FRESH_DAYS)).to_rfc3339());
            t.priority = "high".into();
            assert_eq!(plan_fleet(&[t.clone()], &[], None, now),
                vec![Action::Triage { tickets: vec!["XNAUT-306: t".into()] }]);
            for updated in [(now - chrono::Duration::days(FRESH_DAYS) - chrono::Duration::seconds(1)).to_rfc3339(), "invalid".into()] {
                t.updated_at = updated;
                assert_eq!(plan_fleet(&[t.clone()], &[], None, now),
                    vec![Action::NotifyStaleUnowned { tickets: vec![("XNAUT-306".into(), "t".into())] }]);
            }
            t.updated_at = now.to_rfc3339();
            t.priority = "low".into();
            assert!(plan_fleet(&[t], &[], None, now).is_empty());
        }
    }

    /// The notice is posted once per SET, not once per pass (XNAUT-310). The
    /// same three tickets fired at 12:27, 13:28 and 14:11 on 2026-09-09 with
    /// identical content.
    #[test]
    fn stale_notify_is_once_per_set_and_says_it_again_when_the_set_changes() {
        let a: (String, String) = ("XNAUT-306".into(), "Old work".into());
        let b: (String, String) = ("XNAUT-307".into(), "Other old work".into());
        let c: (String, String) = ("XNAUT-308".into(), "Third old work".into());
        let set = vec![a.clone(), b.clone()];
        let mut announced = Announced::default();

        // Said once. Every later pass carrying the same set is silent, however
        // many times the sweep looks, and however the list is ordered.
        assert!(announced.stale_unowned_is_news(&set));
        announced.stale_unowned_delivered(&set);
        assert!(!announced.stale_unowned_is_news(&set));
        assert!(!announced.stale_unowned_is_news(&[b.clone(), a.clone()]));

        // A ticket entering is a change, and is said once.
        let grown = vec![a.clone(), b.clone(), c.clone()];
        assert!(announced.stale_unowned_is_news(&grown));
        announced.stale_unowned_delivered(&grown);
        assert!(!announced.stale_unowned_is_news(&grown));

        // A ticket leaving is a change too, and is also said once.
        assert!(announced.stale_unowned_is_news(&set));
        announced.stale_unowned_delivered(&set);
        assert!(!announced.stale_unowned_is_news(&set));

        // A title changing under the same ids is not a change: the notice is
        // about which tickets need a decision.
        assert!(!announced.stale_unowned_is_news(&[
            ("XNAUT-306".into(), "Old work, retitled".into()),
            ("XNAUT-307".into(), "Other old work, retitled".into()),
        ]));
    }

    /// An empty set says nothing, and — the actual defect — forgets nothing.
    /// The stale list is scoped to the fleet's projects and re-read from the
    /// board every tick with `unwrap_or_default()`, so one unreadable read
    /// empties it; treating that as "the owner dealt with them" re-posted the
    /// identical notice on the very next pass.
    #[test]
    fn an_empty_stale_set_is_never_posted_and_never_forgets() {
        let set: Vec<(String, String)> = vec![("XNAUT-306".into(), "Old work".into())];
        let mut announced = Announced::default();

        // Never posted: not before a notice has ever gone out, and not after.
        assert!(!announced.stale_unowned_is_news(&[]));
        announced.stale_unowned_delivered(&set);
        assert!(!announced.stale_unowned_is_news(&[]));

        // And the gap did not erase what the owner has already read.
        assert!(!announced.stale_unowned_is_news(&set));
    }

    /// Delivery is what is remembered, not the attempt: an inbox write that
    /// failed left the owner with nothing, so the notice is owed again.
    #[test]
    fn a_failed_stale_delivery_is_retried_on_the_next_pass() {
        let set: Vec<(String, String)> = vec![("XNAUT-306".into(), "Old work".into())];
        let mut announced = Announced::default();
        assert!(announced.stale_unowned_is_news(&set));
        // `run_action` records delivery only in the Ok arm; nothing here.
        assert!(announced.stale_unowned_is_news(&set));
        announced.stale_unowned_delivered(&set);
        assert!(!announced.stale_unowned_is_news(&set));
    }

    /// The fleet starts work only in projects that opted in, and only on
    /// tickets touched in the last two weeks; verification is not scoped.
    #[test]
    fn the_fleet_starts_work_only_in_opted_in_projects_and_only_on_fresh_tickets() {
        let mut tickets = vec![
            ticket("XNAUT-1", "ready", Some("claude"), "2026-09-01T09:00:00Z"),
            ticket("XNAUT-2", "ready", None, "2026-09-01T09:00:00Z"),
            ticket("XNAUT-3", "ready", Some("codex"), "2026-07-12T09:00:00Z"),
            ticket("XNAUT-4", "done", Some("nautbot"), "2026-07-12T09:00:00Z"),
            ticket("ENGRAMOSS-1", "ready", Some("codex"), "2026-09-01T09:00:00Z"),
            ticket("ENGRAMOSS-2", "ready", None, "2026-09-01T09:00:00Z"),
        ];
        tickets[1].priority = "high".into();
        tickets[5].priority = "high".into();
        tickets[5].project = "ENGRAMOSS".into();
        tickets[4].project = "ENGRAMOSS".into();
        let only = Fleet::Only(["XNAUT".to_string()].into_iter().collect());
        let plan = plan_fleet_for(&tickets, &[], None, at("10:00"), &only);
        let dispatched: Vec<&str> = plan.iter().filter_map(|a| match a { Action::Dispatch { ticket, .. } => Some(ticket.as_str()), _ => None }).collect();
        assert_eq!(dispatched, vec!["XNAUT-1"], "Engram is not in the fleet, XNAUT-3 is from July: {plan:?}");
        let triage = plan.iter().find_map(|a| match a { Action::Triage { tickets } => Some(tickets.clone()), _ => None }).unwrap();
        assert_eq!(triage.len(), 1);
        assert!(triage[0].starts_with("XNAUT-2:"), "{triage:?}");
        assert!(plan.iter().any(|a| matches!(a, Action::Verify { ticket, .. } if ticket == "XNAUT-4")), "verification is not scoped by freshness or fleet");
    }

    /// Unfinished business goes first and is never squeezed out by the budget.
    #[test]
    fn an_orphaned_verification_outranks_new_work_and_a_dead_one_is_dropped() {
        let mut tickets = vec![
            ticket("F-1", "done", Some("nautbot"), "2026-09-01T01:00:00Z"),
            ticket("F-2", "done", Some("nautbot"), "2026-09-01T02:00:00Z"),
            ticket("F-3", "done", Some("nautbot"), "2026-09-01T03:00:00Z"),
        ];
        tickets[0].project = "P1".into();
        tickets[1].project = "P2".into();
        let plan = plan_fleet(&tickets, &[], Some("F-3"), at("10:00"));
        assert_eq!(
            plan.first(),
            Some(&Action::Retry {
                ticket: "F-3".into(),
                project: "XNAUT".into()
            }),
            "the retry leads: {plan:?}"
        );
        // It spent one of the three slots, so only two new ones start.
        assert_eq!(
            plan.iter()
                .filter(|a| matches!(a, Action::Verify { .. }))
                .count(),
            MAX_VERIFIES_IN_FLIGHT - 1,
            "{plan:?}"
        );

        // A retry for a ticket that has left the board is dropped out loud.
        let plan = plan_fleet(&tickets, &[], Some("GONE-9"), at("10:00"));
        assert_eq!(
            plan.first(),
            Some(&Action::RetryDropped {
                ticket: "GONE-9".into()
            })
        );
    }
}

/// The production tick's registry/board boundary, also used by the isolated
/// live test. Removing reconciliation here must break the behavioural test.
pub(crate) fn registry_tick_in(
    registry: &std::path::Path,
    leases: &std::path::Path,
    repo: Option<&std::path::Path>,
    ledger: &std::path::Path,
    at: i64,
    mut observe: impl FnMut(&crate::run_control::RunManifest) -> crate::run_control::Proofs,
) -> Result<Vec<crate::project_management::TicketRecord>, String> {
    if let Some(repo) = repo {
        let tickets = crate::project_management::ticket_list_in(repo, None)?;
        crate::run_control::recover_handbacks_in(registry, &tickets)?;
    }
    registry_swap_tick_in(registry, leases, repo, ledger, at, &mut observe, crate::run_control::stop_writer)?;
    let failed = crate::run_control::reconcile_in(registry, at, observe)?;
    for (run, proof) in failed {
        if run.kind != crate::run_control::RunKind::Agent { continue; }
        registry_event_once(ledger, "registry_failed", &run, &run.last_signal)?;
        crate::run_control::finish_failed_in(registry, &run.run_id, |run| {
            let (Some(repo), Some(ticket_id)) = (repo, run.ticket.as_deref()) else {
                return Ok(false);
            };
            let Some(ticket) = crate::project_management::ticket_list_in(repo, None)?
                .into_iter()
                .find(|t| t.id == ticket_id)
            else {
                return Ok(false);
            };
            let receipt = format!("Registry run {} failed:", run.run_id);
            // Recover an acknowledged PM write even if the app died before the
            // ledger/manifest acknowledgement. Never mutate the newer assignment.
            if !ticket.body.contains(&receipt) {
                let latest_session = ticket
                    .body
                    .rsplit("\n## Dispatched ")
                    .next()
                    .unwrap_or_default()
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("- session `")
                            .and_then(|s| s.strip_suffix('`'))
                    });
                if ticket.status != "in_progress"
                    || ticket
                        .owner
                        .as_deref()
                        .unwrap_or_default()
                        .trim_start_matches('@')
                        .to_lowercase()
                        != run.agent_handle
                    || latest_session != run.pty_session.as_deref()
                    || run.pty_session.is_none()
                {
                    return Ok(false);
                }
                // Phase 1 never transfers a still-running writer's ticket or lease.
                if !proof.writers_gone() {
                    registry_event_once(ledger,"registry_return_deferred",&run,"ticket retained: process/session/capture has not proved the writer stopped")?;
                    return Ok(false);
                }
                crate::project_management::ticket_update_in(repo,crate::project_management::TicketUpdateRequest {
                model_requirement: None,
                caller: Some("nautbot".into()), id: ticket.id.clone(), expected_revision: ticket.revision,
                title: None, ticket_type: None, status: Some("ready".into()), priority: None,
                owner: None, clear_owner: true, documentation: None,
                body: Some(format!("{}\n\n{receipt} {}\nReturned to the board after liveness proofs failed.",ticket.body,run.last_signal)),
            })?;
            }
            if proof.writers_gone() {
                crate::writer_lease::release_holder_in(
                    leases,
                    std::path::Path::new(&run.worktree_path),
                    &run.agent_handle,
                    run.owner_pid,
                )?;
            }
            registry_event_once(ledger, "registry_ticket_returned", &run, &run.last_signal)?;
            Ok(true)
        })?;
    }
    match repo {
        Some(repo) => crate::project_management::ticket_list_in(repo, None),
        None => Ok(vec![]),
    }
}

fn ticket_belongs_to_run(ticket: &crate::project_management::TicketRecord, run: &crate::run_control::RunManifest) -> bool {
    let session = ticket.body.rsplit("\n## Dispatched ").next().unwrap_or_default().lines()
        .find_map(|l| l.strip_prefix("- session `").and_then(|v| v.strip_suffix('`')));
    ticket.status == "in_progress" && ticket.owner.as_deref().unwrap_or_default()
        .trim_start_matches('@').eq_ignore_ascii_case(&run.agent_handle)
        && run.pty_session.is_some() && session == run.pty_session.as_deref()
}

/// Detectors never transfer ownership. Only retirement_step_in can call the
/// acknowledgement closure, under the admission lock and after stop proof.
fn registry_swap_tick_in(
    registry: &std::path::Path, leases: &std::path::Path, repo: Option<&std::path::Path>,
    ledger: &std::path::Path, at: i64,
    observe: &mut impl FnMut(&crate::run_control::RunManifest) -> crate::run_control::Proofs,
    mut stop: impl FnMut(&crate::run_control::RunManifest) -> Result<(), String>,
) -> Result<(), String> {
    use crate::run_control::{self, RunState};
    let Some(repo) = repo else { return Ok(()); };
    for id in run_control::list_ids_in(registry)? {
        let mut run = run_control::load_manifest_in(registry, &id)?;
        if !matches!(run.state, RunState::Starting | RunState::Running | RunState::Blocked | RunState::Degraded | RunState::Retiring | RunState::Retired) { continue; }
        if run.state == RunState::Retired && (run.retirement.is_none() || run.ticket_returned) { continue; }
        let Some(ticket) = crate::project_management::ticket_list_in(repo, None)?.into_iter()
            .find(|t| Some(t.id.as_str()) == run.ticket.as_deref()) else { continue; };
        let receipt = format!("Registry swap {}:", run.run_id);
        if !ticket_belongs_to_run(&ticket, &run) && !ticket.body.contains(&receipt) { continue; }
        if !matches!(run.state, RunState::Retiring | RunState::Retired | RunState::Degraded) {
            let reason = run.model.as_deref().filter(|model|
                !crate::run_control::model_meets(model, &ticket.model_requirement))
                .map(|model| format!("reported model {model} does not meet {}", ticket.model_requirement))
                .or_else(|| run.output_path.as_ref().and_then(|p|
                    crate::run_signals::capture_notice(std::path::Path::new(p), &ticket.model_requirement).ok().flatten()));
            if let Some(reason) = reason {
                run = run_control::update_in(registry, &id, |r| {
                    if !r.state.terminal() && !matches!(r.state, RunState::Retiring | RunState::Undead) {
                        r.state = RunState::Degraded; r.last_signal = reason;
                    }
                })?;
                registry_event_once(ledger, "registry_degraded", &run, &run.last_signal)?;
            }
        }
        let proof = observe(&run);
        let after = run_control::retirement_step_in(registry, &id, &ticket.model_requirement, at, &proof, |retired| {
            // Fresh board read plus revision CAS: never return a new assignment.
            let current = crate::project_management::ticket_list_in(repo, None)?.into_iter()
                .find(|t| Some(t.id.as_str()) == retired.ticket.as_deref()).ok_or("swap ticket vanished")?;
            if !ticket_belongs_to_run(&current, retired) && !current.body.contains(&receipt) {
                return Err("swap ticket belongs to a newer assignment".into());
            }
            registry_event_once(ledger, "registry_stop_proven", retired, &format!(
                "pid absent; session absent; capture unchanged for grace window; stopped_at={:?}",
                retired.retirement.as_ref().and_then(|r| r.stopped_at)))?;
            crate::writer_lease::release_swap_holder_in(leases, std::path::Path::new(&retired.worktree_path),
                &retired.agent_handle, retired.owner_pid)?;
            registry_event_once(ledger, "registry_lease_released", retired, "lease released after stop proof")?;
            if !current.body.contains(&receipt) {
                crate::project_management::ticket_update_in(repo, crate::project_management::TicketUpdateRequest {
                    model_requirement: None, caller: Some("nautbot".into()), id: current.id.clone(), expected_revision: current.revision,
                    title: None, ticket_type: None, status: Some("ready".into()), priority: None,
                    owner: None, clear_owner: true, documentation: None,
                    body: Some(format!("{}\n\n{receipt} {}\nStopped writer proven dead; returned to triage. Continue branch `{}` in `{}`. Successor requested: {}. Required model: {}.",
                        current.body, retired.last_signal, retired.branch, retired.worktree_path,
                        retired.next_run_id.as_deref().unwrap_or_default(), ticket.model_requirement)),
                })?;
            }
            registry_event_once(ledger, "registry_swap_returned", retired, &retired.last_signal)?;
            Ok(())
        })?;
        if after.state == RunState::Retiring {
            registry_event_once(ledger, "registry_retiring", &after, &after.last_signal)?;
            let result = stop(&after);
            registry_event_once(ledger, "registry_stop_requested", &after,
                &result.err().unwrap_or_else(|| "SIGTERM followed by zellij delete-session --force requested".into()))?;
        } else if after.state == RunState::Undead {
            registry_event_once(ledger, "registry_undead", &after, &after.last_signal)?;
        }
    }
    Ok(())
}

fn announce_undead(app: &AppHandle, registry: &std::path::Path) -> Result<(), String> {
    for id in crate::run_control::list_ids_in(registry)? {
        let run = crate::run_control::load_manifest_in(registry, &id)?;
        if run.state != crate::run_control::RunState::Undead || run.undead_notified { continue; }
        let req = serde_json::from_value(serde_json::json!({
            "project": run.project, "from": "@nautbot", "ticket": run.ticket,
            "title": "Runtime swap refused: writer did not stop", "body": run.last_signal,
            "level": "error", "context": {"run_id":run.run_id, "worktree":run.worktree_path,
                "session_id":run.pty_session.clone().unwrap_or_default(), "lease":"retained"}
        })).map_err(|e| e.to_string())?;
        crate::inbox::create_and_announce(app, "notify", req, run.pty_session)?;
        crate::run_control::update_in(registry, &id, |r| r.undead_notified = true)?;
    }
    Ok(())
}

fn registry_event_once(
    path: &std::path::Path,
    kind: &str,
    run: &crate::run_control::RunManifest,
    detail: &str,
) -> Result<(), String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    if existing
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|e| e["kind"] == kind && e["run_id"] == run.run_id)
    {
        return Ok(());
    }
    crate::ledger::record_run_in(path, kind, run, detail)
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    use crate::run_control::tests::{directory, proof};
    use crate::run_control::{self, RunState};
    use std::path::Path;

    fn git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn seed(root: &Path, at: i64) -> run_control::RunManifest {
        let repo = root.join("control");
        std::fs::create_dir_all(repo.join("projects/XNAUT/tickets")).unwrap();
        std::fs::create_dir_all(repo.join("events")).unwrap();
        git(&repo, &["init", "-b", "agent/test"]);
        git(&repo, &["config", "user.name", "Registry test"]);
        git(&repo, &["config", "user.email", "registry@example.invalid"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(
            repo.join("projects/XNAUT/project.json"),
            r#"{"key":"XNAUT","name":"xNAUT","revision":1,"created_at":"2026-09-07T00:00:00Z"}"#,
        )
        .unwrap();
        std::fs::write(repo.join("projects/XNAUT/tickets/XNAUT-900.json"),serde_json::to_vec(&serde_json::json!({
            "id":"XNAUT-900","project":"XNAUT","title":"Isolated registry proof","type":"task",
            "status":"in_progress","owner":"codex","revision":1,"priority":"high","created_at":"2026-09-07T00:00:00Z","updated_at":"2026-09-07T00:00:00Z",
            "body":"\n## Dispatched today to @codex\n\n- session `test-session`\n"
        })).unwrap()).unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "seed isolated registry ticket"]);
        let mut record = run_control::RunManifest::requested(
            "codex", "test-process", &repo.to_string_lossy(),
            Some("XNAUT-900".into()), None, &[], at,
        );
        record.pty_session = Some("test-session".into());
        record.last_commit = proof().commit;
        run_control::request_in(&root.join("registry"), record, || Ok(())).unwrap()
    }
    fn seed_lease(root: &std::path::Path, record: &run_control::RunManifest) -> std::path::PathBuf {
        use sha2::{Digest, Sha256};
        let real = std::path::Path::new(&record.worktree_path)
            .canonicalize()
            .unwrap();
        let leases = root.join("leases");
        std::fs::create_dir_all(&leases).unwrap();
        let path = leases.join(format!(
            "{:x}.json",
            Sha256::digest(real.to_string_lossy().as_bytes())
        ));
        std::fs::write(
            &path,
            serde_json::to_vec(&crate::writer_lease::Holder {
                handle: record.agent_handle.clone(),
                pid: record.owner_pid,
                path: record.worktree_path.clone(),
                at: "test".into(),
            })
            .unwrap(),
        )
        .unwrap();
        path
    }
    #[test]
    fn registry_tick_reclaims_dead_run_and_waits_for_a_live_writer() {
        let root = directory("sweep");
        let record = seed(&root, 1_000);
        let lease = seed_lease(&root, &record);
        let registry = root.join("registry");
        let repo = root.join("control");
        let leases = root.join("leases");
        let ledger = root.join("ledger.jsonl");
        // The integration assertion is intentionally through the production
        // tick, not a second hand-written implementation of its verdict.
        let tickets = registry_tick_in(&registry, &leases, Some(&repo), &ledger, 1_000_000, |_| {
            proof()
        })
        .unwrap();
        assert_eq!(
            run_control::load_manifest_in(&registry, &record.run_id)
                .unwrap()
                .state,
            RunState::Failed,
            "sweep must reconcile the dead run"
        );
        assert_eq!(
            tickets[0].status, "ready",
            "sweep must return the failed run's ticket"
        );
        assert!(tickets[0].owner.is_none());
        assert!(!lease.exists(), "the dead writer lease is released");
        let first = std::fs::read_to_string(&ledger).unwrap();
        assert_eq!(first.lines().count(), 2);
        assert!(first.contains(&record.run_id));
        registry_tick_in(&registry, &leases, Some(&repo), &ledger, 1_001_000, |_| {
            proof()
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&ledger).unwrap(),
            first,
            "retry must not duplicate effects"
        );
        // A stale cache/ack after the PM commit is recovered without another
        // PM update, using the exact run receipt in the ticket body.
        run_control::update_in(&registry, &record.run_id, |r| r.ticket_returned = false).unwrap();
        let retry = registry_tick_in(&registry, &leases, Some(&repo), &ledger, 1_002_000, |_| {
            proof()
        })
        .unwrap();
        assert_eq!(retry[0].revision, tickets[0].revision);
        assert!(
            run_control::load_manifest_in(&registry, &record.run_id)
                .unwrap()
                .ticket_returned
        );
        std::fs::remove_dir_all(root).unwrap();

        let root = directory("stalled-writer");
        let record = seed(&root, 1_000);
        let lease = seed_lease(&root, &record);
        let mut alive = proof();
        alive.pid_alive = true;
        let tickets = registry_tick_in(
            &root.join("registry"),
            &root.join("leases"),
            Some(&root.join("control")),
            &root.join("ledger"),
            1_000_000,
            |_| alive.clone(),
        )
        .unwrap();
        assert_eq!(
            run_control::load_manifest_in(&root.join("registry"), &record.run_id)
                .unwrap()
                .state,
            RunState::Failed
        );
        assert_eq!(
            tickets[0].status, "in_progress",
            "a still-running writer retains its ticket"
        );
        assert!(lease.exists(), "a live writer keeps its lease");
        let mut stopped = proof();
        stopped.capture_bytes = 12;
        stopped.capture_quiet = false;
        let tickets = registry_tick_in(
            &root.join("registry"),
            &root.join("leases"),
            Some(&root.join("control")),
            &root.join("ledger"),
            1_100_000,
            |_| stopped.clone(),
        )
        .unwrap();
        assert_eq!(
            tickets[0].status, "in_progress",
            "final output must settle before transfer"
        );
        assert_eq!(
            run_control::load_manifest_in(&root.join("registry"), &record.run_id)
                .unwrap()
                .capture_bytes,
            12
        );
        stopped.capture_quiet = true;
        let tickets = registry_tick_in(
            &root.join("registry"),
            &root.join("leases"),
            Some(&root.join("control")),
            &root.join("ledger"),
            1_280_000,
            |_| stopped.clone(),
        )
        .unwrap();
        assert_eq!(
            tickets[0].status, "ready",
            "a failed writer can be returned after it stops"
        );
        assert!(!lease.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn registry_tick_preserves_waiting_and_a_newer_ticket_assignment() {
        let root = directory("waiting");
        let record = seed(&root, 1_000);
        run_control::update_in(&root.join("registry"), &record.run_id, |r| {
            r.waiting_on = Some("in-approval".into())
        })
        .unwrap();
        let mut alive = proof();
        alive.pid_alive = true;
        let tickets = registry_tick_in(
            &root.join("registry"),
            &root.join("leases"),
            Some(&root.join("control")),
            &root.join("ledger"),
            1_000_000,
            |_| alive.clone(),
        )
        .unwrap();
        assert_eq!(tickets[0].status, "in_progress");
        assert_eq!(
            run_control::load_manifest_in(&root.join("registry"), &record.run_id)
                .unwrap()
                .state,
            RunState::Blocked
        );
        // Reassignment changes the exact session binding, even to the same
        // handle. An old failed run must not claim that newer ticket.
        let path = root.join("control/projects/XNAUT/tickets/XNAUT-900.json");
        let mut ticket: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        ticket["body"] =
            serde_json::json!("\n## Dispatched later to @codex\n\n- session `new-session`\n");
        std::fs::write(path, serde_json::to_vec(&ticket).unwrap()).unwrap();
        let tickets = registry_tick_in(
            &root.join("registry"),
            &root.join("leases"),
            Some(&root.join("control")),
            &root.join("ledger"),
            2_000_000,
            |_| proof(),
        )
        .unwrap();
        assert_eq!(tickets[0].status, "in_progress");
        assert_eq!(tickets[0].revision, 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn accepted_handback_marks_done_before_reconcile_and_recovers_interruption() {
        let root = directory("handback-completion");
        let record = seed(&root, 1_000);
        let registry = root.join("registry");
        let repo = root.join("control");
        let mut h = run_control::tests::handback(&record);
        h.how_verified.clear();
        assert!(matches!(
            crate::project_management::file_handback_with_registry_in(&repo, &registry, &h)
                .unwrap(),
            crate::project_management::Filing::Refused(_)
        ));
        assert_eq!(
            run_control::load_manifest_in(&registry, &record.run_id)
                .unwrap()
                .state,
            RunState::Starting
        );
        h = run_control::tests::handback(&record);
        assert!(matches!(
            crate::project_management::file_handback_with_registry_in(&repo, &registry, &h)
                .unwrap(),
            crate::project_management::Filing::Filed { .. }
        ));
        // This assertion must fail if the filing-time completion line is removed.
        assert_eq!(
            run_control::load_manifest_in(&registry, &record.run_id)
                .unwrap()
                .state,
            RunState::Done
        );
        let stored = crate::project_management::ticket_list_in(&repo, None).unwrap();
        assert_eq!(stored[0].handback.as_ref().unwrap().run_id, h.run_id);
        // Simulate an app crash after PM committed but before the Done journal append.
        run_control::update_in(&registry, &record.run_id, |r| r.state = RunState::Running).unwrap();
        let tickets = registry_tick_in(
            &registry,
            &root.join("leases"),
            Some(&repo),
            &root.join("ledger"),
            1_000_000,
            |_| run_control::Proofs {
                exit_code: Some(137),
                ..proof()
            },
        )
        .unwrap();
        let done = run_control::load_manifest_in(&registry, &record.run_id).unwrap();
        assert_eq!(done.state, RunState::Done);
        assert!(!done.ticket_returned);
        assert!(done.retirement.is_none());
        assert_eq!(tickets[0].status, "in_progress");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn another_runs_handback_does_not_hide_a_dead_process() {
        let root = directory("unrelated-handback");
        let record = seed(&root, 1_000);
        let mut h = run_control::tests::handback(&record);
        h.run_id = Some("unrelated-run".into());
        let path = root.join("control/projects/XNAUT/tickets/XNAUT-900.json");
        let mut t: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        t["handback"] = serde_json::to_value(h).unwrap();
        std::fs::write(path, serde_json::to_vec(&t).unwrap()).unwrap();
        registry_tick_in(
            &root.join("registry"),
            &root.join("leases"),
            Some(&root.join("control")),
            &root.join("ledger"),
            1_000_000,
            |_| proof(),
        )
        .unwrap();
        assert_eq!(
            run_control::load_manifest_in(&root.join("registry"), &record.run_id)
                .unwrap()
                .state,
            RunState::Failed
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "explicit isolated production-wrapper handback/kill proof"]
    fn registry_live_handback_then_kill_stays_done() {
        let root = directory("live-handback");
        let record = seed(&root, run_control::now_ms());
        let registry = root.join("registry");
        let argv = run_control::launch_argv_in(
            &registry,
            &record.run_id,
            &["/bin/sleep".into(), "60".into()],
        )
        .unwrap();
        let mut child = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .env("XNAUT_RUN_ID", &record.run_id)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let live = loop {
            let p = run_control::observe_in(&registry, &record, &[]);
            if p.pid_alive {
                break p;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("wrapper did not record a live process");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        run_control::update_in(&registry, &record.run_id, |r| {
            r.pid = live.pid;
            r.process_birth = live.process_birth.clone();
            r.state = RunState::Running;
        })
        .unwrap();
        let h = run_control::tests::handback(&record);
        let filed = crate::project_management::file_handback_with_registry_in(
            &root.join("control"),
            &registry,
            &h,
        );
        let before_kill = run_control::load_manifest_in(&registry, &record.run_id).unwrap();
        let killed = std::process::Command::new("/bin/kill")
            .args(["-9", &live.pid.unwrap().to_string()])
            .status()
            .unwrap();
        let exit = child.wait().unwrap();
        assert!(killed.success());
        assert!(!exit.success());
        assert!(matches!(
            filed.unwrap(),
            crate::project_management::Filing::Filed { .. }
        ));
        assert_eq!(before_kill.state, RunState::Done);
        let dead = run_control::observe_in(&registry, &before_kill, &[]);
        assert!(!dead.pid_alive && dead.pid_absent);
        let tickets = registry_tick_in(
            &registry,
            &root.join("leases"),
            Some(&root.join("control")),
            &root.join("ledger"),
            run_control::now_ms(),
            |r| run_control::observe_in(&registry, r, &[]),
        )
        .unwrap();
        let done = run_control::load_manifest_in(&registry, &record.run_id).unwrap();
        assert_eq!(done.state, RunState::Done);
        assert!(!done.ticket_returned);
        assert_eq!(tickets[0].status, "in_progress");
        println!(
            "LIVE_HANDBACK {}",
            serde_json::json!({
                "run_id": record.run_id, "pid": live.pid, "process_birth": live.process_birth,
                "state_before_kill": before_kill.state, "exit_code": dead.exit_code,
                "pid_absent": dead.pid_absent, "state_after_reconcile": done.state,
                "handback_run_id": tickets[0].handback.as_ref().unwrap().run_id,
                "ticket_returned": done.ticket_returned, "evidence_dir": root,
            })
        );
    }

    #[test]
    fn production_tick_calls_the_tested_registry_boundary() {
        // The behavioural test above exercises the actual reconciliation;
        // this small wiring check also catches removing the outer app call.
        let source = include_str!("sweep.rs");
        let tick = source
            .split("async fn tick(")
            .nth(1)
            .unwrap()
            .split("///")
            .next()
            .unwrap();
        assert!(tick.contains("registry_tick_in("));
    }
    #[test]
    #[ignore = "explicit isolated live process proof; run with --ignored --nocapture"]
    fn registry_live_kill_reclaims_ticket() {
        let root = directory("live-registry");
        let started = run_control::now_ms();
        let record = seed(&root, started);
        let registry = root.join("registry");
        let repo = root.join("control");
        let leases = root.join("leases");
        let ledger = root.join("ledger.jsonl");
        let argv = run_control::launch_argv_in(
            &registry,
            &record.run_id,
            &["/bin/sleep".into(), "600".into()],
        )
        .unwrap();
        let mut child = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .env("XNAUT_RUN_ID", &record.run_id)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let proof = loop {
            let p = run_control::observe_in(&registry, &record, &[]);
            if p.pid_alive {
                break p;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "launcher did not prove a real CLI pid"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        run_control::update_in(&registry, &record.run_id, |r| {
            r.state = RunState::Running;
            r.pid = proof.pid;
            r.process_birth = proof.process_birth.clone();
        })
        .unwrap();
        let killed_at = run_control::now_ms();
        assert!(std::process::Command::new("/bin/kill")
            .args(["-9", &proof.pid.unwrap().to_string()])
            .status()
            .unwrap()
            .success());
        assert!(!child.wait().unwrap().success());
        let tickets = registry_tick_in(
            &registry,
            &leases,
            Some(&repo),
            &ledger,
            run_control::now_ms(),
            |r| run_control::observe_in(&registry, r, &[]),
        )
        .unwrap();
        let failed = run_control::load_manifest_in(&registry, &record.run_id).unwrap();
        let reconciled_at = run_control::now_ms();
        assert_eq!(failed.state, RunState::Failed);
        assert!(!failed.last_signal.is_empty());
        assert_eq!(tickets[0].status, "ready");
        assert!(tickets[0].owner.is_none());
        assert!(reconciled_at - killed_at < 180_000);
        println!(
            "LIVE evidence {}",
            serde_json::json!({"machine": std::process::Command::new("hostname").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap(),"run_id":record.run_id,"started_at_ms":started,"killed_at_ms":killed_at,"reconciled_at_ms":reconciled_at,"pid":proof.pid,"reason":failed.last_signal,"ticket_status":tickets[0].status,"ticket_revision":tickets[0].revision,"evidence_dir":root})
        );
        println!("LEDGER\n{}", std::fs::read_to_string(ledger).unwrap());
    }
    fn swap_seed(root: &Path, requirement: &str) -> (run_control::RunManifest, std::path::PathBuf) {
        let record = seed(root, 1000);
        let path = root.join("control/projects/XNAUT/tickets/XNAUT-900.json");
        let mut ticket: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        ticket["model_requirement"] = requirement.into();
        std::fs::write(path, serde_json::to_vec(&ticket).unwrap()).unwrap();
        let record = run_control::update_in(&root.join("registry"), &record.run_id, |r| {
            r.state = RunState::Degraded; r.last_signal = "hook model old does not meet required".into();
        }).unwrap();
        let lease = seed_lease(root, &record);
        (record, lease)
    }
    fn stopped_proof() -> run_control::Proofs {
        run_control::Proofs { pid_absent: true, session_known: true, capture_known: true,
            ..proof() }
    }
    fn swap_tick(root: &Path, at: i64, observation: run_control::Proofs) {
        registry_swap_tick_in(&root.join("registry"), &root.join("leases"), Some(&root.join("control")),
            &root.join("ledger"), at, &mut |_| observation.clone(), |_| Ok(())).unwrap();
    }
    #[test]
    fn swap_guard_requires_every_proof_and_retains_the_lease_on_refusal() {
        for missing in ["pid", "session", "session-query", "capture-read", "capture-growth"] {
            let root = directory(missing);
            let (old, lease) = swap_seed(&root, "required");
            swap_tick(&root, 1000, stopped_proof());
            assert!(lease.exists());
            assert_eq!(run_control::load_manifest_in(&root.join("registry"), &old.run_id).unwrap().state, RunState::Retiring);
            let mut p = stopped_proof();
            match missing {
                "pid" => { p.pid_absent = false; p.pid_alive = true; },
                "session" => p.session_alive = true,
                "session-query" => p.session_known = false,
                "capture-read" => p.capture_known = false,
                _ => p.capture_bytes = 12,
            }
            swap_tick(&root, 1000 + 2 * run_control::GRACE_MS, p);
            let refused = run_control::load_manifest_in(&root.join("registry"), &old.run_id).unwrap();
            assert_eq!(refused.state, RunState::Undead, "missing {missing} must refuse the swap");
            assert!(lease.exists(), "unproven writer must retain lease");
            let ticket = crate::project_management::ticket_list_in(&root.join("control"), None).unwrap().remove(0);
            assert_eq!(ticket.status, "in_progress");
            assert_eq!(ticket.owner.as_deref(), Some("codex"));
            assert!(run_control::continuation_in(&root.join("registry"), "XNAUT-900").unwrap().is_none());
            run_control::signal_session_in(&root.join("registry"), "test-session", Some(RunState::Running), None, 200000).unwrap();
            assert_eq!(run_control::load_manifest_in(&root.join("registry"), &old.run_id).unwrap().state, RunState::Undead);
            let next = run_control::RunManifest::requested("codex", "codex", &old.worktree_path,
                old.ticket.clone(), Some("required".into()), &[], 200000);
            assert!(run_control::request_in(&root.join("registry"), next, || panic!("admission must not reach the lease")).is_err());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn swap_waits_for_grace_reserves_same_worktree_and_replays_once() {
        let root = directory("swap-success");
        let (old, lease) = swap_seed(&root, "required");
        swap_tick(&root, 1000, stopped_proof());
        swap_tick(&root, 1000 + run_control::GRACE_MS - 1, stopped_proof());
        assert!(lease.exists(), "mtime or an immediate dead pid cannot skip the observed grace");
        swap_tick(&root, 1000 + run_control::GRACE_MS, stopped_proof());
        let retired = run_control::load_manifest_in(&root.join("registry"), &old.run_id).unwrap();
        assert_eq!(retired.state, RunState::Retired);
        assert!(!lease.exists());
        let next = run_control::continuation_in(&root.join("registry"), "XNAUT-900").unwrap().unwrap();
        assert_eq!(next.state, RunState::Requested);
        assert_eq!(next.previous_run_id.as_deref(), Some(old.run_id.as_str()));
        assert_eq!(retired.next_run_id.as_deref(), Some(next.run_id.as_str()));
        assert_eq!(next.worktree_path, old.worktree_path);
        assert_eq!(next.branch, old.branch);
        assert_eq!(run_control::verdict(&next, &proof(), i64::MAX), run_control::Verdict::Keep);
        let events = std::fs::read_to_string(root.join("ledger")).unwrap();
        assert!(events.find("registry_stop_proven").unwrap() < events.find("registry_lease_released").unwrap());
        assert!(events.find("registry_lease_released").unwrap() < events.find("registry_swap_returned").unwrap());
        let tickets = crate::project_management::ticket_list_in(&root.join("control"), None).unwrap();
        assert_eq!(tickets[0].status, "ready");
        assert!(tickets[0].owner.is_none());
        let rev = tickets[0].revision;
        // Simulate a crash after the PM write but before its acknowledgement.
        run_control::update_in(&root.join("registry"), &old.run_id, |r| r.ticket_returned = false).unwrap();
        swap_tick(&root, 1000 + 3 * run_control::GRACE_MS, stopped_proof());
        assert_eq!(crate::project_management::ticket_list_in(&root.join("control"), None).unwrap()[0].revision, rev);
        assert_eq!(std::fs::read_to_string(root.join("ledger")).unwrap(), events);
        assert_eq!(run_control::list_ids_in(&root.join("registry")).unwrap().len(), 2);
        let wrong = run_control::RunManifest::requested("other", "runtime", &old.worktree_path,
            old.ticket.clone(), Some("wrong".into()), &[], 500000);
        assert!(run_control::request_in(&root.join("registry"), wrong, || panic!("wrong model cannot be admitted")).is_err());
        let replacement = run_control::RunManifest::requested("other", "runtime", &old.worktree_path,
            old.ticket.clone(), Some("required".into()), &[], 500001);
        let admitted = run_control::request_in(&root.join("registry"), replacement, || Ok(())).unwrap();
        assert_eq!(admitted.run_id, next.run_id);
        assert_eq!(admitted.previous_run_id, next.previous_run_id);
        assert_eq!(admitted.state, RunState::Starting);
        assert_eq!(admitted.agent_handle, "other");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn swap_default_off_and_waiting_ticket_are_untouched() {
        for (requirement, waiting) in [("", None), ("required", Some("in-owner"))] {
            let root = directory("swap-disabled");
            let (run, lease) = swap_seed(&root, requirement);
            run_control::update_in(&root.join("registry"), &run.run_id, |r| r.waiting_on = waiting.map(str::to_string)).unwrap();
            registry_tick_in(&root.join("registry"), &root.join("leases"), Some(&root.join("control")),
                &root.join("ledger"), 500000, |_| stopped_proof()).unwrap();
            assert!(lease.exists());
            assert_eq!(run_control::load_manifest_in(&root.join("registry"), &run.run_id).unwrap().state, RunState::Degraded);
            assert_eq!(crate::project_management::ticket_list_in(&root.join("control"), None).unwrap()[0].revision, 1);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    #[ignore = "real isolated zellij/process swap and SIGTERM-resistant refusal; takes three minutes"]
    fn registry_live_model_swap_and_undead() {
        struct Cleanup { pid: Option<u32>, session: Option<String>, child: Option<std::process::Child> }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Some(pid) = self.pid { unsafe { libc::kill(pid as i32, libc::SIGKILL); } }
                if let Some(session) = self.session.take() { let _ = crate::zellij::zellij_delete_session(session); }
                if let Some(child) = self.child.as_mut() { let _ = child.wait(); }
            }
        }
        for resistant in [false, true] {
            let root = directory(if resistant { "live-undead" } else { "live-swap" });
            let (old, lease) = swap_seed(&root, "required");
            let registry = root.join("registry");
            let capture = root.join("capture.log");
            let session = format!("xnaut-swap-proof-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
            let script = if resistant { "trap '' TERM HUP; echo ready; while :; do sleep 1; done" }
                else { "echo ready; exec sleep 600" };
            let argv = run_control::launch_argv_in(&registry, &old.run_id,
                &["/bin/sh".into(), "-c".into(), script.into()]).unwrap();
            let mut cleanup = Cleanup { pid: None, session: None, child: None };
            if resistant {
                let output = std::fs::File::create(&capture).unwrap();
                cleanup.child = Some(std::process::Command::new(&argv[0]).args(&argv[1..]).env("XNAUT_RUN_ID", &old.run_id)
                    .stdout(output.try_clone().unwrap()).stderr(output).spawn().unwrap());
            } else {
                let quote = |v: &str| format!("'{}'", v.replace('\'', "'\\''"));
                let command = format!("{} >{} 2>&1", argv.iter().map(|v| quote(v)).collect::<Vec<_>>().join(" "), quote(capture.to_str().unwrap()));
                let kdl = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"");
                let layout = root.join("layout.kdl");
                std::fs::write(&layout, format!("layout {{\n pane command=\"/bin/sh\" {{\n args \"-c\" \"{}\"\n cwd \"{}\"\n }}\n}}\n", kdl(&command), kdl(root.to_str().unwrap()))).unwrap();
                let config = root.join("zellij-config");
                std::fs::create_dir_all(&config).unwrap();
                std::fs::write(config.join("config.kdl"), "session_serialization false\nshow_startup_tips false\n").unwrap();
                cleanup.session = Some(session.clone());
                let out = std::process::Command::new("zellij").env_remove("ZELLIJ").env_remove("ZELLIJ_SESSION_NAME")
                    .env("XNAUT_RUN_ID", &old.run_id).arg("--config-dir").arg(&config)
                    .arg("--data-dir").arg(root.join("zellij-data")).arg("--layout").arg(&layout)
                    .args(["attach", "--create-background", &session]).output().unwrap();
                assert!(out.status.success(), "zellij fixture launch: {}", String::from_utf8_lossy(&out.stderr));
                cleanup.session = Some(session.clone());
            }
            let old = run_control::update_in(&registry, &old.run_id, |r| {
                r.output_path = Some(capture.to_string_lossy().into());
                r.zellij_session = cleanup.session.clone();
                r.started_at = run_control::now_ms();
                r.last_progress_at = r.started_at;
            }).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            let initial = loop {
                let p = run_control::observe_swap_in(&registry, &old);
                cleanup.pid = p.pid;
                if p.pid_alive && std::fs::read_to_string(&capture).unwrap_or_default().contains("ready") { break p; }
                assert!(std::time::Instant::now() < deadline, "fixture did not launch; capture={:?}", std::fs::read_to_string(&capture));
                std::thread::sleep(Duration::from_millis(100));
            };
            cleanup.pid = initial.pid;
            if !resistant { assert!(initial.session_alive && initial.session_known); }
            let began = run_control::now_ms();
            let deadline = std::time::Instant::now() + Duration::from_secs(150);
            let final_run = loop {
                if let Some(child) = cleanup.child.as_mut() { let _ = child.try_wait(); }
                registry_tick_in(&registry, &root.join("leases"), Some(&root.join("control")),
                    &root.join("ledger"), run_control::now_ms(), |r| run_control::observe_swap_in(&registry, r)).unwrap();
                let r = run_control::load_manifest_in(&registry, &old.run_id).unwrap();
                if matches!(r.state, RunState::Retired | RunState::Undead) { break r; }
                assert!(lease.exists(), "lease released while writer is retiring");
                assert!(std::time::Instant::now() < deadline, "retirement did not reach a verdict");
                std::thread::sleep(Duration::from_secs(1));
            };
            assert_eq!(final_run.state, if resistant { RunState::Undead } else { RunState::Retired });
            assert_eq!(lease.exists(), resistant);
            let ticket = crate::project_management::ticket_list_in(&root.join("control"), None).unwrap().remove(0);
            assert_eq!(ticket.status, if resistant { "in_progress" } else { "ready" });
            let next = run_control::continuation_in(&registry, "XNAUT-900").unwrap();
            if resistant {
                assert!(next.is_none());
                assert!(run_control::observe_swap_in(&registry, &final_run).pid_alive);
            } else {
                let next = next.as_ref().unwrap();
                assert_eq!(next.state, RunState::Requested);
                assert_eq!(next.previous_run_id.as_deref(), Some(old.run_id.as_str()));
                assert_eq!(final_run.next_run_id.as_deref(), Some(next.run_id.as_str()));
                assert_eq!(next.worktree_path, old.worktree_path);
                assert_eq!(next.branch, old.branch);
                assert!(run_control::observe_swap_in(&registry, &final_run).pid_absent);
                let ledger = std::fs::read_to_string(root.join("ledger")).unwrap();
                assert!(ledger.find("registry_stop_proven").unwrap() < ledger.find("registry_lease_released").unwrap());
                cleanup.pid = None;
            }
            let evidence = serde_json::json!({"run":final_run,"successor":next,"lease_held":lease.exists(),
                "ticket_status":ticket.status,"began_at":began,"finished_at":run_control::now_ms(),"evidence_dir":root});
            std::fs::write(root.join("proof.json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
            println!("LIVE {}", evidence);
            println!("LEDGER {}", std::fs::read_to_string(root.join("ledger")).unwrap());
        }
    }

    #[test]
    fn a_late_tick_observes_final_buffered_output_before_releasing() {
        let root = directory("late-stop-observation");
        let (old, lease) = swap_seed(&root, "required");
        let mut alive = stopped_proof(); alive.pid_absent = false; alive.pid_alive = true;
        swap_tick(&root, 1000, alive);
        let mut final_output = stopped_proof(); final_output.capture_bytes = 40;
        swap_tick(&root, 181000, final_output.clone());
        assert!(lease.exists());
        assert_eq!(run_control::load_manifest_in(&root.join("registry"), &old.run_id).unwrap().state, RunState::Retiring);
        swap_tick(&root, 241000, final_output);
        assert!(!lease.exists());
        assert_eq!(run_control::load_manifest_in(&root.join("registry"), &old.run_id).unwrap().state, RunState::Retired);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn an_admission_refusal_keeps_the_continuation_on_its_branch() {
        let root = directory("retry-successor");
        let (old, _) = swap_seed(&root, "required");
        swap_tick(&root, 1000, stopped_proof());
        swap_tick(&root, 61000, stopped_proof());
        let reserved = run_control::continuation_in(&root.join("registry"), "XNAUT-900").unwrap().unwrap();
        let candidate = || run_control::RunManifest::requested("other", "runtime", &old.worktree_path,
            old.ticket.clone(), Some("required".into()), &[], 80000);
        assert!(run_control::request_in(&root.join("registry"), candidate(), || Err("capacity full".into())).is_err());
        let failed = run_control::continuation_in(&root.join("registry"), "XNAUT-900").unwrap().unwrap();
        assert_eq!(failed.run_id, reserved.run_id);
        assert!(failed.admission_refused);
        assert_eq!(failed.worktree_path, old.worktree_path);
        let retry = run_control::request_in(&root.join("registry"), candidate(), || Ok(())).unwrap();
        assert_ne!(retry.run_id, failed.run_id);
        assert_eq!(retry.previous_run_id.as_deref(), Some(failed.run_id.as_str()));
        assert_eq!(run_control::load_manifest_in(&root.join("registry"), &failed.run_id).unwrap().next_run_id.as_deref(), Some(retry.run_id.as_str()));
        assert_eq!(retry.branch, old.branch);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn model_requirement_persists_and_can_be_disabled_again() {
        let root = directory("requirement-persistence");
        seed(&root, 1000);
        let repo = root.join("control");
        let create = serde_json::from_value(serde_json::json!({"project":"XNAUT", "title":"requirement test", "model_requirement":" required "})).unwrap();
        let ticket = crate::project_management::ticket_create_in(&repo, create).unwrap();
        assert_eq!(ticket.model_requirement, "required");
        let update = serde_json::from_value(serde_json::json!({"id":ticket.id,"expected_revision":ticket.revision,"body":"still required"})).unwrap();
        let ticket = crate::project_management::ticket_update_in(&repo, update).unwrap();
        assert_eq!(ticket.model_requirement, "required");
        let update = serde_json::from_value(serde_json::json!({"id":ticket.id,"expected_revision":ticket.revision,"model_requirement":""})).unwrap();
        let ticket = crate::project_management::ticket_update_in(&repo, update).unwrap();
        assert!(ticket.model_requirement.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

}
