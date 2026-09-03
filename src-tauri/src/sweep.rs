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
// ponytail: one ticket dispatched per tick. A batch would be faster and would
// also be the thing that empties a spend ceiling in ninety seconds; the tick
// is cheap, so the queue drains at a visible pace instead.

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

pub fn spawn_sweep_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // A first tick immediately after start would race the app's own
        // initialization (settings, PM repo, hook server). One interval of
        // patience costs nothing and avoids a cold-start false alarm.
        tokio::time::sleep(TICK).await;
        let mut announced = Announced::default();
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
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn tick(app: &AppHandle, announced: &mut Announced) -> Result<(), String> {
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
    let repo = match crate::project_management::repo_now() {
        Ok(repo) => {
            announced.no_repo = false;
            repo
        }
        Err(error) => {
            if !announced.no_repo {
                crate::ledger::record("sweep_idle", "nautbot", "", &error);
                announced.no_repo = true;
            }
            return Ok(());
        }
    };
    let tickets = crate::project_management::ticket_list_in(&repo, None)?;

    // 0. Unfinished business first. A verification that died with the last app
    //    is work already decided on; picking new work ahead of it would leave
    //    the ticket in limbo exactly as long as the board stays busy.
    if let Some(ticket_id) = next_retry() {
        if let Some(ticket) = tickets.iter().find(|t| t.id == ticket_id) {
            crate::ledger::record(
                "sweep_retry",
                "nautbot",
                &ticket.id,
                "re-running a verification the last app died during",
            );
            let app = app.clone();
            let id = ticket.id.clone();
            let project = ticket.project.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    crate::sandbox_verify::sandbox_verify_start_inner(app, id.clone(), project).await
                {
                    crate::ledger::record("sweep_retry_failed", "nautbot", &id, &error);
                }
            });
            return Ok(());
        }
        // The ticket is gone from the board (deleted, or another project's).
        // Dropping it is right; saying so is what keeps the queue honest.
        crate::ledger::record(
            "sweep_retry_dropped",
            "nautbot",
            &ticket_id,
            "orphaned verification's ticket is no longer on the board",
        );
    }

    // 1. Handbacks: the oldest ticket awaiting review that has no verification
    //    running gets one. Reading records is cheap; starting a verify is not,
    //    so only one per tick.
    // Every candidate, oldest first, until one is free. A held ticket must be
    // SKIPPED, not treated as the end of the queue.
    //
    // This asked for the oldest handback and stopped if it was held, so one
    // given-up ticket silently stopped the whole board being worked. The rig
    // measured it on 2026-09-01: RIG-4, in done with zero failures and no hold
    // that could apply to it, drew nothing for 25 minutes across eight ticks
    // while a held ticket sat ahead of it, and was picked up on the very next
    // tick once that one was parked. RIG-1 was starved for over two hours the
    // same way.
    //
    // It also explains a missing announcement: a second held ticket was never
    // reached, so it never got its sweep_gave_up line.
    for ticket in awaiting_review(&tickets) {
        let hold = hold_now(&ticket.id).await;
        if let Hold::GaveUp(failures) = hold {
            // Said once per ticket, not once per tick: giving up is news the
            // first time and noise every three minutes after that.
            if announced.gave_up.insert(ticket.id.clone()) {
                crate::ledger::record(
                    "sweep_gave_up",
                    "nautbot",
                    &ticket.id,
                    &format!(
                        "{} failed verification {failures} times in a row; the sweep will stop \
                         offering it until one passes or the records are cleared",
                        ticket.id
                    ),
                );
            }
        }
        if hold != Hold::None {
            continue;
        }
        {
            crate::ledger::record(
                "sweep_verify",
                "nautbot",
                &ticket.id,
                &format!("{} sat in {} unreviewed", ticket.id, ticket.status),
            );
            let app = app.clone();
            let id = ticket.id.clone();
            let project = ticket.project.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    crate::sandbox_verify::sandbox_verify_start_inner(app, id.clone(), project).await
                {
                    crate::ledger::record("sweep_verify_failed", "nautbot", &id, &error);
                }
            });
            return Ok(());
        }
    }

    // 2. Dispatch: one ready ticket with an owner, oldest first. The nudge
    //    path enforces the ceiling and the quarantine list for us.
    if let Some(ticket) = oldest_ready_with_owner(&tickets) {
        let owner = ticket.owner.clone().unwrap_or_default();
        let message = format!(
            "Check your tickets. Start with {}: {}",
            ticket.id, ticket.title
        );
        match crate::nudge::nudge_agent(app, &owner, &message).await {
            Ok(value) => {
                let delivery = value
                    .get("delivery")
                    .and_then(|d| d.as_str())
                    .unwrap_or("unknown");
                crate::ledger::record(
                    dispatch_kind(delivery),
                    &owner,
                    &ticket.id,
                    &format!("{delivery}: {}", ticket.title),
                );
            }
            Err(error) => {
                // The refusals that arrive as an error rather than as data: an
                // owner who is not on the roster, and a quarantined agent. Not
                // the spend ceiling, which never gets this far (see `delivered`).
                crate::ledger::record("sweep_refused", &owner, &ticket.id, &error);
            }
        }
    }
    Ok(())
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
    let mut out: Vec<&crate::project_management::TicketRecord> =
        tickets.iter().filter(|t| awaits_review(&t.status)).collect();
    out.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    out
}

fn oldest_ready_with_owner(
    tickets: &[crate::project_management::TicketRecord],
) -> Option<&crate::project_management::TicketRecord> {
    tickets
        .iter()
        .filter(|t| t.status == "ready")
        .filter(|t| {
            t.owner
                .as_deref()
                .map(|o| !o.trim().is_empty())
                .unwrap_or(false)
        })
        .min_by(|a, b| a.updated_at.cmp(&b.updated_at))
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
}

/// How long after a finished verification before the same ticket may draw
/// another. Long enough that a broken verify cannot spin; short enough that a
/// real fix lands within a coffee break.
const VERIFY_COOLDOWN: chrono::Duration = chrono::Duration::minutes(30);

/// Consecutive failures before the sweep stops offering this ticket.
const MAX_VERIFY_ATTEMPTS: usize = 3;

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

    // A record stuck in `running` because its app died is NOT in flight after an
    // hour. Without that the zombie record of 2026-08-31 would have meant a
    // ticket that could never be verified again.
    let hour_ago = now - chrono::Duration::hours(1);
    if mine
        .iter()
        .any(|r| r.status == "running" && at(r).map(|t| t > hour_ago).unwrap_or(false))
    {
        return Hold::InFlight;
    }

    // Consecutive failures since the last pass. A ticket that has ever been
    // verified green starts its count again, so fixing the repo re-opens it.
    let mut failures = 0usize;
    let mut ordered: Vec<&crate::sandbox_verify::VerifyRecord> = mine.clone();
    ordered.sort_by_key(|r| r.updated_at.clone());
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

async fn hold_now(ticket_id: &str) -> Hold {
    let records = crate::sandbox_verify::sandbox_verify_records()
        .await
        .unwrap_or_default();
    hold_for(&records, ticket_id, chrono::Utc::now())
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

    fn record(ticket: &str, status: &str, updated: &str) -> crate::sandbox_verify::VerifyRecord {
        crate::sandbox_verify::VerifyRecord {
            id: format!("{ticket}-{updated}"),
            run_id: "r".into(),
            ticket_id: ticket.into(),
            project: "RIG".into(),
            repo_path: String::new(),
            provider_kind: "gitvm-cli".into(),
            sandbox_id: String::new(),
            public_url: String::new(),
            error: String::new(),
            status: status.into(),
            steps: vec![],
            log_dir: String::new(),
            video_path: None,
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
            vec!["HELD", "FREE"],
            "both candidates are offered, oldest first"
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
        assert_eq!(seen, vec!["HELD1", "HELD2", "FREE"]);
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
    fn handbacks_are_picked_oldest_first_and_ready_needs_an_owner() {
        let tickets = vec![
            ticket("A", "ready", None, "2026-08-01"),          // no owner: skipped
            ticket("B", "ready", Some("claude"), "2026-08-03"),
            ticket("C", "ready", Some("codex"), "2026-08-02"),  // older: wins
            ticket("D", "done", Some("nautbot"), "2026-08-05"),
            ticket("E", "review", Some("nautbot"), "2026-08-04"), // older: wins
            ticket("F", "complete", Some("nautbot"), "2026-07-01"),
        ];
        assert_eq!(awaiting_review(&tickets).first().map(|t| t.id.as_str()), Some("E"));
        assert_eq!(oldest_ready_with_owner(&tickets).map(|t| t.id.as_str()), Some("C"));
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
        assert!(oldest_ready_with_owner(&[]).is_none());
    }

    #[test]
    fn complete_is_never_a_handback() {
        // The sweep must not re-verify finished work forever.
        assert!(!awaits_review("complete"));
        assert!(!awaits_review("in_progress"));
        assert!(awaits_review("done"));
        assert!(awaits_review("review"));
    }
}
