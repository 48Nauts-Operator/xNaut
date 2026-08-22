// Which of these actually need YOU?
//
// 122 tickets sit in review. Two of them, side by side in the same list, with
// the same colour and the same weight:
//
//   "German reply to the Securosys engineer, conceding both of his points"
//   "Homebrew cask is four releases stale — refresh it and automate it"
//
// The first leaves the building, in the owner's name, conceding a technical
// argument to another human. Nobody else can send it. The second is mechanical,
// internal, and revertible in one command.
//
// Nothing in the record distinguishes them. `status` is "review" for both,
// `type` is "task" for both. So the queue reads as 122 obligations when it is
// perhaps eight, and the rational response to 122 undifferentiated obligations
// is to stop opening the queue. That is the actual bug: not too much work, an
// undifferentiated pile.
//
// This module separates them. Not by asking anyone to tag things — a tag nobody
// applies is worse than no tag — but by deriving it, the same way flow_drift
// derives status. Three questions, in the owner's own words:
//
//   EXTERNAL      does it leave the building? a reply to a client, a release,
//                 a public push, money.
//   IRREVERSIBLE  can it be undone by reverting a commit? a sent email cannot.
//                 a tag, a migration, a rotated key, a deletion cannot.
//   COMMITTING    does it foreclose later options? a vendor, an architecture,
//                 a contract.
//
// Anything that trips none of them is TRUST: work the owner has already decided
// to delegate, which should be visible but must not sit in the same queue.
//
// TWO RULES ABOUT BEING WRONG, because this module is wrong sometimes by
// construction — it reads prose, and prose lies.
//
//   1. ERR TOWARDS FLAGGING. A false positive costs a glance. A false negative
//      sends an email in the owner's name that he never saw. Not symmetric, and
//      the thresholds must not pretend they are.
//
//   2. NEVER CLASSIFY SILENTLY. Every verdict carries the evidence that
//      produced it — the matched phrase, in context — so the owner can judge the
//      judgement.
//
// WHERE THE FIRST VERSION FAILED, kept because the lesson is the whole design.
//
// It scanned title AND body for the signal phrases. Run against the real queue
// it flagged 57 of 122 — useless, since a pile of 57 is a pile. The cause was
// not bad phrases, it was the wrong FIELD. Ticket bodies here run to a 2,000
// character median and 24,000 at the tail; they are engineering narrative, and
// narrative mentions everything. "destroy" matched `sandbox.rs
// create/wait_ready/exec/destroy`. "migration" matched a slice declaring
// `outputs: [{name: "migration"}]`. "vendor" matched the path
// `vendor/NautRouter/src/index.ts`. Every one a true string match and a false
// signal.
//
// The fix is scope, not vocabulary. A ticket's TITLE is a claim about what the
// work IS; its body is a discussion of how. So the axes read the title only,
// where "delete" means the work deletes something rather than the prose using
// the word. That alone took 57 down to 26.
//
// The one thing worth reading the body for is an explicit request. "Awaiting
// Andre" is not a topic, it is someone addressing the owner directly, and it
// appears deep in bodies where the summary cannot reach. Those get their own
// axis and sort above everything, because they are the only signals with no
// inference in them at all.

use serde::{Deserialize, Serialize};

/// A ticket, reduced to what the gate reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateTicket {
    pub id: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub title: String,
    /// The long field. Named `body` in the record; `description` on some.
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "type", default)]
    pub ticket_type: String,
}

/// Why a ticket was flagged, in the words that flagged it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GateSignal {
    /// external | irreversible | committing
    pub axis: String,
    /// The phrase that matched, so the owner can judge the judgement.
    pub matched: String,
    /// ~90 characters around the match. The whole point: a signal without its
    /// sentence is an assertion, and assertions have to be checked by hand.
    pub context: String,
}

/// One ticket's verdict.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GateVerdict {
    pub id: String,
    pub project: String,
    pub title: String,
    /// yours | trust
    pub lane: String,
    pub signals: Vec<GateSignal>,
}

/// Signals read from the TITLE only. See the note above: a title says what the
/// work is, a body discusses how, and the body's vocabulary is unusable.
///
/// Short stems are safe here in a way they were not against bodies. "delete" in
/// a title is the work deleting something; in a body it is a paragraph about
/// deletion.
const EXTERNAL: &[&str] = &[
    "reply to", "email to", "send to", "publish", "release", "cask", "invoice",
    "pricing", "client", "customer", "announce",
];
const IRREVERSIBLE: &[&str] =
    &["migrate", "delete", "revoke", "rotate", "tag the release", "force push", "wipe"];
const COMMITTING: &[&str] =
    &["vendor", "contract", "licence", "license", "deprecate", "breaking change"];

/// Someone addressing the owner directly, anywhere in the ticket.
///
/// The only signals with no inference in them: not a topic the work touches,
/// but a person asking. They live deep in bodies — "Awaiting Andre: verification,
/// and the push of NautGate c0045ad" is the last line of a 3,000 character
/// ticket — so this is the one axis that reads the whole record, and the one
/// that sorts above everything else.
const ASKED_FOR_YOU: &[&str] = &[
    "awaiting andre", "awaiting andré", "needs your", "sign-off", "signoff",
    "ready for andre", "for andre to test", "awaiting review by andre",
];

/// ~90 characters around a match, collapsed to one line.
///
/// Character-indexed rather than byte-indexed: ticket bodies carry é, —, and →,
/// and slicing a &str mid-codepoint panics. A classifier that crashes on the
/// French spelling of the owner's own name is not a classifier.
fn context_around(hay: &str, at_char: usize, len_chars: usize) -> String {
    let chars: Vec<char> = hay.chars().collect();
    let start = at_char.saturating_sub(35);
    let end = (at_char + len_chars + 55).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

/// Every signal on one axis. Lowercased comparison, original text for context.
fn scan(hay_lower: &str, hay_original: &str, needles: &[&str], axis: &str) -> Vec<GateSignal> {
    let mut out = Vec::new();
    for needle in needles {
        if let Some(byte_at) = hay_lower.find(needle) {
            // Byte offset → char offset, so context slicing stays on boundaries.
            let char_at = hay_lower[..byte_at].chars().count();
            out.push(GateSignal {
                axis: axis.to_string(),
                matched: (*needle).to_string(),
                context: context_around(hay_original, char_at, needle.chars().count()),
            });
        }
    }
    out
}

/// Classify one ticket. Pure.
pub fn classify(ticket: &GateTicket) -> GateVerdict {
    let title_lower = ticket.title.to_lowercase();
    let whole = format!("{}\n{}", ticket.title, ticket.body);
    let whole_lower = whole.to_lowercase();

    // The three axes read the title. Scoping them to the body is what made the
    // first version flag 57 of 122.
    let mut signals = scan(&title_lower, &ticket.title, EXTERNAL, "external");
    signals.extend(scan(&title_lower, &ticket.title, IRREVERSIBLE, "irreversible"));
    signals.extend(scan(&title_lower, &ticket.title, COMMITTING, "committing"));
    // An explicit ask is read from the whole record, because that is where it
    // is written.
    signals.extend(scan(&whole_lower, &whole, ASKED_FOR_YOU, "asked-for-you"));

    // One signal is enough. Requiring two would be the symmetric-error mistake:
    // "German reply to the Securosys engineer" trips `reply to` and nothing
    // else, and it is the most clearly-yours item in the whole queue.
    let lane = if signals.is_empty() { "trust" } else { "yours" };

    GateVerdict {
        id: ticket.id.clone(),
        project: ticket.project.clone(),
        title: ticket.title.clone(),
        lane: lane.to_string(),
        signals,
    }
}

/// The queue, split. Only `review` tickets are considered — this answers "what
/// is waiting for me", and work still in progress is not waiting for anyone.
#[tauri::command]
pub fn review_gate(tickets: Vec<GateTicket>) -> Vec<GateVerdict> {
    let mut out: Vec<GateVerdict> = tickets
        .iter()
        .filter(|t| t.status == "review")
        .map(classify)
        .collect();
    // Yours first, then by how many axes tripped: a ticket that is external AND
    // irreversible outranks one that is merely external.
    //
    // Ranked by an explicit key, NOT by comparing the lane strings — "trust"
    // sorts before "yours" alphabetically, which put the whole trusted pile at
    // the top of a queue whose entire purpose is to surface the few that are
    // not trusted. Caught by the ordering test; it would have been invisible in
    // a UI that merely looked full.
    fn lane_rank(lane: &str) -> u8 {
        match lane {
            "yours" => 0,
            _ => 1,
        }
    }
    // Someone asking for the owner by name outranks anything inferred from a
    // title. It is the only signal that carries no guess.
    fn asked(v: &GateVerdict) -> u8 {
        u8::from(!v.signals.iter().any(|s| s.axis == "asked-for-you"))
    }
    out.sort_by(|a, b| {
        lane_rank(&a.lane)
            .cmp(&lane_rank(&b.lane))
            .then_with(|| asked(a).cmp(&asked(b)))
            .then_with(|| b.signals.len().cmp(&a.signals.len()))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, title: &str, body: &str) -> GateTicket {
        GateTicket {
            id: id.into(),
            project: "XNAUT".into(),
            title: title.into(),
            body: body.into(),
            status: "review".into(),
            ticket_type: "task".into(),
        }
    }

    /// The two real tickets this module was written for. If it cannot tell
    /// these apart it has no reason to exist.
    #[test]
    fn the_securosys_reply_is_the_owners_and_the_cask_is_not() {
        let reply = classify(&t(
            "XNAUT-1",
            "German reply to the Securosys engineer, conceding both of his points",
            "Drafted in German. Concedes the HSM latency point.",
        ));
        assert_eq!(reply.lane, "yours");
        assert!(!reply.signals.is_empty(), "flagged with no evidence");

        let cask = classify(&t(
            "XNAUT-2",
            "Refresh the stale build formula and automate it in the pipeline",
            "It is four versions behind. Regenerate the checksum and wire it into CI.",
        ));
        assert_eq!(cask.lane, "trust", "signals: {:?}", cask.signals);
    }

    #[test]
    fn every_flag_carries_the_sentence_that_caused_it() {
        // The rule that keeps this auditable. A lane with no evidence is an
        // assertion, and the owner would have to re-read all 122 to trust it.
        let v = classify(&t("X", "Rotate the key after the leak", "The token is in a public repo."));
        assert_eq!(v.lane, "yours");
        for s in &v.signals {
            assert!(!s.matched.is_empty(), "signal with no phrase");
            assert!(s.context.contains(&s.matched) || s.context.to_lowercase().contains(&s.matched),
                "context {:?} does not contain {:?}", s.context, s.matched);
        }
    }

    #[test]
    fn a_single_axis_is_enough_to_be_yours() {
        // Erring towards flagging, stated as a test. The Securosys reply trips
        // exactly one phrase; requiring two would drop the clearest case.
        let v = classify(&t("X", "Reply to the auditor", "One paragraph."));
        assert_eq!(v.signals.len(), 1);
        assert_eq!(v.lane, "yours");
    }

    #[test]
    fn ordinary_internal_work_stays_out_of_the_way() {
        // The 114. If routine refactors land in "yours" the queue is a pile
        // again and the owner stops opening it.
        for (title, body) in [
            ("Fix the scrollbar on the running agent list", "overflow hidden clips rows"),
            ("Extract the parser into its own module", "no behaviour change"),
            ("Add a test for the drift linter", "three rules, deterministic"),
        ] {
            let v = classify(&t("X", title, body));
            assert_eq!(v.lane, "trust", "{title} was flagged by {:?}", v.signals);
        }
    }

    #[test]
    fn a_ticket_that_is_both_external_and_irreversible_sorts_first() {
        let out = review_gate(vec![
            t("B", "Reply to the client", "just a note"),
            t("A", "Publish the release and git tag it", "goes to users"),
            t("C", "Rename a local variable", "cosmetic"),
        ]);
        assert_eq!(out[0].id, "A", "multi-axis ticket did not sort first: {out:?}");
        assert_eq!(out.last().unwrap().lane, "trust");
    }

    #[test]
    fn only_work_that_is_actually_waiting_is_considered() {
        let mut in_progress = t("X", "Reply to the client", "");
        in_progress.status = "in_progress".into();
        assert!(review_gate(vec![in_progress]).is_empty());
    }

    #[test]
    fn accented_text_does_not_panic_the_context_window() {
        // Ticket bodies carry é, —, → and the owner's own name is André. Byte
        // slicing a multi-byte codepoint panics; this is the regression guard.
        let v = classify(&t(
            "X",
            "Awaiting André — décision requise",
            "Le système attend une réponse → merci. Awaiting Andre sign-off.",
        ));
        assert_eq!(v.lane, "yours");
        assert!(!v.signals.is_empty());
    }

    /// The 57-of-122 failure, frozen. Each string below is real prose from a
    /// real ticket body that the first version flagged. All are true string
    /// matches and all are false signals; scoping the axes to the title is what
    /// makes them silent.
    #[test]
    fn engineering_narrative_in_a_body_is_not_a_signal() {
        for (title, body) in [
            ("Designer tab — Paper surface with real sandbox builds",
             "src-tauri/src/sandbox.rs (create/wait_ready/exec/destroy -> SandboxHandle)"),
            ("Typed handoff between build slices",
             "A slice declaring `outputs: [{name: \"migration\", data_type: \"object\"}]`"),
            ("An ng_ key cannot reach the plan, so every route is rejected",
             "vendor/NautRouter forwardAnthropic recognises the header at src/index.ts:585"),
            ("Track token usage and cost from rollout transcripts",
             "We track Claude spend via ccusage. For Codex we track nothing yet."),
            ("Build log — a live viewer over a durable master log",
             "the reason was destroyed by the next message. Reconstructing it is lossy."),
        ] {
            let v = classify(&t("X", title, body));
            assert_eq!(v.lane, "trust", "{title:?} flagged by {:?}", v.signals);
        }
    }

    /// An explicit ask lives at the end of a long body, not in the title, and
    /// must still be found — it is the only signal with no inference in it.
    #[test]
    fn a_direct_request_is_found_however_deep_it_is_buried() {
        let body = format!(
            "{}\n\nAwaiting Andre: verification, and the push of NautGate c0045ad.",
            "Init order and four open decisions are listed in the doc. ".repeat(40)
        );
        let v = classify(&t("XNAUT-216", "Phase 5: one bundle where two records verify", &body));
        assert_eq!(v.lane, "yours");
        assert!(v.signals.iter().any(|s| s.axis == "asked-for-you"), "{:?}", v.signals);
    }

    /// Being asked by name outranks anything guessed from a title.
    #[test]
    fn a_direct_request_sorts_above_an_inferred_signal() {
        let out = review_gate(vec![
            t("B", "Publish the release and delete the old cask", "routine"),
            t("A", "Some internal refactor", "…done. Awaiting Andre."),
        ]);
        assert_eq!(out[0].id, "A", "an explicit ask did not sort first: {out:?}");
    }

    #[test]
    fn the_gate_never_approves_anything() {
        // Routing attention is the entire contract. If a lane could ever read
        // "approved" this module would be making the decision it exists to
        // route to a human.
        for lane in review_gate(vec![t("A", "Reply to the client", ""), t("B", "tidy up", "")])
            .iter()
            .map(|v| v.lane.as_str())
        {
            assert!(lane == "yours" || lane == "trust", "unexpected lane {lane}");
        }
    }
}
