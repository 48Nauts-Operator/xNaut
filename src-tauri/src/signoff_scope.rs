// XNAUT-380. Does the ticket already say what the handback says it did not do?
//
// The sign-off gate used to treat ANY not-done item in a handback as scope the
// agent had cut on its own, and sent it to the owner. On the autonomy run of
// 2026-09-14 (XNAUT-370) that escalation was the only human click in the whole
// loop — dispatch 08:48, done 09:15, verify green 09:24:45, escalated 09:24:50,
// approved 09:25:21 — and it fired on work the ticket itself had already listed
// as out of scope. The agent was sent to the owner for obeying its ticket.
//
// So this module reads both sides as the prose they are, and answers one
// question per item: does anything the ticket or its design document declares
// out of scope account for this? The comparison is word-level rather than
// exact, because the two sentences are written hours apart by different
// authors and "the Windows leg is untested" has to match "not in scope: the
// Windows leg".
//
// The asymmetry is the load-bearing part. Containment runs item -> declaration
// — does the declaration account for the item? — and never the other way, and
// anything unmatched still escalates. Accepting scope nobody declared is the
// failure that matters here; escalating something already covered is only the
// noise we started with.

/// Words that name no work. Two authors' prose shares its connective tissue
/// and little else, so the comparison is made on the rest.
const FILLER: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "for", "in", "on", "at", "by", "from", "with",
    "without", "as", "is", "are", "was", "were", "be", "been", "being", "it", "its", "this",
    "that", "these", "those", "there", "their", "them", "they", "we", "i", "our", "has", "have",
    "had", "will", "would", "should", "shall", "can", "could", "may", "might", "must", "do",
    "does", "did", "done", "still", "yet", "only", "also", "but", "so", "then", "than", "when",
    "which", "what", "while", "any", "all", "each", "into", "here", "left", "over", "up", "out",
    "off", "if", "because", "since", "about", "one", "two", "part", "thing", "work", "item",
];

/// `not` and `no` are absent from the filler above, so they count as words an
/// item names and the bar is that much higher. They are NOT read as polarity:
/// a declaration only exists here because it was found in an out-of-scope
/// context, so "not in scope: a policy switch" accounts for "no policy switch
/// was added" even though one sentence is positive and the other negative.
/// Comparing the two polarities would refuse exactly that pair.
///
/// The share of an item's naming words a declaration has to carry.
const COVERAGE: f64 = 0.6;

/// Markers that introduce deliberately abandoned work, in a heading or inline
/// at the head of a sentence. `accepted not_finished` is the hand-written
/// mechanism this rule replaces (XNAUT-303) and still counts.
const MARKERS: &[&str] = &[
    "deliberately not done",
    "accepted not_finished",
    "not in scope",
    "out of scope",
    "non-goals",
    "non goals",
    "also not done",
    "not doing",
    "will not do",
    "not attempted",
];

/// Clauses that say what an item WAITS on rather than naming one. "the Windows
/// leg is untested; waits on a signing cert" names one piece of unfinished work
/// and one dependency, and no ticket can be expected to declare the dependency
/// under its own "not in scope".
const DEPENDENCY: &[&str] = &[
    "waits on",
    "waits for",
    "waiting on",
    "waiting for",
    "blocked on",
    "blocked by",
    "depends on",
    "pending",
];

/// What the ticket makes of a handback's not-done list.
#[derive(Debug, Default, PartialEq)]
pub struct Acceptance {
    /// Items the ticket or its document already declared out of scope. Named
    /// on the decision, so a reader can see what was accepted and by whom.
    pub accepted: Vec<String>,
    /// Items nothing declared. These still go to the owner, and the card names
    /// only these.
    pub open: Vec<String>,
}

/// Paragraphs, with a wrapped one joined back into a single line. A heading or
/// a bullet starts a new one without needing a blank line before it, because a
/// declaration is written both ways: `## Not in scope` with bullets under it,
/// and a wrapped `Deliberately not done: …` paragraph.
fn blocks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut current = String::new();
    let mut flush = |current: &mut String, out: &mut Vec<String>| {
        let done = std::mem::take(current);
        if !done.trim().is_empty() {
            out.push(done.trim().to_string());
        }
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut current, &mut out);
            continue;
        }
        if trimmed.starts_with('#') {
            flush(&mut current, &mut out);
            out.push(trimmed.to_string());
            continue;
        }
        if is_bullet(trimmed) {
            flush(&mut current, &mut out);
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(trimmed);
    }
    flush(&mut current, &mut out);
    out
}

fn is_bullet(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("• ")
        || line.split_once(['.', ')']).is_some_and(|(n, rest)| {
            !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(' ')
        })
}

/// The text of a heading, or None when this block is body text. Markdown
/// headings and the SHOUTED kind a dispatch body writes (`ACCEPTANCE`,
/// `THE FIX`) both count, because both are what a ticket's sections look like.
fn heading(block: &str) -> Option<String> {
    let trimmed = block.trim();
    if trimmed.starts_with('#') {
        return Some(trimmed.trim_start_matches('#').trim().to_string());
    }
    let letters: Vec<char> = trimmed.chars().filter(|c| c.is_alphabetic()).collect();
    let shouted = letters.len() >= 3
        && letters.iter().all(|c| c.is_uppercase())
        && !is_bullet(trimmed)
        && trimmed.len() <= 60;
    let bold = trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4;
    (shouted || bold).then(|| trimmed.trim_matches('*').trim().to_string())
}

/// One entry per declaration or per not-done item. A semicolon separates two of
/// them in the prose both sides are written in; the whole remainder is kept as
/// an entry too, so an item that spans the lot still matches something.
fn push_entries(out: &mut Vec<String>, text: &str) {
    let whole = clean(text);
    for part in text.split(';') {
        let part = clean(part);
        if !part.is_empty() && part != whole {
            out.push(part);
        }
    }
    if !whole.is_empty() {
        out.push(whole);
    }
}

/// A line stripped of the markup that carries no meaning: bullets, numbering,
/// a leading conjunction, trailing punctuation.
fn clean(line: &str) -> String {
    let mut s = line.trim().trim_start_matches(['-', '*', '•', '·']).trim();
    if let Some((n, rest)) = s.split_once(['.', ')']) {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(' ') {
            s = rest.trim();
        }
    }
    for lead in ["and ", "it ", "we ", "then "] {
        if s.len() > lead.len() && s[..lead.len()].eq_ignore_ascii_case(lead) {
            s = s[lead.len()..].trim();
        }
    }
    s.trim_end_matches(['.', ',', ':', ';', ' '])
        .trim()
        .to_string()
}

/// The work a ticket body or design document declares deliberately abandoned.
/// Two forms are recognised, and both are real in this repository:
///
/// - a heading — `## Not in scope`, or a shouted `NOT IN SCOPE` — whose
///   following paragraphs are declarations until the next heading;
/// - a sentence introduced by a marker and a colon, anywhere in the text:
///   `Deliberately not done: the continuation prompt's REVERTED paragraph`,
///   which is how every design doc in the vault actually writes it.
///
/// The colon is required for the inline form on purpose. Without it, a sentence
/// that merely mentions scope — "items out of scope still escalate" — would
/// turn the rest of its paragraph into a declaration.
pub fn declared_out_of_scope(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut inside = false;
    for block in blocks(text) {
        if let Some(head) = heading(&block) {
            let head = head.to_lowercase();
            inside = MARKERS.iter().any(|m| head.contains(m));
            continue;
        }
        // Slicing happens on the lowercased copy throughout: `to_lowercase`
        // can change a string's length, so an offset found in it is not an
        // offset into the original. The entries are only ever read as words.
        let lower = block.to_lowercase();
        let marker = MARKERS
            .iter()
            .filter_map(|m| {
                let after = lower.find(m)? + m.len();
                let colon = lower[after..].find(':')?;
                // `here`, `either`, `for now`: a word may sit between the
                // marker and its colon, a whole clause may not.
                (lower[after..after + colon].trim().len() <= 12).then_some(after + colon + 1)
            })
            .min();
        if let Some(at) = marker.or(inside.then_some(0)) {
            push_entries(&mut out, &lower[at..]);
        }
    }
    out
}

/// The items a handback's `not_finished` names. Newlines, bullets and
/// semicolons all separate them in real handbacks, so all three do here.
pub fn not_done_items(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || ["nothing", "none", "n/a", "-"].contains(&trimmed.to_lowercase().as_str())
    {
        return vec![];
    }
    blocks(text)
        .iter()
        .flat_map(|block| block.split(';').map(clean).collect::<Vec<_>>())
        .filter(|item| {
            let lower = item.to_lowercase();
            !item.is_empty()
                && lower != "nothing"
                && !DEPENDENCY.iter().any(|d| lower.starts_with(d))
        })
        .collect()
}

/// The naming words of a line: lowercased, stripped of punctuation, filler
/// dropped, and lightly stemmed so "test", "tests" and "tested" compare equal.
/// Nothing clever — a real stemmer would be a dependency, and the only pairs
/// this has to survive are plurals and tenses.
fn naming_words(line: &str) -> Vec<String> {
    let mut words: Vec<String> = line
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() > 1 && !FILLER.contains(&w.to_lowercase().as_str()))
        .map(|w| stem(&w.to_lowercase()))
        .collect();
    words.sort();
    words.dedup();
    words
}

fn stem(word: &str) -> String {
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(root) = word.strip_suffix(suffix) {
            if root.len() >= 4 {
                return root.to_string();
            }
        }
    }
    word.to_string()
}

/// Whether `declaration` accounts for `item`: it carries at least 60% of the
/// item's naming words, and at least two of them unless the item names only
/// one thing. One direction only, because the ticket has to account for the
/// item and not the item for the ticket.
fn accounts_for(declaration: &str, item: &str) -> bool {
    let want = naming_words(item);
    if want.is_empty() {
        return false;
    }
    let have = naming_words(declaration);
    let shared = want.iter().filter(|w| have.contains(w)).count();
    let enough = if want.len() == 1 { 1 } else { 2 };
    shared >= enough && shared as f64 / want.len() as f64 >= COVERAGE
}

/// Which of a handback's not-done items the given declarations account for.
pub fn acceptance(not_finished: &str, declarations: &[String]) -> Acceptance {
    let mut result = Acceptance::default();
    for item in not_done_items(not_finished) {
        if declarations.iter().any(|d| accounts_for(d, &item)) {
            result.accepted.push(item);
        } else {
            result.open.push(item);
        }
    }
    result
}

/// The same, reading the declarations out of a ticket body and the text of
/// every design document linked to it.
pub fn ticket_acceptance(not_finished: &str, body: &str, docs: &[String]) -> Acceptance {
    let mut declarations = declared_out_of_scope(body);
    for doc in docs {
        declarations.extend(declared_out_of_scope(doc));
    }
    acceptance(not_finished, &declarations)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The XNAUT-370 shape that produced the click: a ticket that names its own
    /// out-of-scope work, and a handback that repeats it in its own words.
    const TICKET: &str = "\
THE FIX
Compare the handback's not-done list with the ticket.

NOT IN SCOPE
- the Windows leg, which waits on a signing certificate
- a policy switch for the other 43 projects

ACCEPTANCE
- covered items integrate without a card
";

    #[test]
    fn a_fully_covered_list_is_accepted_and_named() {
        let a = ticket_acceptance(
            "the Windows leg is untested; waits on a signing cert",
            TICKET,
            &[],
        );
        assert_eq!(a.accepted, vec!["the Windows leg is untested".to_string()]);
        assert!(
            a.open.is_empty(),
            "the dependency clause is not scope: {a:?}"
        );
    }

    #[test]
    fn an_undeclared_item_still_escalates() {
        let a = ticket_acceptance("the SSH profile importer is not written", TICKET, &[]);
        assert!(a.accepted.is_empty());
        assert_eq!(
            a.open,
            vec!["the SSH profile importer is not written".to_string()]
        );
    }

    /// The mixed list is the case the owner card gets wrong today: it must name
    /// the one undeclared item and nothing else.
    #[test]
    fn a_mixed_list_escalates_only_the_undeclared_item() {
        let a = ticket_acceptance(
            "- the Windows leg is untested\n- the SSH profile importer is not written\n- no policy switch for the other projects",
            TICKET,
            &[],
        );
        assert_eq!(
            a.accepted,
            vec![
                "the Windows leg is untested".to_string(),
                "no policy switch for the other projects".to_string()
            ]
        );
        assert_eq!(
            a.open,
            vec!["the SSH profile importer is not written".to_string()]
        );
    }

    #[test]
    fn nothing_left_is_no_items_at_all() {
        for left in ["nothing", "Nothing", "", "   ", "none"] {
            assert!(not_done_items(left).is_empty(), "{left:?}");
        }
    }

    /// Every design doc in the vault writes its declaration as wrapped inline
    /// prose, not a heading with bullets. The 2026-09-10 doc is the model.
    #[test]
    fn a_wrapped_inline_declaration_is_read_as_one() {
        let doc = "\
## Shipped XNAUT-315

Deliberately not done: the continuation prompt's REVERTED paragraph, which is
still instruction-shaped but is a recovery procedure the model genuinely does
not know; and the tail appended to ticket bodies by hand, which is a ticket
concern, not a prompt one.
";
        let a = ticket_acceptance(
            "the REVERTED paragraph of the continuation prompt",
            "",
            &[doc.to_string()],
        );
        assert!(a.open.is_empty(), "{a:?}");
        let b = ticket_acceptance(
            "the tail appended to ticket bodies by hand",
            "",
            &[doc.to_string()],
        );
        assert!(
            b.open.is_empty(),
            "a semicolon separates two declarations: {b:?}"
        );
    }

    /// A mention of scope is not a declaration. Without the required colon, the
    /// gate's own vocabulary ("items out of scope still escalate") would accept
    /// whatever followed it.
    #[test]
    fn a_mention_of_scope_declares_nothing() {
        let body = "Items out of scope still escalate. The card names only those.";
        assert!(declared_out_of_scope(body).is_empty());
        let a = ticket_acceptance("the card is not built", body, &[]);
        assert_eq!(a.open, vec!["the card is not built".to_string()]);
    }

    /// The hand-written mechanism this rule replaces keeps working, through the
    /// same path: the marker is in the list.
    #[test]
    fn the_hand_written_acceptance_line_still_counts() {
        let a = ticket_acceptance(
            "the mobile bridge is not migrated",
            "Accepted not_finished: the mobile bridge is not migrated",
            &[],
        );
        assert!(a.open.is_empty(), "{a:?}");
    }

    /// A section ends at the next heading, or the whole rest of a ticket would
    /// be readable as a licence to leave things undone.
    #[test]
    fn a_section_ends_at_the_next_heading() {
        let declared = declared_out_of_scope(TICKET);
        assert!(declared.iter().any(|d| d.contains("windows leg")));
        assert!(
            !declared
                .iter()
                .any(|d| d.contains("integrate without a card")),
            "ACCEPTANCE is a different section: {declared:?}"
        );
    }

    /// A declaration is only here because it was found in an out-of-scope
    /// context, so its phrasing may be the opposite of the item's and still
    /// account for it. What must never pass is a declaration that only shares
    /// some of the item's vocabulary.
    #[test]
    fn phrasing_may_differ_but_shared_vocabulary_alone_is_not_enough() {
        assert!(accounts_for(
            "a policy switch for the other 43 projects",
            "no policy switch for the other projects"
        ));
        assert!(!accounts_for(
            "the sign-off gate compares the handback with the ticket",
            "the sign-off gate's owner card is not built"
        ));
    }

    #[test]
    fn plurals_and_tenses_compare_equal_but_unrelated_words_do_not() {
        assert!(accounts_for(
            "the workflow tests are not written",
            "the workflow test is not written"
        ));
        assert!(!accounts_for(
            "the sidebar chevron is not removed",
            "the windows leg is untested"
        ));
    }
}
