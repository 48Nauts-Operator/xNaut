# Live Journal

Open **Workspace → Journal** to keep project work beside the conversation.

## Scope and navigation

The top selector chooses the project, an assigned release, a task group or a
single ticket. **Following · Pin** keeps the chosen scope while the conversation
changes. The menu labelled **Journal history and tools** contains saved dates,
refresh and Open in Wiki.

The six sections have separate purposes:

| Section | What it shows |
| --- | --- |
| Tickets | Compact work-state groups; select a ticket for its description, acceptance criteria, evidence and attempts. |
| Timeline | Timeline, Waterfall, Activity Map and Event log views of recorded work. |
| Agents | Workers associated with this project, with last observations and session links where identities match. |
| Insights | Recorded completion history and a full-year activity heatmap. |
| Notes | Saved Journal notes grouped by date, with month and author filters. |
| Handoffs | Recorded outcomes and saved handoff documents. |

**Needs you** contains scoped review decisions. Selecting Review opens the
relevant ticket; a saved review request is not evidence that its work passed.

## Ticket evidence

Open a ticket, then **Open trace**, to inspect recorded attempts. Saved output
loads on request. Comparisons show changed attempt fields and retained output.
Activity Map nodes open an inline inspector with a link to the exact trace.

Green checkmarks mean a ticket author checked a criterion. Grey dashes mean it
is unchecked. These checkmarks are not independent verification results.

Waterfall bars span recorded status changes; points are dated observations.
Unknown or open boundaries do not receive invented durations. The map shows
recorded relationships, not inferred causality. The Event log has filters and
expandable details; dispatch groups are available in its history section.

## Activity and notes

The GitHub-style heatmap keeps the whole selected year visible. Choose a month
to narrow the activity scope, or select a day with recorded activity. Empty
partial-week padding keeps the grid aligned without adding dates to the year.
Yellow intensity is relative to the busiest loaded day in that year.

Notes load across daily Journal documents in the selected month. Adding a note
currently requires an existing project ticket. Saved questions remain notes;
use the agent conversation to request an answer.

## Conversations

Agent Space groups conversations by creation year, month and day. Use search to
find a thread, Archive to move a completed conversation out of the active list,
and the Archived view to revisit or restore it. Archiving a conversation does
not stop or delete a worker. Copy controls are available on user and agent
messages.

The **Sort** control above the conversation offers **Oldest first** (the default,
latest messages at the bottom) and **Newest first** (latest at the top). The
preference persists across threads and restarts. It changes presentation only;
message content, stored history and model context retain their original order.
Streaming replies stay with their message, and reading older content preserves
its position as the reply grows.

## Independent review

A published task PR can be reviewed even after its ticket is reassigned or its
recorded description changes. The current project must still authorize automatic
review for the same repository. Reviewer launches bind to the exact task and
base commits; owner-only controls, stopped groups, read-only/quarantine policy
and worker capacity still apply. Author repair retains its separate scope checks.

A handback filing conflict preserves the current owner's ticket and the published
PR evidence. Queueing a review does not mean a worker has launched. Inspect its
attempts, child run and report. Requeueing retains the previous blocker in the
status; the next attempt must pass the current checks.

## Current limits

This release does not implement delivery forecasting, cutoff dates, or a release
CI/merge aggregate. The Insights chart shows recorded completion transitions;
it must not be read as a forecast. Earlier history may be absent from the loaded
window. A flat line can mean no completion transitions are recorded there.

Release numbers come from actual ticket assignments. The release header appears
when that release is selected; project-wide views and ticket drill-down do not
show it. Unassigned tickets do not acquire the example release used in a mockup.

Worker status is an observation with a timestamp, not a liveness guarantee.
Missing phase timing, spend and token metrics remain explicitly unavailable.
