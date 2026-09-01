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

use std::time::Duration;
use tauri::AppHandle;

/// How often the board is read. Long enough that a busy fleet is not
/// re-examined constantly, short enough that a handback is picked up while the
/// owner is still awake.
const TICK: Duration = Duration::from_secs(180);

/// Statuses that mean "a human or an agent finished something and it needs
/// checking". `review` and `done` are the same claim from an agent's side
/// (project_management.rs makes both hand back to NautBot).
fn awaits_review(status: &str) -> bool {
    matches!(status, "done" | "review")
}

pub fn spawn_sweep_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // A first tick immediately after start would race the app's own
        // initialization (settings, PM repo, hook server). One interval of
        // patience costs nothing and avoids a cold-start false alarm.
        tokio::time::sleep(TICK).await;
        let mut announced_read_only = false;
        loop {
            match tick(&app, &mut announced_read_only).await {
                Ok(()) => {}
                // A sweep that dies silently is the very failure mode this
                // sprint exists to kill, so its own errors go in the ledger.
                Err(error) => {
                    crate::ledger::record("sweep_failed", "nautbot", "", &error);
                }
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn tick(app: &AppHandle, announced_read_only: &mut bool) -> Result<(), String> {
    let switches = crate::switches::load();
    if switches.read_only {
        if !*announced_read_only {
            crate::ledger::record(
                "sweep_paused",
                "nautbot",
                "",
                "read_only kill-switch engaged; the sweep is idle until the owner lifts it",
            );
            *announced_read_only = true;
        }
        return Ok(());
    }
    *announced_read_only = false;

    let repo = crate::project_management::repo_now()?;
    let tickets = crate::project_management::ticket_list_in(&repo, None)?;

    // 1. Handbacks: the oldest ticket awaiting review that has no verification
    //    running gets one. Reading records is cheap; starting a verify is not,
    //    so only one per tick.
    if let Some(ticket) = oldest_awaiting_review(&tickets) {
        if !verify_running_for(&ticket.id).await {
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
                    "sweep_dispatch",
                    &owner,
                    &ticket.id,
                    &format!("{delivery}: {}", ticket.title),
                );
            }
            Err(error) => {
                // A refusal is data, not a failure: the ceiling saying no is
                // the ceiling working. It is recorded and the tick ends.
                crate::ledger::record("sweep_refused", &owner, &ticket.id, &error);
            }
        }
    }
    Ok(())
}

fn oldest_awaiting_review(
    tickets: &[crate::project_management::TicketRecord],
) -> Option<&crate::project_management::TicketRecord> {
    tickets
        .iter()
        .filter(|t| awaits_review(&t.status))
        .min_by(|a, b| a.updated_at.cmp(&b.updated_at))
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

/// Is a verification already in flight for this ticket?
///
/// Reads the same records the pill and the timeline read. A record stuck in
/// `running` because its app died is treated as NOT running after an hour:
/// the alternative is a ticket that can never be verified again, which is how
/// the zombie record of 2026-08-31 would have poisoned the sweep forever.
async fn verify_running_for(ticket_id: &str) -> bool {
    let records = crate::sandbox_verify::sandbox_verify_records()
        .await
        .unwrap_or_default();
    let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
    records.iter().any(|record| {
        record.ticket_id == ticket_id
            && record.status == "running"
            && chrono::DateTime::parse_from_rfc3339(&record.updated_at)
                .map(|at| at.with_timezone(&chrono::Utc) > hour_ago)
                .unwrap_or(false)
    })
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
        assert_eq!(oldest_awaiting_review(&tickets).map(|t| t.id.as_str()), Some("E"));
        assert_eq!(oldest_ready_with_owner(&tickets).map(|t| t.id.as_str()), Some("C"));
    }

    #[test]
    fn an_empty_board_asks_for_nothing() {
        assert!(oldest_awaiting_review(&[]).is_none());
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
