// XNAUT-313. Issue -> ticket -> root cause -> fix, joined and searchable.
//
// The same failures were diagnosed from scratch every time. Sign-off escalated
// XNAUT-305 four separate times on four different pieces of bad evidence, and
// each was investigated as if new. The material to recognise a repeat was
// already on disk in four stores that share one join key, the ticket id, and
// nothing read across them.
//
// So this is a join, not a graph and not a new store. Nothing here writes
// anything; it reads what the run registry, the jury and the control repo have
// already recorded, and answers one question: has this exact shape of failure
// been seen before, and what closed it last time.

use crate::run_control::{RunManifest, RunState};
use std::path::Path;

/// One failure, with as much of its story as the stores can supply. `cause`
/// and `fix` are frequently absent, and that is information too: an incident
/// with neither is one nobody ever got to the bottom of.
#[derive(Debug, Clone, PartialEq)]
pub struct Incident {
    pub ticket: String,
    pub run_id: Option<String>,
    /// Epoch millis. Ordering only; incidents from different stores are not
    /// guaranteed to agree to the second.
    pub at: i64,
    /// The failure as it was first recorded, verbatim.
    pub signal: String,
    /// Why it happened, once somebody said so.
    pub cause: Option<String>,
    /// The commit that closed it.
    pub fix: Option<String>,
}

impl Incident {
    /// Did anybody get to the bottom of this one?
    pub fn resolved(&self) -> bool {
        self.cause.is_some() || self.fix.is_some()
    }
}

/// The shape of a failure, with everything that makes one occurrence differ
/// from the next stripped out: run ids, shas, paths, pids, counts, times.
///
/// This is the whole trick, and it is deliberately blunt. "agent binary not
/// found: codex (install it or edit /Users/zelda/...)" and the same line from
/// another machine with another path have to land on one shape, or a repeat
/// looks new. Being too blunt costs a false match, which shows the owner one
/// extra prior incident; being too precise costs the entire feature.
/// Below this many distinctive words a shape describes a category, not a
/// failure. Four is what it takes to separate the real repeats in this
/// registry from "run failed".
pub const MIN_SHAPE_WORDS: usize = 4;

pub fn shape(signal: &str) -> String {
    let mut out = String::with_capacity(signal.len());
    let mut last_was_space = true;
    for word in signal.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());
        // A path, a ULID, a sha, a number, a duration: all of them identify
        // ONE occurrence, which is exactly what a shape must not carry.
        let noisy = word.is_empty()
            || word.contains('/')
            || word.contains('\\')
            || word.chars().any(|c| c.is_ascii_digit());
        if noisy {
            continue;
        }
        if !last_was_space {
            out.push(' ');
        }
        out.push_str(&word.to_ascii_lowercase());
        last_was_space = false;
    }
    out
}

/// Incidents whose failure has the same shape as `signal`, newest first.
///
/// A run may not match itself: asking "have I seen this before" about the
/// thing that just happened must not answer "yes, that". Everything else is
/// fair game, the same ticket included, because a ticket escalating four times
/// on four pieces of bad evidence is the case this exists for.
pub fn prior<'a>(
    incidents: &'a [Incident],
    signal: &str,
    exclude_run: Option<&str>,
) -> Vec<&'a Incident> {
    let wanted = shape(signal);
    // A shape of one or two words is not a shape, it is a category. "run
    // failed" or "admission refused" is true of nearly every incident ever
    // recorded, and matching on it answered "seen 285 times before, on
    // XNAUT-289, STARKCTRL-35, ..." to questions about unrelated tickets
    // (XNAUT-329, when the line reached the Delivery page and the owner could
    // finally read it). Recognition has to mean something; below this it
    // means nothing.
    if wanted.split_whitespace().count() < MIN_SHAPE_WORDS {
        return Vec::new();
    }
    let mut found: Vec<&Incident> = incidents
        .iter()
        .filter(|i| i.run_id.as_deref() != exclude_run || exclude_run.is_none())
        .filter(|i| shape(&i.signal) == wanted)
        .collect();
    found.sort_by_key(|i| std::cmp::Reverse(i.at));
    found
}

/// One line for the owner, or nothing. Attached to an escalation so a repeat
/// arrives already recognised instead of being investigated again.
pub fn recognised(incidents: &[Incident], signal: &str, exclude_run: Option<&str>) -> Option<String> {
    let prior = prior(incidents, signal, exclude_run);
    let first = prior.first()?;
    // Distinct tickets, in order of recency. `dedup` alone only collapses
    // neighbours, so the same ticket used to be listed several times while
    // the fourth distinct one was cut for space. An incident from a run that
    // carried no ticket has nothing to name, so it is counted, not listed.
    let tickets: Vec<&str> = {
        let mut seen: Vec<&str> = Vec::new();
        for t in prior.iter().map(|i| i.ticket.trim()).filter(|t| !t.is_empty()) {
            if !seen.contains(&t) {
                seen.push(t);
            }
        }
        seen.truncate(4);
        seen
    };
    if tickets.is_empty() {
        return None;
    }
    let closed = match (&first.cause, &first.fix) {
        (_, Some(sha)) => format!("; closed last time by {}", &sha[..sha.len().min(8)]),
        (Some(cause), None) => format!("; last time: {}", cause.trim()),
        (None, None) => "; nobody got to the bottom of it then either".into(),
    };
    Some(format!(
        "Seen {} time{} before, on {}{closed}.",
        prior.len(),
        if prior.len() == 1 { "" } else { "s" },
        tickets.join(", "),
    ))
}

/// Every failure the run registry recorded. A run that failed carries its own
/// signal; a run that succeeded is not an incident.
pub fn from_runs(registry: &Path) -> Result<Vec<Incident>, String> {
    let mut out = Vec::new();
    for id in crate::run_control::list_ids_in(registry)? {
        let Ok(run) = crate::run_control::load_manifest_in(registry, &id) else {
            continue;
        };
        if !failed(&run) || run.last_signal.trim().is_empty() {
            continue;
        }
        out.push(Incident {
            ticket: run.ticket.clone().unwrap_or_default(),
            run_id: Some(run.run_id.clone()),
            at: run.started_at,
            signal: run.last_signal.clone(),
            cause: None,
            fix: None,
        });
    }
    Ok(out)
}

fn failed(run: &RunManifest) -> bool {
    matches!(
        run.state,
        RunState::Failed | RunState::Degraded | RunState::Undead
    )
}

/// Every escalation and revert the jury recorded. The jury is the store that
/// actually writes down WHY, so these are the incidents most likely to carry a
/// cause.
pub fn from_jury(store: &Path) -> Vec<Incident> {
    let Ok(entries) = std::fs::read_dir(store) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if entry.path().extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Ok(job) =
            serde_json::from_slice::<crate::jury::Job>(&std::fs::read(entry.path()).unwrap_or_default())
        else {
            continue;
        };
        if !["owner_required", "reverted", "revoked", "superseded"].contains(&job.state.as_str())
            || job.reason.trim().is_empty()
        {
            continue;
        }
        out.push(Incident {
            ticket: job.ticket.clone(),
            run_id: job.author_run.clone(),
            at: job.deadline,
            signal: job.reason.lines().next().unwrap_or_default().to_string(),
            cause: None,
            fix: job.signoff.as_ref().map(|s| s.merge_sha.clone()),
        });
    }
    out
}

/// The join. A ticket's handback names the commits that closed its work, so
/// every incident on that ticket gains a fix; its summary is the nearest thing
/// the stores keep to a stated cause.
///
/// Takes the handbacks rather than the tickets, because that is all it needs
/// and a ticket is an awkward thing to build in a test.
pub fn join(
    mut incidents: Vec<Incident>,
    handbacks: &[(String, crate::handback::Handback)],
) -> Vec<Incident> {
    for incident in incidents.iter_mut() {
        let Some((_, handback)) = handbacks.iter().find(|(id, _)| *id == incident.ticket) else {
            continue;
        };
        if incident.fix.is_none() {
            incident.fix = handback.commits.first().cloned();
        }
        if incident.cause.is_none() && !handback.summary.trim().is_empty() {
            incident.cause = Some(handback.summary.clone());
        }
    }
    incidents.sort_by_key(|i| std::cmp::Reverse(i.at));
    incidents
}

/// Every incident these stores know about, joined. The one entry point
/// production uses.
pub fn all(
    registry: &Path,
    tickets: &[crate::project_management::TicketRecord],
) -> Result<Vec<Incident>, String> {
    let mut incidents = from_runs(registry)?;
    incidents.extend(from_jury(&registry.join("jury")));
    let handbacks: Vec<(String, crate::handback::Handback)> = tickets
        .iter()
        .filter_map(|t| t.handback.clone().map(|h| (t.id.clone(), h)))
        .collect();
    Ok(join(incidents, &handbacks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incident(ticket: &str, at: i64, signal: &str) -> Incident {
        Incident {
            ticket: ticket.into(),
            run_id: Some(format!("run-{ticket}-{at}")),
            at,
            signal: signal.into(),
            cause: None,
            fix: None,
        }
    }

    #[test]
    fn one_failure_has_one_shape_however_many_ways_it_is_written() {
        // The same fault from two machines, with different paths and a
        // different run id in the text, has to land on one shape or a repeat
        // reads as new. This is the case that made codex look like four
        // unrelated problems on 2026-09-09.
        let a = "agent binary not found: codex (install it or edit /Users/zelda/Library/agents.toml)";
        let b = "agent binary not found: codex (install it or edit /Users/cand0rian/Library/agents.toml)";
        assert_eq!(shape(a), shape(b));
        assert_eq!(shape(a), "agent binary not found codex install it or edit");

        // A different fault is a different shape.
        assert_ne!(shape(a), shape("agent launch binary not found: codex"));

        // Nothing but noise has no shape, and no shape never matches, which
        // is what stops an empty signal recognising everything.
        assert_eq!(shape("01M23A2XRN9SJ9Z2BYH03M03BM /tmp/x 42"), "");
        assert!(prior(&[incident("XNAUT-1", 1, "anything")], "  ", None).is_empty());
    }

    #[test]
    fn a_repeat_is_recognised_across_tickets_and_never_by_itself() {
        let incidents = vec![
            incident("XNAUT-88", 300, "agent binary not found: codex (edit /a/b)"),
            incident("XNAUT-310", 200, "agent binary not found: codex (edit /c/d)"),
            incident("XNAUT-75", 100, "worktree absent; worktree branch mismatch"),
        ];

        let found = prior(&incidents, "agent binary not found: codex (edit /e/f)", None);
        assert_eq!(found.len(), 2, "both codex failures, not the worktree one");
        assert_eq!(found[0].ticket, "XNAUT-88", "newest first");

        // Asking about the occurrence that just happened must not answer
        // "yes, that one".
        let self_run = incidents[0].run_id.clone().unwrap();
        let found = prior(&incidents, &incidents[0].signal, Some(&self_run));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].ticket, "XNAUT-310");

        // A shape nobody has seen returns nothing rather than the nearest
        // thing, because a wrong prior incident is worse than none.
        assert!(prior(&incidents, "the disk is full", None).is_empty());
    }

    #[test]
    fn a_shape_too_short_to_mean_anything_recognises_nothing() {
        // "admission refused" is true of hundreds of incidents. Before this
        // guard the Delivery page's Learnings line read "Seen 285 times
        // before, on XNAUT-289, STARKCTRL-35, ..." under tickets that had
        // nothing to do with any of them.
        let broad: Vec<Incident> = (0..40)
            .map(|n| incident("XNAUT-1", n, "run 7 failed"))
            .collect();
        assert_eq!(shape("run 7 failed"), "run failed", "two words is a category");
        assert!(prior(&broad, "run 9 failed", None).is_empty());
        assert_eq!(recognised(&broad, "run 9 failed", None), None);

        // A shape with enough of its own words still recognises.
        let real = vec![
            incident("XNAUT-2", 1, "agent binary not found: codex, install it or edit agents.toml"),
            incident("XNAUT-3", 2, "agent binary not found: codex, install it or edit agents.toml"),
        ];
        assert_eq!(prior(&real, "agent binary not found: codex, install it or edit agents.toml", None).len(), 2);
    }

    #[test]
    fn the_tickets_named_are_distinct_and_an_untagged_incident_is_only_counted() {
        let long = "the integration build refused to merge because the worktree had moved underneath it";
        let mut all = vec![
            incident("XNAUT-5", 1, long),
            incident("XNAUT-5", 2, long),
            incident("XNAUT-6", 3, long),
            incident("XNAUT-5", 4, long),
        ];
        all.push(incident("", 5, long));
        let line = recognised(&all, long, None).expect("a real repeat is recognised");
        assert!(line.starts_with("Seen 5 times before, on "), "all five counted: {line}");
        assert_eq!(line.matches("XNAUT-5").count(), 1, "one ticket named once: {line}");
        assert!(line.contains("XNAUT-6"), "and the other ticket is not crowded out: {line}");
        assert!(!line.contains(", ,") && !line.contains("on ,"), "no empty ticket in the list: {line}");

        // An incident nobody can attribute names nothing, so it says nothing.
        let orphans = vec![incident("", 1, long), incident("", 2, long)];
        assert_eq!(recognised(&orphans, long, None), None);
    }

    #[test]
    fn the_line_for_the_owner_says_what_closed_it_last_time() {
        let mut fixed = incident("XNAUT-305", 200, "evidence has no test totals");
        fixed.fix = Some("4e22fae4ae316c07f3c1149439409a827300ba48".into());
        let mut explained = incident("XNAUT-266", 100, "evidence has no test totals");
        explained.cause = Some("the log tail cut the totals off".into());
        let bare = incident("XNAUT-44", 50, "evidence has no test totals");

        let line = recognised(&[fixed.clone(), explained.clone(), bare.clone()],
                              "evidence has no test totals", None)
            .expect("three priors");
        assert!(line.contains("Seen 3 times before"), "{line}");
        assert!(line.contains("XNAUT-305"), "{line}");
        assert!(line.contains("closed last time by 4e22fae4"), "{line}");

        // No fix, but somebody said why.
        let line = recognised(&[explained, bare.clone()], "evidence has no test totals", None).unwrap();
        assert!(line.contains("last time: the log tail cut the totals off"), "{line}");

        // Nobody ever got to the bottom of it, and the line says so rather
        // than implying it was handled.
        let line = recognised(&[bare], "evidence has no test totals", None).unwrap();
        assert!(line.contains("Seen 1 time before"), "{line}");
        assert!(line.contains("nobody got to the bottom of it"), "{line}");

        // Nothing seen, nothing said.
        assert!(recognised(&[], "brand new failure", None).is_none());
    }

    #[test]
    fn the_join_gives_every_incident_on_a_ticket_the_commit_that_closed_it() {
        let handback = crate::handback::Handback {
            ticket: "XNAUT-305".into(),
            summary: "the log tail cut the totals off".into(),
            commits: vec!["abc1234def".into()],
            ..Default::default()
        };

        let joined = join(
            vec![
                incident("XNAUT-305", 100, "escalated once"),
                incident("XNAUT-305", 200, "escalated again"),
                incident("XNAUT-999", 150, "a ticket with no handback"),
            ],
            &[("XNAUT-305".to_string(), handback)],
        );

        assert_eq!(joined[0].at, 200, "newest first");
        let on_305: Vec<&Incident> = joined.iter().filter(|i| i.ticket == "XNAUT-305").collect();
        assert_eq!(on_305.len(), 2);
        for i in on_305 {
            assert_eq!(i.fix.as_deref(), Some("abc1234def"), "both join to the fix");
            assert!(i.resolved());
        }

        let orphan = joined.iter().find(|i| i.ticket == "XNAUT-999").unwrap();
        assert_eq!(orphan.fix, None);
        assert!(
            !orphan.resolved(),
            "an incident nobody closed must not look closed"
        );
    }
}
