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

impl Announced {
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

pub fn spawn_sweep_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // A first tick immediately after start would race the app's own
        // initialization (settings, PM repo, hook server). One interval of
        // patience costs nothing and avoids a cold-start false alarm.
        tokio::time::sleep(TICK).await;
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
            tokio::time::sleep(TICK).await;
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
    let records = crate::sandbox_verify::sandbox_verify_records()
        .await
        .unwrap_or_default();

    let plan = plan_fleet(
        &tickets,
        &records,
        next_retry().as_deref(),
        chrono::Utc::now(),
    );
    for action in plan {
        run_action(app, announced, action).await;
    }
    Ok(())
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
            // An owner woken in the last ten minutes, by anyone, is left
            // alone: on 2026-09-06 NautBot's triage woke grok and codex, and
            // two minutes later this arm woke them again for the same
            // tickets, retiring the young sessions and launching duplicates.
            if recently_woken(&owner, chrono::Utc::now()) {
                return;
            }
            let message = format!("Check your tickets. Start with {ticket}: {title}");
            let (kind, reason) = match crate::nudge::nudge_agent(app, &owner, &message).await {
                Ok(value) => {
                    let delivery = value
                        .get("delivery")
                        .and_then(|d| d.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    (dispatch_kind(&delivery), format!("{delivery}: {title}"))
                }
                // The refusals that arrive as an error rather than as data: an
                // owner who is not on the roster, and a quarantined agent. Not
                // the spend ceiling, which never gets this far (see
                // `dispatch_kind`).
                Err(error) => ("sweep_refused", error),
            };
            if announced.dispatch_is_news(format!("{owner}:{ticket}"), kind, &reason) {
                crate::ledger::record(kind, &owner, &ticket, &reason);
            }
        }
        Action::Triage { tickets } => {
            // Once per distinct list, and never more than every thirty
            // minutes: NautBot assigned five of eight at 10:36 and the sweep
            // handed it the next eight at 10:37, mid-work. A triage list is
            // a batch, not a stream.
            let key = tickets.join("|");
            let now_ms = chrono::Utc::now().timestamp_millis();
            if announced.triage.as_deref() == Some(key.as_str())
                || announced.triage_at.is_some_and(|at| now_ms - at < TRIAGE_EVERY_MS)
            {
                return;
            }
            announced.triage = Some(key);
            announced.triage_at = Some(now_ms);
            let message = format!(
                "Triage: these ready tickets have no owner. For each one, either assign an owner and \
                 dispatch it (dispatch_ticket), or set it back to inbox with one line saying why it is \
                 not ready. Do not start the work yourself.\n{}",
                tickets.iter().map(|t| format!("- {t}")).collect::<Vec<_>>().join("\n")
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
                &format!("{outcome}: {} unowned ready ticket(s)", tickets.len()),
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

    let unowned: Vec<String> = ready_unowned_urgent(tickets)
        .iter()
        .map(|t| format!("{}: {}", t.id, t.title))
        .collect();
    if !unowned.is_empty() {
        actions.push(Action::Triage { tickets: unowned });
    }

    let mut woken: std::collections::HashSet<String> = std::collections::HashSet::new();
    for ticket in ready_with_owner(tickets) {
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

/// Every ready ticket that names an owner, oldest first.
///
/// A list rather than the single oldest, because a fleet wakes every owner with
/// work and not just the one whose ticket has been waiting longest. Sorted so
/// that when an owner holds several, the one they are handed is the oldest.
/// Ready, unowned, and urgent: what NautBot is woken to triage. Oldest first
/// and capped, so one wake is a list a person could read too.
fn ready_unowned_urgent(
    tickets: &[crate::project_management::TicketRecord],
) -> Vec<&crate::project_management::TicketRecord> {
    let mut out: Vec<&crate::project_management::TicketRecord> = tickets
        .iter()
        .filter(|t| t.status == "ready")
        .filter(|t| t.owner.as_deref().map(str::trim).unwrap_or("").is_empty())
        .filter(|t| matches!(t.priority.as_str(), "high" | "critical"))
        .collect();
    out.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    out.truncate(MAX_TRIAGE_PER_WAKE);
    out
}

const MAX_TRIAGE_PER_WAKE: usize = 8;
const TRIAGE_EVERY_MS: i64 = 30 * 60 * 1000;
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
        assert_eq!(verifying, vec!["FLEET-6"], "{plan:?}");
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
        let green = verdict("FLEET-6", "passed", 0);
        // The next pass, with FLEET-6's run finished, picks up FLEET-7: the
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
        assert_eq!(verifying, vec!["FLEET-7"], "{second:?}");
        crate::ledger::record("sweep_verify", "nautbot", "FLEET-7", "FLEET-7 sat in review unreviewed");
        let red = verdict("FLEET-7", "failed", 1);
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
            on_board(&repo, "FLEET-6").status,
            "complete",
            "the verified ticket closed"
        );
        assert!(
            on_board(&repo, "FLEET-6")
                .body
                .contains(&format!("Record: `{}`", green.id)),
            "and carries the evidence that closed it"
        );
        assert_eq!(
            on_board(&repo, "FLEET-7").status,
            "review",
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
        assert_eq!(by_id("FLEET-6").verdict, "passed");
        assert!(by_id("FLEET-6").verified, "closed on the strength of a run");
        assert!(!by_id("FLEET-6").unverified_close);
        assert_eq!(by_id("FLEET-7").verdict, "failed");
        assert!(
            !by_id("FLEET-7").verified && !by_id("FLEET-7").unverified_close,
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
        assert_eq!(verifying, vec!["F-1", "F-2", "F-3"], "oldest first");

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
        assert_eq!(verifying, vec!["F-3"], "one slot left: {plan:?}");

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
        assert_eq!(names(&plan), vec!["A-1", "B-1"], "A-2 waits for A-1: {plan:?}");

        let mut live = record("A-1", "running", "2026-09-01T09:59:00+00:00");
        live.project = "XNAUT".into();
        let plan = plan_fleet(&tickets, &[live], None, at("10:00"));
        assert_eq!(names(&plan), vec!["B-1"], "XNAUT is busy, B is not: {plan:?}");

        let plan = plan_fleet(&tickets, &[], Some("A-2"), at("10:00"));
        assert_eq!(names(&plan), vec!["A-2", "B-1"], "a retry holds its project: {plan:?}");
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
