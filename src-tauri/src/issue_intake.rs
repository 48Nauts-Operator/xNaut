// Issue intake (XNAUT-382): somebody else's issue tracker becomes a ticket on
// this board, and the issue learns where it went.
//
// The gap this closes is small and embarrassing. On the morning of 2026-09-14
// GitHub #75 was copied into XNAUT-363 BY HAND. Read the issue, retype the
// body, invent a type, and then nothing ever told #75's author that anything
// had happened. Every step of that is mechanical and every step of it was
// skipped the next time somebody was busy.
//
// WHAT A TICK DOES, per project that has intake on:
//
//   1. Lists the source's issues newer than the cursor it stored last time.
//   2. Keeps the ones the project's TRIGGER wants: every issue, or only the
//      ones carrying a label (`xnaut` unless the project says otherwise).
//   3. Creates one inbox ticket each: type read from the labels, the reporter's
//      body plus a provenance line, `source_id` naming the issue, NO owner and
//      nothing dispatched. An issue is a request, not an assignment.
//   4. Comments "Tracked as XNAUT-N" on the issue, once.
//   5. Mirrors each tracked ticket's status back as an `xnaut:<status>` label.
//
// NEVER TWICE, and the two guards do different jobs. The CURSOR is a
// high-water mark: it bounds how much is considered and is what makes a tick
// cheap. `source_id` is the correctness guard: an issue that already has a
// ticket never gets a second one, whatever the cursor says. Losing the state
// file therefore costs a re-list, not a duplicate board.
//
// The cursor being a high-water mark has one consequence worth saying out
// loud: an issue LABELLED AFTER it was created, below the cursor, is not
// picked up. That is the behaviour the ticket asked for, and the escape hatch
// is explicit rather than silent. `issue_intake_run_now` takes a `rescan`
// flag that drops the cursor back to zero, and the pane offers it as a button.
//
// ONE TRAIT, three dialects. `IssueSource` is the seam: list, comment, mirror.
// Forgejo, GitHub and GitLab share `ForgeSource` because forges.rs already
// speaks all three; Linear is `LinearSource` over GraphQL. Jira is the next
// one in and needs no change here (XNAUT-382 says so, and leaves it out).
//
// The DECIDING is pure and the TALKING is not, on purpose. `plan` takes the
// issues, the cursor and the source_ids already on the board, and returns what
// to create, with no network, no clock and no app handle. Everything that can be
// wrong about intake is in `plan`, and `plan` is a table test.

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ─── What a project asked for ────────────────────────────────────────────────

/// Which issues become tickets.
///
/// `Labelled` is the default because the alternative, on a public repo, is a
/// board that fills with other people's questions. The label is per project so
/// a repo that already uses `triage` does not have to learn a second word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trigger {
    /// Every issue the source lists.
    All,
    /// Only issues carrying `label`.
    Labelled,
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger::Labelled
    }
}

pub fn default_label() -> String {
    "xnaut".to_string()
}

/// A project's intake settings, stored on its `project.json`.
///
/// `skip_serializing_if` on the whole struct at the use site: every project
/// record is a file in git, and writing `"issue_intake": {...default...}` onto
/// thirty projects that never asked for it is thirty diffs saying nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueIntake {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub trigger: Trigger,
    #[serde(default = "default_label")]
    pub label: String,
    /// Empty means "the project's forge_remote". Set to a `TEAM` key to read
    /// Linear instead.
    #[serde(default)]
    pub linear_team: String,
}

impl Default for IssueIntake {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger: Trigger::default(),
            label: default_label(),
            linear_team: String::new(),
        }
    }
}

impl IssueIntake {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

// ─── What a source hands back ────────────────────────────────────────────────

/// One issue, reduced to what intake needs. Deliberately not `ForgeIssue`:
/// Linear has no issue number worth storing and no `is_pr`, and a struct that
/// carries fields one source cannot fill is a struct that invites reading
/// them anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomingIssue {
    /// The reference this source uses in a `source_id`: "75" on a forge,
    /// "ENG-123" on Linear.
    pub reference: String,
    /// Orders issues against the cursor. A zero-padded number on a forge, an
    /// RFC3339 timestamp on Linear. They are compared as strings, which is why
    /// the forge one is padded.
    pub cursor: String,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub author: String,
    pub url: String,
}

/// Everything intake does to a source. Three calls, and Jira slots in here.
///
/// `BoxFuture` rather than `async fn` in the trait: the intake tick holds
/// `Box<dyn IssueSource>` so a project's source can be a forge or Linear, and
/// `async fn` in a trait is not dyn-safe on this toolchain. `async-trait`
/// would hide the box; it would not remove it.
pub trait IssueSource: Send + Sync {
    /// The `source_id` prefix, e.g. "github:48Nauts/xnaut" or "linear:ENG".
    /// Names the source well enough that two projects on the same forge, or
    /// the same repo read by two machines, never collide.
    fn origin(&self) -> String;

    /// What a ticket body calls this source: "GitHub", "Forgejo", "Linear".
    fn display_name(&self) -> String;

    /// Issues whose `cursor` is greater than `cursor`. A source may over-serve
    /// (most list endpoints cannot filter exactly); `plan` filters again.
    fn list_since<'a>(&'a self, cursor: &'a str)
        -> BoxFuture<'a, Result<Vec<IncomingIssue>, String>>;

    /// Comment on the issue named by `reference`.
    fn comment<'a>(&'a self, reference: &'a str, body: &'a str)
        -> BoxFuture<'a, Result<(), String>>;

    /// Replace the issue's `xnaut:` label with `label`, leaving its other
    /// labels alone.
    fn mirror_status<'a>(&'a self, reference: &'a str, label: &'a str)
        -> BoxFuture<'a, Result<(), String>>;
}

// ─── The pure core ───────────────────────────────────────────────────────────

/// The `xnaut:` label prefix. One constant because three places compare
/// against it and a fourth would have been a second spelling.
pub const STATUS_LABEL_PREFIX: &str = "xnaut:";

/// How many issues one list call returns, across every dialect forges.rs
/// speaks and Linear's GraphQL `first:`. Named so the tick can tell a page
/// that happened to be full from a source that had nothing more.
pub const PAGE_SIZE: usize = 50;

/// A ticket type read from the issue's labels.
///
/// `bug` and `feature` are the two an issue tracker actually carries, and
/// everything else is `task` rather than a guess. `enhancement` is here
/// because it is GitHub's own default label for what xNAUT calls a feature,
/// and a repo using the stock labels would otherwise file every feature
/// request as a task.
pub fn type_from_labels(labels: &[String]) -> &'static str {
    let has = |want: &str| {
        labels
            .iter()
            .any(|label| label.trim().eq_ignore_ascii_case(want))
    };
    if has("bug") || has("defect") || has("regression") {
        return "bug";
    }
    if has("feature") || has("enhancement") || has("feature request") {
        return "feature";
    }
    "task"
}

/// Does this issue match what the project asked for?
pub fn wanted(issue: &IncomingIssue, trigger: &Trigger, label: &str) -> bool {
    match trigger {
        Trigger::All => true,
        Trigger::Labelled => {
            let want = label.trim();
            // An empty label under `Labelled` would match everything, which is
            // the opposite of what the setting says. Fall back to the default
            // word rather than silently becoming `All`.
            let want = if want.is_empty() { "xnaut" } else { want };
            issue
                .labels
                .iter()
                .any(|have| have.trim().eq_ignore_ascii_case(want))
        }
    }
}

/// `github:48Nauts/xnaut#75`, `linear:ENG-123`.
///
/// Linear's identifier is already globally unique and already carries its
/// team, so a `#` there would only be punctuation. The forge form needs the
/// number, because 75 means nothing without the repo.
pub fn source_id(origin: &str, reference: &str) -> String {
    match origin.starts_with("linear:") {
        // A Linear identifier already carries its team, so taking the team
        // from the origin as well would say it twice: `linear:ENG-ENG-123`.
        true => format!("linear:{reference}"),
        false => format!("{origin}#{reference}"),
    }
}

/// The reference inside a `source_id` this source owns, or None.
///
/// "Owns" is the whole point. The mirror looks up a ticket's issue by this,
/// and a source that answered for another source's ids would push one repo's
/// statuses onto another repo's issues.
pub fn reference_of(origin: &str, source_id: &str) -> Option<String> {
    match origin.strip_prefix("linear:") {
        Some(team) => {
            let rest = source_id.strip_prefix("linear:")?;
            // ENG-123 belongs to team ENG and to no other, and `ENG` must not
            // claim `ENGINE-1` either; hence the hyphen.
            rest.strip_prefix(team)?.strip_prefix('-')?;
            Some(rest.to_string())
        }
        None => {
            let rest = source_id.strip_prefix(origin)?.strip_prefix('#')?;
            (!rest.is_empty()).then(|| rest.to_string())
        }
    }
}

/// The ticket body: the reporter's words, then where they came from.
///
/// Provenance goes at the BOTTOM. An agent reading this ticket should hit the
/// problem first; "From GitHub #75 by cand0rian" is how to answer it, not what
/// it is. An empty issue body still gets the line, so a ticket is never
/// anonymous.
pub fn intake_body(issue: &IncomingIssue, display_name: &str) -> String {
    let reported = issue.body.trim();
    let provenance = format!(
        "From {display_name} #{} by {}{}",
        issue.reference,
        match issue.author.trim().is_empty() {
            true => "an unnamed reporter",
            false => issue.author.trim(),
        },
        match issue.url.trim().is_empty() {
            true => String::new(),
            false => format!("\n{}", issue.url.trim()),
        },
    );
    match reported.is_empty() {
        true => provenance,
        false => format!("{reported}\n\n---\n\n{provenance}"),
    }
}

/// The label a ticket's status mirrors as.
pub fn status_label(status: &str) -> String {
    format!("{STATUS_LABEL_PREFIX}{}", status.trim())
}

/// One ticket intake decided to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intake {
    pub reference: String,
    pub source_id: String,
    pub title: String,
    pub ticket_type: String,
    pub body: String,
}

/// What a tick should do, decided without touching the network.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub create: Vec<Intake>,
    /// The cursor to store afterwards. Never moves backwards.
    pub cursor: String,
    /// Issues the trigger wanted but a ticket already covers. Counted so a
    /// quiet tick can say "nothing new" rather than nothing at all.
    pub already_tracked: usize,
}

/// Decide what to create.
///
/// Everything that can be wrong about intake lives here: the trigger, the
/// two guards, the type, the body and where the cursor ends up. No I/O, no
/// clock; the arguments are the whole world.
pub fn plan(
    issues: &[IncomingIssue],
    cursor: &str,
    seen_source_ids: &HashSet<String>,
    origin: &str,
    display_name: &str,
    intake: &IssueIntake,
) -> Plan {
    let mut out = Plan {
        cursor: cursor.to_string(),
        ..Default::default()
    };
    for issue in issues {
        // The cursor moves for every issue the source served, not only the
        // ones taken in. An issue the trigger rejected is still SEEN, and
        // re-considering it every tick forever is the cost this guard exists
        // to avoid.
        if issue.cursor > out.cursor {
            out.cursor = issue.cursor.clone();
        }
        if issue.cursor.as_str() <= cursor {
            continue;
        }
        if !wanted(issue, &intake.trigger, &intake.label) {
            continue;
        }
        let id = source_id(origin, &issue.reference);
        if seen_source_ids.contains(&id) {
            out.already_tracked += 1;
            continue;
        }
        out.create.push(Intake {
            reference: issue.reference.clone(),
            source_id: id,
            title: issue.title.trim().to_string(),
            ticket_type: type_from_labels(&issue.labels).to_string(),
            body: intake_body(issue, display_name),
        });
    }
    out
}

/// Which tracked tickets need their label pushed, given what was pushed last.
///
/// The stored map is what makes the mirror cheap: without it every tick would
/// have to ASK the source what each issue is labelled, which is one HTTP call
/// per tracked ticket per three minutes, forever. With it a tick that changed
/// nothing makes no calls at all.
pub fn mirror_plan(
    tracked: &[(String, String)],
    mirrored: &HashMap<String, String>,
) -> Vec<(String, String)> {
    tracked
        .iter()
        .filter_map(|(source_id, status)| {
            let want = status_label(status);
            (mirrored.get(source_id) != Some(&want)).then(|| (source_id.clone(), want))
        })
        .collect()
}

/// The label set to PUT: the issue's own labels with any `xnaut:` one
/// replaced by `want`.
///
/// A replace rather than an add, because the old status label has to go. Only
/// the `xnaut:` ones are touched, because the repo's own labels are the repo's
/// business and intake has no opinion about them.
pub fn labels_with_status(current: &[String], want: &str) -> Vec<String> {
    let mut out: Vec<String> = current
        .iter()
        .filter(|label| !label.trim().starts_with(STATUS_LABEL_PREFIX))
        .cloned()
        .collect();
    out.push(want.to_string());
    out
}

// ─── The cursor store ────────────────────────────────────────────────────────

/// What intake remembers between ticks, beside the agent registry.
///
/// Not in the control repo on purpose. The cursor is a cache; losing it costs
/// a re-list, and `source_id` still stops the duplicate. The control repo is
/// shared between machines and every write to it is a commit, and three
/// minutes of commits saying "the cursor is still 75" is not a history.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntakeState {
    /// origin → cursor.
    #[serde(default)]
    pub cursors: HashMap<String, String>,
    /// source_id → the `xnaut:` label last pushed.
    #[serde(default)]
    pub mirrored: HashMap<String, String>,
}

pub fn state_path(registry: &Path) -> std::path::PathBuf {
    registry.join("issue-intake.json")
}

pub fn read_state(registry: &Path) -> IntakeState {
    std::fs::read(state_path(registry))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn write_state(registry: &Path, state: &IntakeState) -> Result<(), String> {
    std::fs::create_dir_all(registry).map_err(|error| error.to_string())?;
    crate::project_management::write_json_atomic(&state_path(registry), state)
}

// ─── Forges: Forgejo, GitHub, GitLab ─────────────────────────────────────────

/// A forge repository read through forges.rs.
pub struct ForgeSource {
    pub host: crate::settings::ForgeHost,
    pub owner: String,
    pub repo: String,
}

/// Forge issue numbers are compared as STRINGS against the cursor, so they are
/// padded. Without this #9 sorts after #10 and intake stalls one issue short
/// of the truth, which is the sort of bug that only shows up on the tenth
/// issue and then looks like a network problem.
pub fn pad_number(number: u64) -> String {
    format!("{number:012}")
}

impl ForgeSource {
    fn number(reference: &str) -> Result<u64, String> {
        reference
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("not an issue number: {reference}"))
    }
}

impl IssueSource for ForgeSource {
    fn origin(&self) -> String {
        format!("{}:{}/{}", self.host.kind, self.owner, self.repo)
    }

    fn display_name(&self) -> String {
        match self.host.kind.as_str() {
            "github" => "GitHub".into(),
            "forgejo" => "Forgejo".into(),
            "gitlab" => "GitLab".into(),
            other => other.to_string(),
        }
    }

    fn list_since<'a>(
        &'a self,
        _cursor: &'a str,
    ) -> BoxFuture<'a, Result<Vec<IncomingIssue>, String>> {
        Box::pin(async move {
            // The list endpoint is asked for open issues and nothing else. A
            // `since` parameter exists on GitHub and means "updated since",
            // not "numbered above", so filtering here against the real cursor
            // is both simpler and correct; `plan` does it.
            let issues = crate::forges::list_issues_for(
                &self.host,
                &self.owner,
                &self.repo,
                crate::forges::IssueKind::Issues,
            )
            .await?;
            Ok(issues
                .into_iter()
                .filter(|issue| !issue.is_pr)
                .map(|issue| IncomingIssue {
                    reference: issue.number.to_string(),
                    cursor: pad_number(issue.number),
                    title: issue.title,
                    body: issue.body,
                    labels: issue.labels,
                    author: issue.author,
                    url: issue.html_url,
                })
                .collect())
        })
    }

    fn comment<'a>(
        &'a self,
        reference: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let number = Self::number(reference)?;
            crate::forges::add_issue_comment_for(
                &self.host,
                &self.owner,
                &self.repo,
                number,
                body,
            )
            .await
            .map(|_| ())
        })
    }

    fn mirror_status<'a>(
        &'a self,
        reference: &'a str,
        label: &'a str,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let number = Self::number(reference)?;
            // Read the issue's labels first so the repo's own labels survive
            // the replace. One GET per status CHANGE, not per tick.
            let issue =
                crate::forges::get_issue_for(&self.host, &self.owner, &self.repo, number).await?;
            let labels = labels_with_status(&issue.labels, label);
            crate::forges::set_issue_labels(
                &self.host,
                &self.owner,
                &self.repo,
                number,
                &labels,
            )
            .await
        })
    }
}

// ─── Linear ──────────────────────────────────────────────────────────────────

/// A Linear team read over GraphQL.
///
/// Linear has no REST issue API worth using and no issue numbers that mean
/// anything outside a team, so this is its own source rather than a fourth
/// forge dialect: the cursor is a timestamp, the reference is an identifier,
/// and a label is an id that has to exist before it can be attached.
pub struct LinearSource {
    pub api_key: String,
    pub team: String,
    pub endpoint: String,
}

pub const LINEAR_ENDPOINT: &str = "https://api.linear.app/graphql";

/// The GraphQL documents, named so the tests can assert on the shape that
/// actually goes over the wire rather than on a paraphrase of it.
pub const LINEAR_LIST_QUERY: &str = r#"query($team: String!, $after: DateTimeOrDuration) {
  issues(filter: { team: { key: { eq: $team } }, updatedAt: { gt: $after } }, first: 50) {
    nodes {
      identifier
      title
      description
      updatedAt
      url
      creator { displayName }
      labels { nodes { name } }
    }
  }
}"#;

pub const LINEAR_COMMENT_MUTATION: &str =
    r#"mutation($issue: String!, $body: String!) {
  commentCreate(input: { issueId: $issue, body: $body }) { success }
}"#;

pub const LINEAR_ISSUE_LABELS_QUERY: &str = r#"query($id: String!) {
  issue(id: $id) {
    id
    team { id }
    labels { nodes { id name } }
  }
}"#;

pub const LINEAR_TEAM_LABELS_QUERY: &str = r#"query($team: String!) {
  teams(filter: { key: { eq: $team } }, first: 1) {
    nodes { id labels(first: 250) { nodes { id name } } }
  }
}"#;

pub const LINEAR_LABEL_CREATE_MUTATION: &str =
    r#"mutation($name: String!, $team: String!, $color: String!) {
  issueLabelCreate(input: { name: $name, teamId: $team, color: $color }) {
    issueLabel { id name }
  }
}"#;

pub const LINEAR_ISSUE_UPDATE_MUTATION: &str =
    r#"mutation($id: String!, $labels: [String!]!) {
  issueUpdate(id: $id, input: { labelIds: $labels }) { success }
}"#;

/// Map one `issues.nodes[]` entry.
///
/// Its own function because it is the half of Linear support that can be
/// tested against a recorded payload without a socket.
pub fn map_linear_issue(node: &Value) -> IncomingIssue {
    let text = |key: &str| {
        node.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    IncomingIssue {
        reference: text("identifier"),
        cursor: text("updatedAt"),
        title: text("title"),
        body: text("description"),
        labels: node
            .pointer("/labels/nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|label| label.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        author: node
            .pointer("/creator/displayName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        url: text("url"),
    }
}

/// GraphQL answers 200 with an `errors` array, so a plain status check reads a
/// refusal as a success and then finds no data. Both halves are checked here,
/// once, for every Linear call.
pub fn linear_payload(body: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("Linear returned invalid JSON: {error}"))?;
    if let Some(errors) = value.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            let said = errors
                .iter()
                .filter_map(|error| error.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "Linear refused the request: {}",
                match said.is_empty() {
                    true => errors[0].to_string(),
                    false => said,
                }
            ));
        }
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| "Linear returned no data".to_string())
}

impl LinearSource {
    async fn call(&self, query: &str, variables: Value) -> Result<Value, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("xnaut")
            .build()
            .map_err(|error| format!("failed to build http client: {error}"))?;
        let response = client
            .post(&self.endpoint)
            // Linear's personal API keys go in Authorization RAW, with no
            // "Bearer". A Bearer-prefixed key answers 400 with an
            // authentication error, which reads like a bad key.
            .header("Authorization", self.api_key.trim())
            .header("Content-Type", "application/json")
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(|error| format!("Linear request failed: {error}"))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| format!("Linear: failed to read body: {error}"))?;
        if !status.is_success() {
            let snippet: String = text.chars().take(300).collect();
            return Err(format!("Linear answered {status}: {snippet}"));
        }
        linear_payload(&text)
    }

    /// The Linear label id for `name`, creating it on the team when missing.
    async fn ensure_label_id(&self, team_id: &str, name: &str) -> Result<String, String> {
        let data = self
            .call(LINEAR_TEAM_LABELS_QUERY, json!({ "team": self.team }))
            .await?;
        let labels = data
            .pointer("/teams/nodes/0/labels/nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(id) = labels.iter().find_map(|label| {
            (label.get("name").and_then(Value::as_str) == Some(name))
                .then(|| label.get("id").and_then(Value::as_str))
                .flatten()
        }) {
            return Ok(id.to_string());
        }
        let created = self
            .call(
                LINEAR_LABEL_CREATE_MUTATION,
                json!({ "name": name, "team": team_id, "color": "#f5b840" }),
            )
            .await?;
        created
            .pointer("/issueLabelCreate/issueLabel/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("Linear created no label for {name}"))
    }
}

impl IssueSource for LinearSource {
    fn origin(&self) -> String {
        format!("linear:{}", self.team)
    }

    fn display_name(&self) -> String {
        "Linear".into()
    }

    fn list_since<'a>(
        &'a self,
        cursor: &'a str,
    ) -> BoxFuture<'a, Result<Vec<IncomingIssue>, String>> {
        Box::pin(async move {
            // Linear's `updatedAt: { gt: null }` is not "everything"; it is a
            // type error. A never-run cursor becomes the epoch instead.
            let after = match cursor.trim().is_empty() {
                true => "1970-01-01T00:00:00.000Z",
                false => cursor.trim(),
            };
            let data = self
                .call(
                    LINEAR_LIST_QUERY,
                    json!({ "team": self.team, "after": after }),
                )
                .await?;
            Ok(data
                .pointer("/issues/nodes")
                .and_then(Value::as_array)
                .map(|nodes| nodes.iter().map(map_linear_issue).collect())
                .unwrap_or_default())
        })
    }

    fn comment<'a>(
        &'a self,
        reference: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.call(
                LINEAR_COMMENT_MUTATION,
                json!({ "issue": reference, "body": body }),
            )
            .await
            .map(|_| ())
        })
    }

    fn mirror_status<'a>(
        &'a self,
        reference: &'a str,
        label: &'a str,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let issue = self
                .call(LINEAR_ISSUE_LABELS_QUERY, json!({ "id": reference }))
                .await?;
            let id = issue
                .pointer("/issue/id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Linear has no issue {reference}"))?
                .to_string();
            let team_id = issue
                .pointer("/issue/team/id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let current: Vec<(String, String)> = issue
                .pointer("/issue/labels/nodes")
                .and_then(Value::as_array)
                .map(|nodes| {
                    nodes
                        .iter()
                        .filter_map(|node| {
                            Some((
                                node.get("id").and_then(Value::as_str)?.to_string(),
                                node.get("name").and_then(Value::as_str)?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut keep: Vec<String> = current
                .iter()
                .filter(|(_, name)| !name.starts_with(STATUS_LABEL_PREFIX))
                .map(|(id, _)| id.clone())
                .collect();
            keep.push(self.ensure_label_id(&team_id, label).await?);
            self.call(
                LINEAR_ISSUE_UPDATE_MUTATION,
                json!({ "id": id, "labels": keep }),
            )
            .await
            .map(|_| ())
        })
    }
}

// ─── Building a source for a project ─────────────────────────────────────────

/// Why a project with intake switched on is nevertheless doing nothing.
///
/// Returned as words rather than swallowed: an intake that is configured, on,
/// and silent is exactly the failure this module was written to stop being
/// done by hand, and "no token configured for github.com" is a sentence the
/// owner can act on.
pub fn source_for(
    project: &crate::project_management::ProjectRecord,
    forges: &[crate::settings::ForgeHost],
    linear: &crate::settings::LinearSettings,
) -> Result<Box<dyn IssueSource>, String> {
    let intake = &project.issue_intake;
    if !intake.linear_team.trim().is_empty() {
        if linear.api_key.trim().is_empty() {
            return Err("Linear intake needs an API key in Settings → Issue Intake".into());
        }
        return Ok(Box::new(LinearSource {
            api_key: linear.api_key.clone(),
            team: intake.linear_team.trim().to_string(),
            endpoint: match linear.endpoint.trim().is_empty() {
                true => LINEAR_ENDPOINT.to_string(),
                false => linear.endpoint.trim().to_string(),
            },
        }));
    }
    let remote = project.forge_remote.trim();
    if remote.is_empty() {
        return Err("the project has no forge remote and no Linear team".into());
    }
    let (host, parsed) = crate::forges::host_for_remote(forges, remote)
        .ok_or_else(|| format!("no configured forge host serves {remote}"))?;
    Ok(Box::new(ForgeSource {
        host: host.clone(),
        owner: parsed.owner,
        repo: parsed.repo,
    }))
}

// ─── The tick ────────────────────────────────────────────────────────────────

/// What one project's intake did, for the ledger and the pane.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectIntakeReport {
    pub project: String,
    pub origin: String,
    pub created: Vec<String>,
    pub mirrored: usize,
    pub already_tracked: usize,
    /// Empty when the project's intake ran. Otherwise why it did not.
    pub blocked: String,
}

/// The comment left on an issue, once.
pub fn tracked_comment(ticket: &str) -> String {
    format!("Tracked as {ticket} in xNAUT.")
}

/// Run intake for every project that asked for it.
///
/// Rides the sweep rather than owning a timer, for the reason `core_team_beat`
/// gives: the sweep is the app's clock and a second one is a second thing that
/// can be wrong. Never fails the tick. A source that is down is a quiet three
/// minutes, and the reason lands in the ledger.
pub async fn beat(
    repo: &Path,
    registry: &Path,
    forges: &[crate::settings::ForgeHost],
    linear: &crate::settings::LinearSettings,
) -> Vec<ProjectIntakeReport> {
    run(repo, registry, forges, linear, None, false).await
}

/// The body of a tick, and of the pane's "Run now".
///
/// `only` narrows it to one project; `rescan` drops that project's cursor so a
/// label added after the fact can still be picked up. Both exist because the
/// cursor is a high-water mark and the escape hatch from a high-water mark has
/// to be something a person can press.
pub async fn run(
    repo: &Path,
    registry: &Path,
    forges: &[crate::settings::ForgeHost],
    linear: &crate::settings::LinearSettings,
    only: Option<&str>,
    rescan: bool,
) -> Vec<ProjectIntakeReport> {
    let projects = match crate::project_management::list_projects(repo) {
        Ok(projects) => projects,
        Err(error) => {
            return vec![ProjectIntakeReport {
                blocked: format!("the board could not be read: {error}"),
                ..Default::default()
            }]
        }
    };
    let wanted: Vec<_> = projects
        .into_iter()
        .filter(|project| project.issue_intake.enabled)
        .filter(|project| only.is_none_or(|key| key == project.key))
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut state = read_state(registry);
    let mut reports = Vec::new();
    for project in &wanted {
        let mut report = ProjectIntakeReport {
            project: project.key.clone(),
            ..Default::default()
        };
        let source = match source_for(project, forges, linear) {
            Ok(source) => source,
            Err(error) => {
                report.blocked = error;
                reports.push(report);
                continue;
            }
        };
        report.origin = source.origin();
        if rescan {
            state.cursors.remove(&report.origin);
        }
        match intake_one(repo, project, source.as_ref(), &mut state, &mut report).await {
            Ok(()) => {}
            Err(error) => report.blocked = error,
        }
        reports.push(report);
    }
    if let Err(error) = write_state(registry, &state) {
        // The work already happened and the tickets already exist. A state
        // file that could not be written costs a re-list next tick, which the
        // source_id guard makes harmless. So this is a note, not a failure.
        crate::ledger::record(
            "issue_intake_state_unwritten",
            "nautbot",
            "",
            &format!("intake ran but its cursor was not stored: {error}"),
        );
    }
    reports
}

async fn intake_one(
    repo: &Path,
    project: &crate::project_management::ProjectRecord,
    source: &dyn IssueSource,
    state: &mut IntakeState,
    report: &mut ProjectIntakeReport,
) -> Result<(), String> {
    let origin = source.origin();
    let tickets = crate::project_management::ticket_list_in(repo, Some(project.key.clone()))?;
    let seen: HashSet<String> = tickets
        .iter()
        .map(|ticket| ticket.source_id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    let cursor = state.cursors.get(&origin).cloned().unwrap_or_default();
    let issues = source.list_since(&cursor).await?;
    // A full page means the source had more to give and this tick did not see
    // it, and because the cursor then jumps to the newest of the fifty, what
    // was left behind is left behind for good. Saying so is the difference
    // between a bound and a silent truncation; the escape hatch is Rescan,
    // which is why the pane has the button.
    if issues.len() >= PAGE_SIZE {
        crate::ledger::record(
            "issue_intake_page_full",
            "nautbot",
            "",
            &format!(
                "{origin} served a full page of {PAGE_SIZE} issues; anything older was not read this tick"
            ),
        );
    }
    let decided = plan(
        &issues,
        &cursor,
        &seen,
        &origin,
        &source.display_name(),
        &project.issue_intake,
    );
    report.already_tracked = decided.already_tracked;

    for item in &decided.create {
        let created = crate::project_management::ticket_create_in(
            repo,
            crate::project_management::TicketCreateRequest {
                model_requirement: String::new(),
                project: project.key.clone(),
                title: item.title.clone(),
                ticket_type: item.ticket_type.clone(),
                // Inbox, unowned, undispatched. An issue is a request; deciding
                // it is worth an agent is the owner's call and the triage
                // loop's, not intake's.
                status: "inbox".into(),
                priority: "medium".into(),
                owner: None,
                documentation: Vec::new(),
                body: item.body.clone(),
                parent: None,
                release: String::new(),
                tags: Vec::new(),
                source_id: item.source_id.clone(),
            },
        )?;
        report.created.push(created.id.clone());
        // The comment is best-effort and the ticket is not. A forge that
        // refuses a comment must not cause the same issue to be filed again
        // next tick, so the ticket's existence is what "done" means here and
        // the failure is only recorded.
        if let Err(error) = source
            .comment(&item.reference, &tracked_comment(&created.id))
            .await
        {
            crate::ledger::record(
                "issue_intake_comment_failed",
                "nautbot",
                &created.id,
                // `item.source_id`, not `{origin}#{reference}`: Linear's ids
                // carry no `#`, and a ledger line naming an issue that does
                // not exist is worse than no line.
                &format!("{}: {error}", item.source_id),
            );
        }
        crate::ledger::record(
            "issue_intake_filed",
            "nautbot",
            &created.id,
            &format!("{} → {}", item.source_id, created.id),
        );
    }

    // Mirror AFTER creating, and off the board rather than off `decided`, so a
    // ticket filed by an earlier tick and moved since still gets its label.
    let tickets = match decided.create.is_empty() {
        true => tickets,
        false => crate::project_management::ticket_list_in(repo, Some(project.key.clone()))?,
    };
    let tracked: Vec<(String, String)> = tickets
        .iter()
        .filter(|ticket| reference_of(&origin, &ticket.source_id).is_some())
        .map(|ticket| (ticket.source_id.clone(), ticket.status.clone()))
        .collect();
    for (source_id, label) in mirror_plan(&tracked, &state.mirrored) {
        let Some(reference) = reference_of(&origin, &source_id) else {
            continue;
        };
        match source.mirror_status(&reference, &label).await {
            Ok(()) => {
                state.mirrored.insert(source_id, label);
                report.mirrored += 1;
            }
            Err(error) => crate::ledger::record(
                "issue_intake_mirror_failed",
                "nautbot",
                "",
                &format!("{source_id}: {error}"),
            ),
        }
    }
    state.cursors.insert(origin, decided.cursor);
    Ok(())
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

/// What the Issue Intake pane shows: every project that could take issues in,
/// whether it does, and where its cursor is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeProjectStatus {
    pub key: String,
    pub name: String,
    pub forge_remote: String,
    pub enabled: bool,
    pub trigger: String,
    pub label: String,
    pub linear_team: String,
    pub origin: String,
    pub cursor: String,
    pub tracked: usize,
    pub blocked: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeStatus {
    pub projects: Vec<IntakeProjectStatus>,
    /// Why NOTHING can run, which is usually no control repo. A project that alone
    /// cannot run says so on its own row instead.
    pub blocked: String,
}

#[tauri::command]
pub async fn issue_intake_status(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<IntakeStatus, String> {
    let (pm, forges, linear) = {
        let settings = state.settings.lock().await;
        (
            settings.project_management.clone(),
            settings.forges.clone(),
            settings.linear.clone(),
        )
    };
    let mut status = IntakeStatus {
        projects: Vec::new(),
        blocked: String::new(),
    };
    let repo = match crate::project_management::configured_repo(&pm) {
        Ok(repo) => repo,
        Err(error) => {
            status.blocked = error;
            return Ok(status);
        }
    };
    // An unreadable registry means "no cursors yet", not "read a file called
    // issue-intake.json out of whatever the working directory happens to be",
    // which is what an empty PathBuf would have done.
    let stored = crate::agents::registry_dir()
        .map(|registry| read_state(&registry))
        .unwrap_or_default();
    let projects = crate::project_management::list_projects(&repo)?;
    for project in projects {
        // A project with nowhere to read issues FROM is not a row on this
        // pane: the list is what can be switched on, not every project.
        if project.forge_remote.trim().is_empty() && project.issue_intake.linear_team.is_empty() {
            continue;
        }
        let intake = &project.issue_intake;
        let (origin, blocked) = match source_for(&project, &forges, &linear) {
            Ok(source) => (source.origin(), String::new()),
            Err(error) => (String::new(), error),
        };
        let tracked = crate::project_management::ticket_list_in(&repo, Some(project.key.clone()))
            .unwrap_or_default()
            .iter()
            .filter(|ticket| !ticket.source_id.is_empty())
            .filter(|ticket| origin.is_empty() || ticket.source_id.starts_with(&origin))
            .count();
        status.projects.push(IntakeProjectStatus {
            key: project.key.clone(),
            name: project.name.clone(),
            forge_remote: project.forge_remote.clone(),
            enabled: intake.enabled,
            trigger: match intake.trigger {
                Trigger::All => "all".into(),
                Trigger::Labelled => "labelled".into(),
            },
            label: intake.label.clone(),
            linear_team: intake.linear_team.clone(),
            cursor: stored.cursors.get(&origin).cloned().unwrap_or_default(),
            origin,
            tracked,
            blocked,
        });
    }
    Ok(status)
}

/// Save one project's intake settings.
///
/// Its own command rather than a field on the project editor: intake is the
/// only thing on `project.json` a person changes without changing the project,
/// and routing it through `project_update` would make every toggle a full
/// record write with an expected_revision the pane does not have.
#[tauri::command]
pub async fn issue_intake_configure(
    state: tauri::State<'_, crate::state::AppState>,
    project: String,
    intake: IssueIntake,
) -> Result<IssueIntake, String> {
    let pm = state.settings.lock().await.project_management.clone();
    let repo = crate::project_management::configured_repo(&pm)?;
    crate::project_management::owner_action(|| crate::project_management::set_issue_intake_in(&repo, &project, intake))
}

/// Run intake now, for one project or all of them.
#[tauri::command]
pub async fn issue_intake_run_now(
    state: tauri::State<'_, crate::state::AppState>,
    project: Option<String>,
    rescan: Option<bool>,
) -> Result<Vec<ProjectIntakeReport>, String> {
    let (pm, forges, linear) = {
        let settings = state.settings.lock().await;
        (
            settings.project_management.clone(),
            settings.forges.clone(),
            settings.linear.clone(),
        )
    };
    let repo = crate::project_management::configured_repo(&pm)?;
    let registry = crate::agents::registry_dir()?;
    Ok(run(
        &repo,
        &registry,
        &forges,
        &linear,
        project.as_deref(),
        rescan.unwrap_or(false),
    )
    .await)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(reference: &str, labels: &[&str]) -> IncomingIssue {
        IncomingIssue {
            reference: reference.into(),
            cursor: pad_number(reference.parse().unwrap_or(0)),
            title: format!("Issue {reference}"),
            body: "It is broken.".into(),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            author: "cand0rian".into(),
            url: format!("https://github.com/48Nauts/xnaut/issues/{reference}"),
        }
    }

    fn labelled() -> IssueIntake {
        IssueIntake {
            enabled: true,
            ..Default::default()
        }
    }

    // ─── A forge, recorded ───────────────────────────────────────────────────
    //
    // The mapping tests above prove intake reads a payload correctly. They
    // cannot prove it ASKS for the right thing, and the two dialects disagree
    // about almost every request: GitHub mixes PRs into /issues and takes
    // label NAMES, Forgejo serves /api/v1 and takes label IDS it will not
    // create for you. Both of those are silent when wrong; a dropped label is
    // a 200. So the requests themselves are what these tests assert on.
    //
    // A recorded server rather than a mocking crate: this codebase already
    // stands up a tiny HTTP listener in three places (`nautloom::static_serve`,
    // `designer_local`), and a new dependency to avoid forty lines is a poor
    // trade when the forty lines are also what let us assert on the request.

    /// One request the code under test actually made.
    #[derive(Debug, Clone)]
    struct Recorded {
        method: String,
        path: String,
        body: String,
        authorization: String,
    }

    /// Serves canned responses by "METHOD /path" and records every request.
    struct RecordedForge {
        pub base_url: String,
        pub seen: std::sync::Arc<std::sync::Mutex<Vec<Recorded>>>,
        shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl RecordedForge {
        async fn start(routes: Vec<(&'static str, Value)>) -> Self {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let table: HashMap<String, Value> = routes
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect();
            let (tx, mut rx) = tokio::sync::oneshot::channel();
            let recorded = seen.clone();
            tokio::spawn(async move {
                loop {
                    let accepted = tokio::select! {
                        _ = &mut rx => break,
                        accepted = listener.accept() => accepted,
                    };
                    let Ok((mut socket, _)) = accepted else { break };
                    let mut buffer = Vec::new();
                    let mut chunk = [0u8; 4096];
                    // Read until the body is complete. A single read is not
                    // enough: a PUT's headers and body can arrive separately,
                    // and half a body parses as no body at all.
                    let (head, body) = loop {
                        let read = socket.read(&mut chunk).await.unwrap_or(0);
                        if read == 0 {
                            break (String::from_utf8_lossy(&buffer).to_string(), String::new());
                        }
                        buffer.extend_from_slice(&chunk[..read]);
                        let text = String::from_utf8_lossy(&buffer).to_string();
                        let Some((head, body)) = text.split_once("\r\n\r\n") else {
                            continue;
                        };
                        let want: usize = head
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse().ok())
                            })
                            .unwrap_or(0);
                        if body.len() >= want {
                            break (head.to_string(), body.to_string());
                        }
                    };
                    let mut lines = head.lines();
                    let request = lines.next().unwrap_or_default().to_string();
                    let mut parts = request.split_whitespace();
                    let method = parts.next().unwrap_or_default().to_string();
                    let path = parts.next().unwrap_or_default().to_string();
                    let authorization = head
                        .lines()
                        .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
                        .map(|line| line[14..].trim().to_string())
                        .unwrap_or_default();
                    // Recorded WITHOUT the query string, and matched the same
                    // way, so a test does not have to restate `?limit=200`.
                    let bare = path.split('?').next().unwrap_or(&path).to_string();
                    recorded.lock().unwrap().push(Recorded {
                        method: method.clone(),
                        path: bare.clone(),
                        body,
                        authorization,
                    });
                    let answer = table.get(&format!("{method} {bare}"));
                    let payload = match &answer {
                        Some(value) => serde_json::to_string(value).unwrap(),
                        None => json!({ "message": "no recording for this route" }).to_string(),
                    };
                    let status = match answer {
                        Some(_) => "200 OK",
                        None => "404 Not Found",
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                }
            });
            RecordedForge {
                base_url: format!("http://127.0.0.1:{port}"),
                seen,
                shutdown: Some(tx),
            }
        }

        fn requests(&self) -> Vec<Recorded> {
            self.seen.lock().unwrap().clone()
        }

        fn one(&self, method: &str, path: &str) -> Recorded {
            self.requests()
                .into_iter()
                .find(|r| r.method == method && r.path == path)
                .unwrap_or_else(|| {
                    panic!(
                        "no {method} {path} was ever sent; the code asked for {:?}",
                        self.requests()
                            .iter()
                            .map(|r| format!("{} {}", r.method, r.path))
                            .collect::<Vec<_>>()
                    )
                })
        }
    }

    impl Drop for RecordedForge {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown.take() {
                let _ = tx.send(());
            }
        }
    }

    fn host(kind: &str, base_url: &str) -> crate::settings::ForgeHost {
        crate::settings::ForgeHost {
            kind: kind.into(),
            base_url: base_url.into(),
            owner: "someone-else".into(),
            token: Some("t0ken".into()),
        }
    }

    /// A real `GET /repos/o/r/issues` body from GitHub, trimmed to the fields
    /// intake reads. The `pull_request` entry is the important one: GitHub
    /// serves PRs from the issues endpoint and a PR must never become a ticket.
    fn github_issue_list() -> Value {
        json!([
            {
                "number": 75,
                "title": "Terminal shows a black screen after an update",
                "body": "Since 1.26 the first tab never draws.",
                "state": "open",
                "labels": [{ "name": "xnaut" }, { "name": "bug" }],
                "user": { "login": "cand0rian" },
                "updated_at": "2026-09-14T08:00:00Z",
                "html_url": "https://github.com/48Nauts/xnaut/issues/75",
            },
            {
                "number": 76,
                "title": "Add a Windows build",
                "body": "",
                "state": "open",
                "labels": [{ "name": "enhancement" }],
                "user": { "login": "someone" },
                "updated_at": "2026-09-14T08:05:00Z",
                "html_url": "https://github.com/48Nauts/xnaut/issues/76",
            },
            {
                "number": 77,
                "title": "A pull request, not an issue",
                "body": "",
                "state": "open",
                "labels": [{ "name": "xnaut" }],
                "user": { "login": "someone" },
                "updated_at": "2026-09-14T08:10:00Z",
                "html_url": "https://github.com/48Nauts/xnaut/pull/77",
                "pull_request": { "merged_at": null },
            },
        ])
    }

    /// The same shape off Forgejo, which serves it from /api/v1 and labels it
    /// with objects carrying ids.
    fn forgejo_issue_list() -> Value {
        json!([
            {
                "number": 12,
                "title": "SSH profiles do not load",
                "body": "The modal is empty.",
                "state": "open",
                "labels": [{ "id": 3, "name": "xnaut" }, { "id": 4, "name": "bug" }],
                "user": { "login": "andre" },
                "updated_at": "2026-09-14T07:00:00Z",
                "html_url": "http://cosmos:3000/48Nauts/xnaut/issues/12",
            },
        ])
    }

    #[tokio::test]
    async fn a_labelled_github_issue_becomes_a_ticket_and_gets_the_comment() {
        let forge = RecordedForge::start(vec![
            ("GET /repos/48Nauts/xnaut/issues", github_issue_list()),
            (
                "POST /repos/48Nauts/xnaut/issues/75/comments",
                json!({ "html_url": "https://github.com/48Nauts/xnaut/issues/75#issuecomment-1" }),
            ),
        ])
        .await;
        let source = ForgeSource {
            host: host("github", &forge.base_url),
            owner: "48Nauts".into(),
            repo: "xnaut".into(),
        };
        assert_eq!(source.origin(), "github:48Nauts/xnaut");
        assert_eq!(source.display_name(), "GitHub");

        let issues = source.list_since("").await.unwrap();
        assert_eq!(
            issues.iter().map(|i| i.reference.as_str()).collect::<Vec<_>>(),
            vec!["75", "76"],
            "a pull request must never reach intake"
        );

        let decided = plan(
            &issues,
            "",
            &HashSet::new(),
            &source.origin(),
            &source.display_name(),
            &labelled(),
        );
        assert_eq!(decided.create.len(), 1, "only the xnaut-labelled one");
        let made = &decided.create[0];
        assert_eq!(made.ticket_type, "bug");
        assert_eq!(made.source_id, "github:48Nauts/xnaut#75");
        assert!(made.body.contains("Since 1.26 the first tab never draws."));
        assert!(made.body.contains("From GitHub #75 by cand0rian"));
        // The cursor passes the PR too: it was seen, and re-listing it every
        // tick forever is the cost the cursor exists to avoid.
        assert_eq!(decided.cursor, pad_number(76));

        source
            .comment(&made.reference, &tracked_comment("XNAUT-400"))
            .await
            .unwrap();
        let posted = forge.one("POST", "/repos/48Nauts/xnaut/issues/75/comments");
        let body: Value = serde_json::from_str(&posted.body).unwrap();
        assert_eq!(body["body"], "Tracked as XNAUT-400 in xNAUT.");
        assert_eq!(
            posted.authorization, "Bearer t0ken",
            "GitHub takes a Bearer token"
        );
        // The owner comes from the project's remote, not from the host's
        // default org. Reading `host.owner` here would file another org's
        // issues onto this board, which is what `list_issues_for` exists for.
        assert!(forge
            .requests()
            .iter()
            .all(|r| !r.path.contains("someone-else")));
    }

    #[tokio::test]
    async fn a_forgejo_issue_is_read_from_api_v1_with_a_token_header() {
        let forge = RecordedForge::start(vec![
            ("GET /api/v1/repos/48Nauts/xnaut/issues", forgejo_issue_list()),
            (
                "POST /api/v1/repos/48Nauts/xnaut/issues/12/comments",
                json!({ "html_url": "http://cosmos:3000/48Nauts/xnaut/issues/12#1" }),
            ),
        ])
        .await;
        let source = ForgeSource {
            host: host("forgejo", &forge.base_url),
            owner: "48Nauts".into(),
            repo: "xnaut".into(),
        };
        assert_eq!(source.origin(), "forgejo:48Nauts/xnaut");

        let issues = source.list_since("").await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].reference, "12");
        assert_eq!(issues[0].author, "andre");
        assert_eq!(type_from_labels(&issues[0].labels), "bug");

        source.comment("12", "Tracked as XNAUT-401 in xNAUT.").await.unwrap();
        let posted = forge.one("POST", "/api/v1/repos/48Nauts/xnaut/issues/12/comments");
        assert_eq!(
            posted.authorization, "token t0ken",
            "Forgejo takes `token`, not `Bearer`"
        );
        let decided = plan(
            &issues,
            "",
            &HashSet::new(),
            &source.origin(),
            &source.display_name(),
            &labelled(),
        );
        assert_eq!(decided.create[0].source_id, "forgejo:48Nauts/xnaut#12");
        assert!(decided.create[0].body.contains("From Forgejo #12 by andre"));
    }

    #[tokio::test]
    async fn mirroring_a_status_on_github_replaces_only_the_xnaut_label() {
        let forge = RecordedForge::start(vec![
            (
                "GET /repos/48Nauts/xnaut/issues/75",
                json!({
                    "number": 75, "title": "t", "body": "", "state": "open",
                    "labels": [{ "name": "bug" }, { "name": "xnaut" }, { "name": "xnaut:inbox" }],
                    "user": { "login": "cand0rian" },
                    "updated_at": "2026-09-14T08:00:00Z",
                    "html_url": "https://github.com/48Nauts/xnaut/issues/75",
                }),
            ),
            ("PUT /repos/48Nauts/xnaut/issues/75/labels", json!([])),
        ])
        .await;
        let source = ForgeSource {
            host: host("github", &forge.base_url),
            owner: "48Nauts".into(),
            repo: "xnaut".into(),
        };
        source.mirror_status("75", "xnaut:in_progress").await.unwrap();
        let put = forge.one("PUT", "/repos/48Nauts/xnaut/issues/75/labels");
        let body: Value = serde_json::from_str(&put.body).unwrap();
        let sent: Vec<&str> = body["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(sent, vec!["bug", "xnaut", "xnaut:in_progress"]);
        assert!(
            !sent.contains(&"xnaut:inbox"),
            "the previous status label must go or the issue carries two"
        );
    }

    #[tokio::test]
    async fn mirroring_on_forgejo_creates_the_label_it_needs_and_sends_ids() {
        // Gitea's issue-labels endpoint takes IDS and creates nothing. A name
        // it does not know is dropped with a 200, so without the create the
        // mirror would "succeed" and set no label at all.
        let forge = RecordedForge::start(vec![
            (
                "GET /api/v1/repos/48Nauts/xnaut/issues/12",
                json!({
                    "number": 12, "title": "t", "body": "", "state": "open",
                    "labels": [{ "id": 4, "name": "bug" }],
                    "user": { "login": "andre" },
                    "updated_at": "2026-09-14T07:00:00Z",
                    "html_url": "http://cosmos:3000/48Nauts/xnaut/issues/12",
                }),
            ),
            (
                "GET /api/v1/repos/48Nauts/xnaut/labels",
                json!([{ "id": 4, "name": "bug" }]),
            ),
            (
                "POST /api/v1/repos/48Nauts/xnaut/labels",
                json!({ "id": 9, "name": "xnaut:done" }),
            ),
            ("PUT /api/v1/repos/48Nauts/xnaut/issues/12/labels", json!([])),
        ])
        .await;
        let source = ForgeSource {
            host: host("forgejo", &forge.base_url),
            owner: "48Nauts".into(),
            repo: "xnaut".into(),
        };
        source.mirror_status("12", "xnaut:done").await.unwrap();

        let created = forge.one("POST", "/api/v1/repos/48Nauts/xnaut/labels");
        let created: Value = serde_json::from_str(&created.body).unwrap();
        assert_eq!(created["name"], "xnaut:done");

        let put = forge.one("PUT", "/api/v1/repos/48Nauts/xnaut/issues/12/labels");
        let body: Value = serde_json::from_str(&put.body).unwrap();
        assert_eq!(
            body["labels"],
            json!([4, 9]),
            "Forgejo takes label ids; names are silently dropped"
        );
    }

    #[tokio::test]
    async fn a_forge_that_refuses_is_an_error_and_not_an_empty_list() {
        // An unreachable or refusing forge answering "no issues" would look
        // exactly like a quiet day, and intake would say nothing forever.
        let forge = RecordedForge::start(Vec::new()).await;
        let source = ForgeSource {
            host: host("github", &forge.base_url),
            owner: "48Nauts".into(),
            repo: "xnaut".into(),
        };
        let error = source.list_since("").await.unwrap_err();
        assert!(error.contains("404"), "{error}");
    }

    #[tokio::test]
    async fn linear_is_asked_over_graphql_with_a_raw_authorization_header() {
        let forge = RecordedForge::start(vec![(
            "POST /graphql",
            json!({ "data": { "issues": { "nodes": [{
                "identifier": "ENG-7",
                "title": "The board does not sort",
                "description": "Clicking a header does nothing.",
                "updatedAt": "2026-09-14T11:00:00.000Z",
                "url": "https://linear.app/48nauts/issue/ENG-7",
                "creator": { "displayName": "Andre" },
                "labels": { "nodes": [{ "name": "xnaut" }, { "name": "Bug" }] },
            }] } } }),
        )])
        .await;
        let source = LinearSource {
            api_key: "lin_api_secret".into(),
            team: "ENG".into(),
            endpoint: format!("{}/graphql", forge.base_url),
        };
        let issues = source.list_since("").await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].reference, "ENG-7");
        assert_eq!(issues[0].cursor, "2026-09-14T11:00:00.000Z");

        let asked = forge.one("POST", "/graphql");
        assert_eq!(
            asked.authorization, "lin_api_secret",
            "Linear takes the key raw; a Bearer prefix is a 400"
        );
        let sent: Value = serde_json::from_str(&asked.body).unwrap();
        assert_eq!(sent["variables"]["team"], "ENG");
        assert_eq!(
            sent["variables"]["after"], "1970-01-01T00:00:00.000Z",
            "a never-run cursor is the epoch; `gt: null` is a Linear type error"
        );
        assert!(sent["query"].as_str().unwrap().contains("updatedAt"));

        let decided = plan(
            &issues,
            "",
            &HashSet::new(),
            &source.origin(),
            &source.display_name(),
            &labelled(),
        );
        assert_eq!(decided.create[0].source_id, "linear:ENG-7");
        assert_eq!(decided.create[0].ticket_type, "bug");
        assert!(decided.create[0].body.contains("From Linear #ENG-7 by Andre"));
    }

    // ─── End to end, against a real control repo ─────────────────────────────

    /// A control repo with one project, wired to a recorded forge.
    ///
    /// A real git repo because `ticket_create_in` commits: the whole point of
    /// the board is that a write is a commit plus an event, and a fixture that
    /// skipped that would test a function no caller uses.
    fn control_repo(remote: &str, intake: IssueIntake) -> std::path::PathBuf {
        let repo = std::env::temp_dir().join(format!("xnaut-intake-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(repo.join("projects/DEMO/tickets")).unwrap();
        std::fs::create_dir_all(repo.join("events")).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "intake test"],
            vec!["config", "user.email", "intake@xnaut.local"],
        ] {
            std::process::Command::new("git")
                .args(&args)
                .current_dir(&repo)
                .output()
                .unwrap();
        }
        crate::project_management::write_json_atomic(
            &repo.join("projects/DEMO/project.json"),
            &json!({
                "key": "DEMO",
                "name": "Demo",
                "forge_remote": remote,
                "issue_intake": intake,
                "created_at": "2026-09-14T00:00:00Z",
            }),
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "seed"])
            .current_dir(&repo)
            .output()
            .unwrap();
        repo
    }

    #[tokio::test]
    async fn a_tick_files_the_ticket_comments_on_the_issue_and_a_second_tick_does_nothing() {
        // A tick writes to the ledger, and XNAUT_LEDGER_PATH is process-global.
        // Without this guard these writes land in whatever ledger is current:
        // another test's scratch file, or on a developer's machine the REAL
        // one. It is also the mutex the other ledger-writing tests take, so it
        // is what stops them racing.
        let _ledger = crate::ledger::scratch("issue-intake-tick");
        // The acceptance criterion, end to end: a labelled issue produces an
        // inbox ticket with type, body and source_id within one tick, the
        // issue gets the comment, and running again creates nothing.
        let forge = RecordedForge::start(vec![
            ("GET /repos/48Nauts/demo/issues", github_issue_list()),
            (
                "POST /repos/48Nauts/demo/issues/75/comments",
                json!({ "html_url": "https://github.com/48Nauts/demo/issues/75#c1" }),
            ),
            (
                "GET /repos/48Nauts/demo/issues/75",
                json!({
                    "number": 75, "title": "t", "body": "", "state": "open",
                    "labels": [{ "name": "xnaut" }, { "name": "bug" }],
                    "user": { "login": "cand0rian" },
                    "updated_at": "2026-09-14T08:00:00Z",
                    "html_url": "https://github.com/48Nauts/demo/issues/75",
                }),
            ),
            ("PUT /repos/48Nauts/demo/issues/75/labels", json!([])),
        ])
        .await;
        let repo = control_repo(
            &format!("{}/48Nauts/demo.git", forge.base_url),
            IssueIntake {
                enabled: true,
                ..Default::default()
            },
        );
        let registry = repo.join("registry");
        let hosts = vec![host("github", &forge.base_url)];
        let linear = crate::settings::LinearSettings::default();

        let first = run(&repo, &registry, &hosts, &linear, None, false).await;
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].blocked, "", "intake refused to run");
        assert_eq!(first[0].created, vec!["DEMO-1"]);

        let filed = crate::project_management::ticket_list_in(&repo, Some("DEMO".into()))
            .unwrap()
            .into_iter()
            .find(|t| t.id == "DEMO-1")
            .expect("no ticket was written to the board");
        assert_eq!(filed.status, "inbox", "an issue is a request, not a job");
        assert_eq!(filed.ticket_type, "bug");
        assert_eq!(filed.owner, None, "intake never assigns anybody");
        assert_eq!(filed.source_id, "github:48Nauts/demo#75");
        assert!(filed.body.contains("Since 1.26 the first tab never draws."));
        assert!(filed.body.contains("From GitHub #75 by cand0rian"));

        // The comment reached the issue, naming the ticket.
        let comment = forge.one("POST", "/repos/48Nauts/demo/issues/75/comments");
        let comment: Value = serde_json::from_str(&comment.body).unwrap();
        assert_eq!(comment["body"], "Tracked as DEMO-1 in xNAUT.");
        // And the status went back as a label, in the same tick.
        assert_eq!(first[0].mirrored, 1);
        let put = forge.one("PUT", "/repos/48Nauts/demo/issues/75/labels");
        let put: Value = serde_json::from_str(&put.body).unwrap();
        assert_eq!(put["labels"], json!(["xnaut", "bug", "xnaut:inbox"]));

        // Second tick: the same list comes back and NOTHING happens.
        let before = forge.requests().len();
        let second = run(&repo, &registry, &hosts, &linear, None, false).await;
        assert!(second[0].created.is_empty(), "a second tick filed a duplicate");
        assert_eq!(second[0].mirrored, 0, "an unchanged status was pushed again");
        assert_eq!(
            crate::project_management::ticket_list_in(&repo, Some("DEMO".into()))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            forge.requests().len() - before,
            1,
            "a quiet tick must cost one list and nothing else, got {:?}",
            &forge.requests()[before..]
        );
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn a_status_change_updates_the_label_and_a_rescan_can_reach_an_old_issue() {
        let _ledger = crate::ledger::scratch("issue-intake-mirror");
        let forge = RecordedForge::start(vec![
            ("GET /repos/48Nauts/demo/issues", github_issue_list()),
            (
                "POST /repos/48Nauts/demo/issues/75/comments",
                json!({ "html_url": "x" }),
            ),
            (
                "POST /repos/48Nauts/demo/issues/76/comments",
                json!({ "html_url": "x" }),
            ),
            (
                "GET /repos/48Nauts/demo/issues/75",
                json!({
                    "number": 75, "title": "t", "body": "", "state": "open",
                    "labels": [{ "name": "xnaut" }],
                    "user": { "login": "c" }, "updated_at": "2026-09-14T08:00:00Z",
                    "html_url": "https://github.com/48Nauts/demo/issues/75",
                }),
            ),
            (
                "GET /repos/48Nauts/demo/issues/76",
                json!({
                    "number": 76, "title": "t", "body": "", "state": "open",
                    "labels": [{ "name": "enhancement" }],
                    "user": { "login": "c" }, "updated_at": "2026-09-14T08:05:00Z",
                    "html_url": "https://github.com/48Nauts/demo/issues/76",
                }),
            ),
            ("PUT /repos/48Nauts/demo/issues/75/labels", json!([])),
            ("PUT /repos/48Nauts/demo/issues/76/labels", json!([])),
        ])
        .await;
        let repo = control_repo(
            &format!("{}/48Nauts/demo.git", forge.base_url),
            IssueIntake {
                enabled: true,
                ..Default::default()
            },
        );
        let registry = repo.join("registry");
        let hosts = vec![host("github", &forge.base_url)];
        let linear = crate::settings::LinearSettings::default();
        run(&repo, &registry, &hosts, &linear, None, false).await;

        // Move the ticket on the board, the way an agent or the owner would.
        let path = repo.join("projects/DEMO/tickets/DEMO-1.json");
        let mut ticket: crate::project_management::TicketRecord =
            crate::project_management::read_json(&path).unwrap();
        ticket.status = "in_progress".into();
        crate::project_management::write_json_atomic(&path, &ticket).unwrap();

        let moved = run(&repo, &registry, &hosts, &linear, None, false).await;
        assert_eq!(moved[0].mirrored, 1, "the status change never reached the issue");
        let pushed: Vec<Value> = forge
            .requests()
            .iter()
            .filter(|r| r.method == "PUT" && r.path == "/repos/48Nauts/demo/issues/75/labels")
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect();
        assert_eq!(pushed.last().unwrap()["labels"], json!(["xnaut", "xnaut:in_progress"]));

        // #76 carries `enhancement` but not `xnaut`, and the cursor is now
        // past it. A rescan is the escape hatch, and on its own it still
        // changes nothing, because the trigger has not changed.
        let rescanned = run(&repo, &registry, &hosts, &linear, Some("DEMO"), true).await;
        assert!(rescanned[0].created.is_empty());

        // Switch the project to "every issue" and rescan: now #76 arrives,
        // typed from its own label rather than from the trigger's.
        crate::project_management::set_issue_intake_in(
            &repo,
            "DEMO",
            IssueIntake {
                enabled: true,
                trigger: Trigger::All,
                ..Default::default()
            },
        )
        .unwrap();
        let widened = run(&repo, &registry, &hosts, &linear, Some("DEMO"), true).await;
        assert_eq!(widened[0].created, vec!["DEMO-2"]);
        assert_eq!(widened[0].already_tracked, 1, "#75 already has a ticket");
        let second = crate::project_management::ticket_list_in(&repo, Some("DEMO".into()))
            .unwrap()
            .into_iter()
            .find(|t| t.id == "DEMO-2")
            .unwrap();
        assert_eq!(second.ticket_type, "feature", "`enhancement` is a feature");
        assert_eq!(second.source_id, "github:48Nauts/demo#76");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn a_project_with_intake_off_is_never_listed_and_one_with_no_host_says_why() {
        let _ledger = crate::ledger::scratch("issue-intake-off");
        let forge = RecordedForge::start(vec![("GET /repos/48Nauts/demo/issues", json!([]))]).await;
        let remote = format!("{}/48Nauts/demo.git", forge.base_url);
        let linear = crate::settings::LinearSettings::default();

        let off = control_repo(&remote, IssueIntake::default());
        assert!(
            run(&off, &off.join("registry"), &[host("github", &forge.base_url)], &linear, None, false)
                .await
                .is_empty(),
            "intake is opt-in and a project that did not opt in is not touched"
        );
        assert!(forge.requests().is_empty(), "an off project was asked anyway");
        let _ = std::fs::remove_dir_all(&off);

        // On, but no configured forge host serves that remote. Silence here is
        // the exact failure this module exists to stop, so it is a sentence.
        let on = control_repo(
            &remote,
            IssueIntake {
                enabled: true,
                ..Default::default()
            },
        );
        let reports = run(&on, &on.join("registry"), &[], &linear, None, false).await;
        assert_eq!(reports.len(), 1);
        assert!(
            reports[0].blocked.contains("no configured forge host"),
            "{}",
            reports[0].blocked
        );
        let _ = std::fs::remove_dir_all(&on);
    }

    #[test]
    fn intake_settings_are_written_to_the_project_record() {
        let repo = control_repo("https://github.com/48Nauts/demo.git", IssueIntake::default());
        let saved = crate::project_management::set_issue_intake_in(
            &repo,
            "DEMO",
            IssueIntake {
                enabled: true,
                trigger: Trigger::Labelled,
                label: "  triage  ".into(),
                linear_team: String::new(),
            },
        )
        .unwrap();
        assert_eq!(saved.label, "triage", "the label is trimmed before it is used");
        let project = crate::project_management::list_projects(&repo)
            .unwrap()
            .into_iter()
            .find(|p| p.key == "DEMO")
            .unwrap();
        assert!(project.issue_intake.enabled);
        assert_eq!(project.issue_intake.label, "triage");
        assert_eq!(project.revision, 2, "a toggle is a revision like any other write");

        // A label trigger with no label would match every issue, which is the
        // opposite of what it says. Refused rather than quietly widened.
        assert!(crate::project_management::set_issue_intake_in(
            &repo,
            "DEMO",
            IssueIntake { enabled: true, label: "   ".into(), ..Default::default() },
        )
        .is_err());

        // Switching intake back off leaves the key out of the JSON entirely,
        // so a project nobody configured has no diff to review.
        crate::project_management::set_issue_intake_in(&repo, "DEMO", IssueIntake::default())
            .unwrap();
        let raw = std::fs::read_to_string(repo.join("projects/DEMO/project.json")).unwrap();
        assert!(!raw.contains("issue_intake"), "{raw}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn a_github_host_written_out_in_full_is_not_rewritten_to_github_com() {
        // The rule used to be "unless it says api.github.com, use
        // api.github.com", which sent every GitHub Enterprise request to the
        // wrong host. This test is the recorded server standing in for one.
        let forge = RecordedForge::start(vec![(
            "GET /api/v3/repos/48Nauts/xnaut/issues",
            json!([]),
        )])
        .await;
        let source = ForgeSource {
            host: host("github", &format!("{}/api/v3", forge.base_url)),
            owner: "48Nauts".into(),
            repo: "xnaut".into(),
        };
        assert!(source.list_since("").await.is_ok());
        forge.one("GET", "/api/v3/repos/48Nauts/xnaut/issues");
    }

    #[test]
    fn labels_choose_the_ticket_type() {
        let t = |labels: &[&str]| {
            type_from_labels(&labels.iter().map(|l| l.to_string()).collect::<Vec<_>>())
        };
        assert_eq!(t(&["bug"]), "bug");
        assert_eq!(t(&["xnaut", "Bug"]), "bug", "labels are not case-sensitive");
        assert_eq!(t(&["regression"]), "bug");
        assert_eq!(t(&["feature"]), "feature");
        // GitHub's stock label for a feature request. Without it every feature
        // request off a default repo files as a task.
        assert_eq!(t(&["enhancement"]), "feature");
        assert_eq!(t(&["feature request"]), "feature");
        assert_eq!(t(&[]), "task", "no label is a task, not a guess");
        assert_eq!(t(&["question", "wontfix"]), "task");
        // A bug that is also tagged as an enhancement is a bug: the more
        // specific claim wins, and the order is deliberate.
        assert_eq!(t(&["enhancement", "bug"]), "bug");
    }

    #[test]
    fn the_trigger_decides_what_is_wanted() {
        let plain = issue("1", &[]);
        let tagged = issue("2", &["xnaut"]);
        let other = issue("3", &["triage"]);
        let mut intake = labelled();
        assert!(!wanted(&plain, &intake.trigger, &intake.label));
        assert!(wanted(&tagged, &intake.trigger, &intake.label));
        assert!(!wanted(&other, &intake.trigger, &intake.label));

        intake.label = "triage".into();
        assert!(!wanted(&tagged, &intake.trigger, &intake.label));
        assert!(wanted(&other, &intake.trigger, &intake.label));

        intake.trigger = Trigger::All;
        assert!(wanted(&plain, &intake.trigger, &intake.label));

        // An empty label under Labelled must not silently become All.
        let empty = IssueIntake {
            trigger: Trigger::Labelled,
            label: String::new(),
            ..Default::default()
        };
        assert!(!wanted(&plain, &empty.trigger, &empty.label));
        assert!(wanted(&tagged, &empty.trigger, &empty.label));
    }

    #[test]
    fn a_labelled_issue_becomes_a_ticket_with_type_body_and_source_id() {
        let issues = vec![issue("75", &["xnaut", "bug"])];
        let decided = plan(
            &issues,
            "",
            &HashSet::new(),
            "github:48Nauts/xnaut",
            "GitHub",
            &labelled(),
        );
        assert_eq!(decided.create.len(), 1);
        let made = &decided.create[0];
        assert_eq!(made.ticket_type, "bug");
        assert_eq!(made.source_id, "github:48Nauts/xnaut#75");
        assert_eq!(made.title, "Issue 75");
        assert!(made.body.starts_with("It is broken."));
        assert!(
            made.body.contains("From GitHub #75 by cand0rian"),
            "the ticket must say where it came from: {}",
            made.body
        );
        assert!(made.body.contains("https://github.com/48Nauts/xnaut/issues/75"));
        assert_eq!(decided.cursor, pad_number(75));
    }

    #[test]
    fn the_since_id_guard_stops_a_second_tick_creating_anything() {
        let issues = vec![issue("75", &["xnaut"]), issue("76", &["xnaut"])];
        let first = plan(
            &issues,
            "",
            &HashSet::new(),
            "github:48Nauts/xnaut",
            "GitHub",
            &labelled(),
        );
        assert_eq!(first.create.len(), 2);
        assert_eq!(first.cursor, pad_number(76));

        // Second tick, same list, the cursor the first one stored.
        let second = plan(
            &issues,
            &first.cursor,
            &HashSet::new(),
            "github:48Nauts/xnaut",
            "GitHub",
            &labelled(),
        );
        assert!(
            second.create.is_empty(),
            "the cursor must stop a re-list creating the same tickets again"
        );
        assert_eq!(second.cursor, first.cursor, "the cursor never goes backwards");
    }

    #[test]
    fn the_source_id_guard_survives_a_lost_cursor() {
        // The state file is a cache. Losing it re-lists everything, and the
        // board is what stops the duplicate.
        let issues = vec![issue("75", &["xnaut"]), issue("76", &["xnaut"])];
        let seen: HashSet<String> = ["github:48Nauts/xnaut#75".to_string()].into_iter().collect();
        let decided = plan(
            &issues,
            "",
            &seen,
            "github:48Nauts/xnaut",
            "GitHub",
            &labelled(),
        );
        assert_eq!(decided.create.len(), 1);
        assert_eq!(decided.create[0].source_id, "github:48Nauts/xnaut#76");
        assert_eq!(decided.already_tracked, 1);
    }

    #[test]
    fn the_cursor_moves_past_issues_the_trigger_rejected() {
        // Otherwise an unlabelled issue is re-considered every three minutes
        // for the life of the repo.
        let issues = vec![issue("80", &[]), issue("81", &[])];
        let decided = plan(
            &issues,
            "",
            &HashSet::new(),
            "github:48Nauts/xnaut",
            "GitHub",
            &labelled(),
        );
        assert!(decided.create.is_empty());
        assert_eq!(decided.cursor, pad_number(81));
    }

    #[test]
    fn issue_numbers_are_padded_so_nine_sorts_before_ten() {
        // Cursors are compared as strings. Unpadded, "9" > "10" and intake
        // stalls on the tenth issue.
        assert!(pad_number(9) < pad_number(10));
        assert!(pad_number(99) < pad_number(100));
        let issues = vec![issue("9", &["xnaut"]), issue("10", &["xnaut"])];
        let decided = plan(
            &issues,
            &pad_number(9),
            &HashSet::new(),
            "github:48Nauts/xnaut",
            "GitHub",
            &labelled(),
        );
        assert_eq!(decided.create.len(), 1);
        assert_eq!(decided.create[0].reference, "10");
    }

    #[test]
    fn an_empty_issue_body_still_says_where_it_came_from() {
        let mut bare = issue("7", &[]);
        bare.body = "   ".into();
        bare.author = String::new();
        let body = intake_body(&bare, "Forgejo");
        assert!(body.starts_with("From Forgejo #7 by an unnamed reporter"), "{body}");
    }

    #[test]
    fn source_ids_round_trip_for_both_shapes() {
        assert_eq!(
            source_id("github:48Nauts/xnaut", "75"),
            "github:48Nauts/xnaut#75"
        );
        assert_eq!(source_id("linear:ENG", "ENG-123"), "linear:ENG-123");
        assert_eq!(
            reference_of("github:48Nauts/xnaut", "github:48Nauts/xnaut#75").as_deref(),
            Some("75")
        );
        assert_eq!(
            reference_of("linear:ENG", "linear:ENG-123").as_deref(),
            Some("ENG-123")
        );
        // A source never claims another's tickets: the mirror would otherwise
        // push one repo's status onto another repo's issue.
        assert_eq!(reference_of("github:48Nauts/xnaut", "forgejo:48Nauts/xnaut#75"), None);
        assert_eq!(reference_of("github:48Nauts/other", "github:48Nauts/xnaut#75"), None);
        assert_eq!(reference_of("linear:ENG", "linear:OPS-1"), None);
        // A team key that is a prefix of another must not claim its issues.
        assert_eq!(reference_of("linear:ENG", "linear:ENGINE-1"), None);
    }

    #[test]
    fn the_mirror_pushes_only_what_changed() {
        let tracked = vec![
            ("github:o/r#1".to_string(), "inbox".to_string()),
            ("github:o/r#2".to_string(), "done".to_string()),
        ];
        let mut mirrored = HashMap::new();
        // Nothing pushed yet: both go.
        assert_eq!(mirror_plan(&tracked, &mirrored).len(), 2);
        mirrored.insert("github:o/r#1".to_string(), "xnaut:inbox".to_string());
        mirrored.insert("github:o/r#2".to_string(), "xnaut:done".to_string());
        assert!(
            mirror_plan(&tracked, &mirrored).is_empty(),
            "a tick that changed nothing must make no calls"
        );
        // A status change is one push, for the one that moved.
        let moved = vec![
            ("github:o/r#1".to_string(), "ready".to_string()),
            ("github:o/r#2".to_string(), "done".to_string()),
        ];
        assert_eq!(
            mirror_plan(&moved, &mirrored),
            vec![("github:o/r#1".to_string(), "xnaut:ready".to_string())]
        );
    }

    #[test]
    fn the_status_label_replaces_the_old_one_and_keeps_the_rest() {
        let current = vec![
            "bug".to_string(),
            "xnaut".to_string(),
            "xnaut:inbox".to_string(),
        ];
        let next = labels_with_status(&current, "xnaut:in_progress");
        assert_eq!(next, vec!["bug", "xnaut", "xnaut:in_progress"]);
        assert!(
            !next.contains(&"xnaut:inbox".to_string()),
            "the old status label must go, or the issue carries two"
        );
        // The trigger label itself is not a status label and must survive.
        assert!(next.contains(&"xnaut".to_string()));
    }

    #[test]
    fn the_comment_names_the_ticket() {
        assert_eq!(tracked_comment("XNAUT-363"), "Tracked as XNAUT-363 in xNAUT.");
    }

    #[test]
    fn intake_settings_default_to_off_and_labelled() {
        let intake = IssueIntake::default();
        assert!(!intake.enabled, "intake is opt-in per project");
        assert_eq!(intake.trigger, Trigger::Labelled);
        assert_eq!(intake.label, "xnaut");
        assert!(intake.is_default());
        // An older project.json has no key at all and must still read.
        let parsed: IssueIntake = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, IssueIntake::default());
    }

    #[test]
    fn linear_issue_json_maps_to_an_incoming_issue() {
        let node = json!({
            "identifier": "ENG-123",
            "title": "The sweep stalls",
            "description": "It stops after one verify.",
            "updatedAt": "2026-09-14T09:30:00.000Z",
            "url": "https://linear.app/48nauts/issue/ENG-123",
            "creator": { "displayName": "Andre" },
            "labels": { "nodes": [{ "name": "Bug" }, { "name": "xnaut" }] },
        });
        let issue = map_linear_issue(&node);
        assert_eq!(issue.reference, "ENG-123");
        assert_eq!(issue.cursor, "2026-09-14T09:30:00.000Z");
        assert_eq!(issue.author, "Andre");
        assert_eq!(issue.labels, vec!["Bug", "xnaut"]);
        assert_eq!(type_from_labels(&issue.labels), "bug");
        assert_eq!(source_id("linear:ENG", &issue.reference), "linear:ENG-123");
    }

    #[test]
    fn linear_cursors_are_timestamps_and_still_guard_a_second_tick() {
        let one = map_linear_issue(&json!({
            "identifier": "ENG-1", "title": "a", "updatedAt": "2026-09-14T09:00:00.000Z",
            "labels": { "nodes": [{ "name": "xnaut" }] },
        }));
        let two = map_linear_issue(&json!({
            "identifier": "ENG-2", "title": "b", "updatedAt": "2026-09-14T10:00:00.000Z",
            "labels": { "nodes": [{ "name": "xnaut" }] },
        }));
        let issues = vec![one, two];
        let first = plan(&issues, "", &HashSet::new(), "linear:ENG", "Linear", &labelled());
        assert_eq!(first.create.len(), 2);
        assert_eq!(first.cursor, "2026-09-14T10:00:00.000Z");
        let second = plan(
            &issues,
            &first.cursor,
            &HashSet::new(),
            "linear:ENG",
            "Linear",
            &labelled(),
        );
        assert!(second.create.is_empty());
    }

    #[test]
    fn a_graphql_error_is_an_error_even_with_a_200() {
        // Linear answers 200 and puts the refusal in the body. A status-only
        // check reads that as success and then finds no issues, forever.
        let refused = r#"{"errors":[{"message":"Authentication required"}],"data":null}"#;
        let error = linear_payload(refused).unwrap_err();
        assert!(error.contains("Authentication required"), "{error}");
        let fine = r#"{"data":{"issues":{"nodes":[]}}}"#;
        assert!(linear_payload(fine).is_ok());
        assert!(linear_payload("not json").is_err());
        assert!(linear_payload(r#"{"noData":1}"#).is_err());
    }

    #[test]
    fn the_state_file_round_trips_and_an_absent_one_is_empty() {
        let dir = std::env::temp_dir().join(format!("xnaut-intake-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read_state(&dir).cursors.is_empty());
        let mut state = IntakeState::default();
        state.cursors.insert("github:o/r".into(), pad_number(75));
        state
            .mirrored
            .insert("github:o/r#75".into(), "xnaut:inbox".into());
        write_state(&dir, &state).unwrap();
        let back = read_state(&dir);
        assert_eq!(back.cursors.get("github:o/r"), Some(&pad_number(75)));
        assert_eq!(back.mirrored.get("github:o/r#75").map(String::as_str), Some("xnaut:inbox"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
