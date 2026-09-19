# Changelog

All notable changes to xNAUT are documented in this file.

## [1.27.1] - 2026-09-15

### Added
- Sidebar: a Sessions list behind its own rail icon, first in the row, with a badge for the live count. Every live zellij session on this machine: the ones xNAUT launched or adopted in blue with their agent's status word, the owner's own `cx-*` and `cl-*` sessions in yellow with their age, exited ones folded under a count. Click opens or focuses the tab, right-click offers Close with a question first, the + on the header opens a plain terminal tab. (XNAUT-402)

### Fixed
- Approve on a sign-off card no longer silently fails after the ticket was edited: the scope pin covers only the commits and files under review, not the body, so an evidence note or a status change while the card is open no longer refuses the decision. When a decision is refused, the card says why ("Refused: ...") in the Inbox and the flow view instead of a button that does nothing, and offers Re-review, which supersedes the stale job and opens a fresh one against the same passed verify. A fresh sign-off posts its own card instead of reusing an already answered one, and a superseded review no longer blocks the next job for the same commit. (XNAUT-399)
- The evidence video attached to a green verify is the ticket's own: the harvest prefers the result folder of a spec file the handback lists as changed, and falls back to the first green one only when the run touched no spec. `board.spec.ts` had won every ChessTrainer run alphabetically. (XNAUT-398)
- An approved sign-off reverted with "npm: command not found": the integration verify ran its steps through /bin/sh with the Finder-launched app's minimal PATH. The verify shell now gets the same PATH an agent launch gets (Homebrew, ~/.local/bin, the runtime dirs) ahead of the app's own. (XNAUT-410)
- A sign-off whose merge the app reverted can be reviewed again. The guard read ancestry alone, so a reverted commit stayed "already on dev" forever and every re-review was refused; it now looks for the revert the app itself committed. (XNAUT-411)
- A ticket write no longer fails because some other ticket, or the app's own event files, are uncommitted in the control repository. Only the ticket being written has to be clean; every mutation commits with `--only` its own paths, so nothing else can ride along. One stale file used to stop the whole board silently: a green sandbox verify could not move its ticket to complete and no sign-off started. (XNAUT-412)
- A ticket write no longer fails when the control repository's remote cannot be reached. The app fetches before writing, and a Finder-launched app has no ssh agent, so that fetch failed and its error became the write's error: every ticket update from the app failed while the same write from a terminal worked. The write now lands locally and the next reachable write reconciles. (XNAUT-414)
- Jury reviewers died with exit 71 on every plan review: sandbox-exec started `claude` and `codex` by bare name under the app's Finder PATH. Reviewers are now started by absolute path, resolved the way agent launches are. Each failure had become an owner card reading "missing, stale, late or invalid reviewer identity". (XNAUT-405)

### Changed
- Exited `xnaut-*` zellij sessions are pruned a minute after they end instead of a day, and the Sessions list never shows them; resurrecting one re-ran the agent and restored no scrollback, so they were dead names. The owner's own exited sessions stay, folded.
- Sessions never become tabs: one host tab in the strip shows whichever session the Sessions list selected, and selecting another swaps it in place (zellij keeps the previous one running). The list's + opens a plain terminal tab; starting zellij from inside a session nests one in another, which nobody wants.
- A session can be renamed from either side, double-click on the tab name or on the row (or right-click, Rename). The name is stored on the session, so it shows on the tab and in the list and survives switching; the zellij name stays on the row's second line.
- Session rows carry no fill; a one-pixel line in the kind's colour marks them, yellow around your own terminals, blue around the bots, two pixels on the active one. A rename shows on the row even when the app has adopted the session, and a session counts as yours unless its name starts with `xnaut-`.
- Your own sessions show activity too: one `ps` per repaint (no tty lookup, a tenth of a second) sums the CPU under each session's zellij server, and a busy one reads "working" with the same square snake in yellow. The rows repaint every five seconds while the list is open.
- A zellij session no longer parks its view above the bottom after a resize. Switching sessions, zooming the font or dragging the window makes zellij reflow its scrollback and keep the old line offset, so the pane read "SCROLL: 126/603" and the newest output was hidden until a key was pressed. After every resize of a zellij tab the app now asks zellij to scroll to the bottom; the re-attach also spawns at the pane's real size. The re-attached session is spawned at the pane's real size instead of 120 by 40 and resized a moment later, so zellij never reflows its scrollback on attach. The working word and the snake on your own sessions hold for fifteen seconds instead of flapping with every CPU sample. (XNAUT-415)
- Observatory, Automations and Tasks are single tabs: a second click jumps to the open one instead of opening another.
- The Inbox icon opens the Mesh surface as well as the right pane's flow view, so the click always shows the open asks.
- Tracked agent sessions no longer open a tab each on every poll and restart; the Sessions list is where they show, and a tab opens on click. The tab strip scrolls inside itself, so the controls on its right stay reachable however many tabs are open. Tabs driven by xNAUT agents are blue, the owner's own sessions yellow.

## [1.27.0] - 2026-09-14

The release where xNAUT built itself. 199 commits since 1.26.3; the last five
tickets (354, 356, 357, 370 and the fixes they exposed) were dispatched,
verified and merged by the app on the fleet host with one owner click each.

### Added
- **A workspace per project.** A project opens on its code: file tree, the
  file in the centre, every other surface (Work, Delivery, NAUT-Flow, Vault,
  Memory) as a tab beside it. Every file type opens colour-coded. "Start
  something new…" on a project makes the ticket and, if asked, the worktree.
- **Sidebar: an icon rail and a project tree.** Five icons and a More menu
  replace twelve rows; projects with their worktrees as children; Pinned as
  its own block that keeps a project's worktrees; the name opens and folds.
- **One Code View.** Files and diffs render through one module across the
  app, with green and red rows in diffs.
- **The Work list.** List by default and remembered; every header sorts,
  Priority and Status by rank; the filter reads the columns, not the body
  (`status:review owner:claude release:1.28`); a Release column.
- **NautBot offers the swarm.** "Work on all open tickets for X" becomes a
  plan card consumed by the first yes; every run goes through dispatch, the
  registry, the jury and the ledger. The Multi-Agent Manager pane is gone.
- **NAUT-Flow personas are agent profiles**, matched by role; a role nobody
  holds runs as NautBot and says so. The Agent roster page is gone.
- **A Researcher who looks outside.** `@researcher` on Perplexity; the Analyst
  and Architect stages get a brief with numbered sources and cite from it.
- **The core team** (off by default): Researcher, Reviewer, PoC, Judge, a
  loop that reads other people's code, files findings, builds a PoC in a
  worktree under a budget, and writes `council.verdict`. Nothing merges
  without a click.
- **Machine roles.** `instance.role` = fleet, workstation or sandbox; the
  instance id, role and version on every ledger line and run manifest.
- **Ticket links in chat.** Ids in a reply are links with a hover card;
  click opens the ticket. Replies render as markdown.
- **Sessions in the Observatory**, grouped by project; foreign sessions past
  their TTL are reaped; a compaction storm is caught and the run ended.
- **Memory view**: what xNAUT remembers, per project.
- A check is a gate, a soft signal or a threshold; the example policy is not
  the default policy; routes refuse by default and never take identity from
  the body.

### Fixed
- A Claude runtime whose gateway is down runs on its own subscription; it is
  never rerouted to a local server with a placeholder key, and a run parked
  on Claude Code's custom-key prompt is ended with a ledger line.
- The registry no longer marks a live run failed for a detached HEAD (a
  mutation check); a mismatch counts once the writer is gone.
- The sign-off signs a source the integration ref already contains instead
  of bouncing the owner's approval; a refused decision says why in the Mesh.
- The interface fits the window at any zoom; the sidebar footer stays.
- Ripgrep is found from a Finder-launched app; search outside a repo works.
- The Type column read the wrong key and was empty; release notes rendered
  raw; the composer painted scrollbars while empty.

### Changed
- Change Management removed; nine Projects sub-tabs folded into the
  workspace; the Overview page's stats moved to Delivery.

## [1.27.0-rc, first cut] - 2026-09-11

The release where delivery became readable. Four tickets, built in parallel
by five agents on disjoint files, then joined.

### Added
- **Delivery > Tests, rebuilt around the ticket.** A project dropdown and the
  project's runs on the left; the totals as donuts across the top; the centre
  split between the issue (title, type, priority, owner, branch, the ticket's
  own text) and what the verify proved (suite totals, failing test names, the
  steps as chips with exit code and duration, sandbox and commit). Raw logs
  are one control away instead of being the page. Underneath, the ticket's
  life in seven ordered stages: issue, proposed solution, final solution,
  tested, done, merged, learnings. A stage the ticket has not reached is shown
  empty, never omitted.
- **`delivery_lifecycle`.** One reader that joins the five places a ticket's
  story is already written: the ticket JSON, the plan gate job the jury
  approved, the handback, the verify record and the sign-off. Learnings come
  from the vault document's Shipped section, then the handback, then the
  incident memory, and are empty when none of the three has anything.
- **Evidence.** The sandbox now runs the UI suite with video and trace on,
  pulls them back before teardown, and keeps the failing test's video, its
  last frame and its trace plus one passing run's, under a file and byte cap.
  The record says when capture was attempted and why it is absent. A
  thumbnail on the run row opens an overlay; with no recording it says so.
- **A Memory view.** What xNAUT remembers, as a list with the note beside it,
  searched with the agents' own search rather than a second one built for
  display. For a ticket in flight it shows the recall block that went into
  its dispatch prompt, verbatim, assembled by the dispatcher itself.
- **`run_detail`.** A run could not be opened from anywhere: the registry
  owned the manifest, the signal, what a run waits on and its capture, and
  exposed none of it. Now it returns the manifest with a bounded tail of the
  capture, and says why the capture is missing when it is.
- **Per-step timing on a verify.** `VerifyStep` records when it started and
  how long it took, so a step chip can show a duration.

### Fixed
- **Actions rows open what they are about.** A row with a ticket opens the
  ticket, a row with a run opens the run, a row with a live session attaches
  it; the kind is a filter chip. Consecutive identical rows collapse into one
  with a count, expandable. Thirty-four rows for one fact was one fact
  rendered thirty-four times.
- **Recognition that means something.** `incidents::recognised` matched on
  shapes as short as "run failed", so it answered "seen 285 times before" to
  questions about unrelated tickets; it listed the same ticket repeatedly
  because `dedup` only collapses neighbours; and it named incidents from runs
  that carried no ticket as an empty name. A shape under four distinctive
  words now matches nothing.
- **A stuck rebase no longer blocks the memory.** The vault sync used
  `pull --rebase --autostash`; one conflicting human document left the vault
  mid-rebase and every memory commit after it failed for a day. Reads
  fast-forward only and first back out anything left half done; writes commit
  with signing off and, on a conflict, keep the commit local until the next
  write.

## [1.26.4] - 2026-09-11

The release where xNAUT developed itself. Seven tickets on this list were
built, verified, reviewed by two independent AI reviewers on different
runtimes and merged by the machine; XNAUT-317, the last of them, went from
dispatch to promoted with nobody in the loop at all.

### Added
- **The jury.** Plan approval and sign-off share one blind reviewer pair on
  different runtimes, with a confidence threshold, owner-tier rules for
  protected paths, and a sign-off that merges to `dev`, runs the integration
  build, promotes `uat` fast-forward-only, and reverts itself on red.
- **An agent may divide its own ticket.** `create_child_ticket` carves a child
  the agent owns; it inherits the parent's release, tags, documents and model
  requirement, is dispatched like any ticket, and the parent's handback waits
  for it. Two levels deep.
- **The swarm lane.** A branch where the jury does not run, a worker merges its
  own green build, and errors are expected. Nothing on it reaches `dev`
  without a person. The doctor reports both lanes side by side.
- **Throughput on the doctor endpoint.** Machine merges an hour, agent commits
  against hand commits, escalations per merge, and what each escalation was
  worth: a catch or a cost.
- **Incident memory.** An escalation now says how many times this shape of
  failure has been seen, on which tickets, and what closed it last time.
- **Parallel integration builds.** Approved sign-offs build concurrently on
  private clones instead of queueing behind one lock.
- **Guardrails in Settings.** The four kill switches finally have a surface.
- **Tags and a release field on tickets.** Integration stamps the release.
- **Mesh in four tabs**: All, You, Jury, Archived.
- **One launcher for every environment**: local, exe.dev, GitVM.
- **A launch floor.** Below 95% used, nothing new starts, and the ticket keeps
  its owner. A full disk is not the runtime's fault.
- **A failed startup says what failed** instead of nothing.

### Fixed
- **Sign-off asked a shared worktree whose work had drifted.** One worktree
  serves every ticket, so at most one could pass and the rest escalated. 293
  dead approvals came from this; the newest escalation now supersedes the
  ones it replaces.
- **A pushed merge counts as merged.** Agent worktrees were never reclaimed
  because the check read the local branch, which NautBot's push never moves.
  25 GB of build caches filled tron on the 9th.
- **A supervisor restart is not an absent reviewer**, and the integration
  verifier it restarts is re-run at most twice, not forever. Eleven builds for
  one merged ticket came from the unbounded version.
- **A runtime that could not start is not a candidate**, for every ticket, not
  only those naming a model; the refusal expires so a reinstalled CLI is tried
  again.
- **A review no longer spends a worker's daily launch slot.** Two tickets
  exhausted a day and took the jury with them.
- **An abandoned atomic-write temp file is not an uncommitted change.** One
  such file, left when the disk filled, blocked every board write from a
  machine for 26 hours.
- **The same triage list is asked again after six hours, not never.** A board
  that stopped changing was triaged once per process.
- **The dispatch prompt states what done means** rather than a numbered
  checklist, after Cursor's harness findings; every string a gate parses is
  unchanged.
- **The reviewed diff starts at the merge base**, not the moving integration
  tip, so newer fixes no longer read as deletions.
- **Verify records keep their test totals** when the step log is cut to its
  tail; a green run settles only a ticket awaiting review.
- **The Mesh says why a decision was refused** instead of showing a button
  that does nothing, and the same question is asked once.
- **Work already on the integration branch is not a decision.** The sign-off
  gate no longer sweeps a hundred historical tickets when it turns on.
- **The doctor reads projects through the resolver the fleet uses**, so
  `throughput` is not null on a machine whose settings never named the
  control repo.
- **tron's install restarts the launchd supervisor** instead of opening a
  second copy beside it.

## [1.26.3] - 2026-09-05

### Fixed
- **NautBot can move its own tickets.** Codex agents never received xNAUT's
  tool server, so the one agent whose job is the board asked the owner to
  move tickets for it. Codex now gets the server, with no secret on the
  command line.
- **A restart no longer doubles the agent list.** Every restart added an
  "adopted" twin beside each surviving agent, and each twin counted toward
  the launch ceiling. Rows whose session has ended are dropped, not kept.
- **The health check no longer waits for a machine-wide scan** on every
  request; it reads a snapshot refreshed in the background.

## [1.26.2] - 2026-09-05

### Fixed
- **A restart no longer spawns a second NautBot.** After a restart the app
  re-adopts a surviving agent but no longer holds its terminal, and the wake
  read that as "no agent" and launched another; four were created in one
  day. A wake now reaches the surviving session directly.
- **A verification the app died on is corrected within a sweep tick**, not
  only at the next start, and never when it happened within the last two
  minutes. A ghost run blocked its whole project for an hour today.
- **Release builds keep line tables**, so a freeze or crash in the field can
  be read as a backtrace instead of a list of addresses. The binary grows
  from 34 MB to 58 MB.
- **Two verifications no longer fight over one directory.** The sweep offers
  one verification per project per pass and none while that project has one
  running; the second handback waits a tick instead of failing at warm-up.
- **A surviving agent keeps its worktree after a restart.** The writer lease
  was held under the old app's process, so any agent could have taken over a
  worktree a durable agent was still writing in. Adoption re-takes it.

### Added
- **The loop's five stations ship seeded**: NautBot, a coding agent, Ralph
  (validates on a clean machine), Otto (releases only from a validator
  record), and the Librarian.
- **Publishing gets a state machine**, not a prompt.
- **Chat streams**: a message is a question, and the harness starts on a
  build handshake.
- **Windows install smoke gate** in the release workflow.
- **A housekeeper** reclaims the disk that agent worktrees ate.

## [1.26.1] - 2026-09-05

### Fixed
- **A woken agent no longer parks at an unsubmitted composer.** The agent
  runtime registry from before this build seeded once and never healed, so
  older installs still launched Claude Code with the draft flag and every
  wake sat in the composer waiting for an Enter nobody pressed. The registry
  now heals itself once on start. This is the fix every 1.26.0 install was
  missing.
- **Codex agents no longer stop at their own approval prompts.** NautBot on
  the Codex runtime parked on "Would you like to run the following
  command?" for every push, tool call and inbox message, and that prompt
  never reached the inbox. The runtime now runs with approvals off, the way
  the Claude runtime already did; xNAUT's own guard rails stay in front.
- **Two agents can no longer share one checkout.** The writer lease existed
  but was only taken by the build flow. Every launch takes it now, and the
  second agent is refused by name.
- **Agents can read the project's standing conventions.** The launch prompt
  told an agent to call a tool it did not have. `xnaut_resolve_marker` now
  exists, so branch naming, commit style and the test command are looked up
  instead of asked about.
- **A verification can pass on xNAUT itself.** The sandbox never installed a
  browser, looked for it at a macOS path on Linux, was fed the whole 21 GB
  build tree, and inherited a stuck sandbox after any app restart. Each is
  fixed; the first green verification of xNAUT's own code ran today.
- **A woken agent starts inside a git repository**, so its conventions and
  worktree rules apply from the first command.
- **A verification checks the ticket's own tree.** It used to run one fixed
  directory for every ticket in a project; the commit under test is now
  recorded on the result.
- **A finished agent hands back a structured record**: what changed, which
  commits, how it was verified, what was not finished. An empty or evasive
  handback is refused with the gaps listed.
- **Every launched agent gets xNAUT's tools.** Agents were editing the ticket
  store by hand because the MCP server was never in their launch config.
- **The Agent pane is one timeline**: today's live view, cost, sandbox runs
  and actions under collapsible date headers; the Flow Watch and Verify tabs
  are folded into it.
- **Native confirm() dialogs did nothing** in the app's WebKit, so "Remove"
  and every delete acted without asking. An in-app dialog replaces them,
  and 58 silent alert() calls show a toast.
- Dispatch a ticket to its owner in one action; answer the inbox from the
  phone; a per-thread harness switch keeps the transcript; the Observatory
  can show all 30 agents of a fleet run; the agent runtime registry is
  versioned with a diff against this build's defaults; scheduled runs and
  dispatched runs survive the app quitting; a boot self-check says why
  NautBot cannot work on this machine.

## [1.26.0] - 2026-09-03

### Fixed
- A wake delivers its submit as its own byte, so the message is sent rather
  than left in the composer, and a woken or adopted agent gets a tab with a
  live status dot.
- NautBot's board sweep runs on its own clock and completed its first
  successful pass. A failed verification cools down and then stops asking;
  a held ticket no longer blocks every ticket behind it.
- The work report is re-sourced from the evidence chain and works
  retroactively; an empty report says where it looked.
- Scheduled tasks run backend side, durably, without the Automations panel
  being open.
- The idle reaper collects finished agents instead of refusing on the app's
  own reflection.

## [1.25.2] - 2026-08-29

### Fixed
- **An agent finishing a ticket hands it back for real.** Agents got xNAUT's
  tools in 1.25.0, but their calls arrived anonymously, so the safeguards on
  those tools treated them as your own actions and stayed out of the way. A
  finished ticket stayed owned by the agent that finished it. Tool calls now
  identify the agent making them.

## [1.25.1] - 2026-08-29

### Fixed
- **Terminals work without an internet connection.** The terminal engine was
  fetched from a CDN each time the app started, so a brief network problem
  meant no terminal or agent session could be opened at all, and restarting
  the app was the only way back even once the connection returned. It now
  ships inside xNAUT.
- Crash reports in the debug log now include what went wrong, not only where.

## [1.25.0] - 2026-08-29

The first run where an agent worked a ticket end to end found three things
in the loop around it. All three are fixed here.

### Fixed
- **Agents can use xNAUT's own tools.** A launched agent was given the
  plugins you had enabled and nothing else, so the ticket, decision and
  document tools did not exist for it. It fell back to editing the project
  files by hand, which works but skips every safeguard those tools enforce.
  Every launched agent now gets them.
- **A woken agent works the ticket you woke it for.** The queue was ordered
  oldest-change-first, so a fresh assignment came last behind anything that
  had been sitting around. It now orders by what is already started, then
  priority, then recency, and a wake can name the ticket to start with.
- **A prefilled prompt gets submitted.** Claude Code is launched with the
  task already in its composer, and deliberately waits for a return key
  before running it. Nothing sent that key, so a woken agent sat at its
  input box looking idle until somebody pressed Enter by hand.

## [1.24.6] - 2026-08-28

### Fixed
- **A woken agent starts working instead of waiting at its input box.** The
  task was pasted into the agent's terminal but never submitted, because
  Claude Code holds a large paste for review and our Enter arrived inside
  the same keystroke. The agent looked idle and unresponsive; it was simply
  waiting for a return key that never came.
- **Flow Watch shows readable output.** Agent terminals draw by moving the
  cursor rather than writing spaces, so the previous view glued words
  together and lost every column. It now renders the terminal honestly.

## [1.24.5] - 2026-08-28

### Fixed
- **Asking you a question no longer leaves the agent with nothing to wait
  on.** A question or approval request held its reply for two minutes before
  answering, so an agent whose own timeout was shorter received nothing at
  all, not even the request's id, and could not follow it up. The request
  was on your screen the whole time; only the agent was blind to it. The
  first reply now comes back promptly and always carries the id.

## [1.24.4] - 2026-08-28

### Added
- **Answer a blocked agent from Flow Watch.** Open questions and approval
  requests now sit at the top of the Flow Watch pane, above the sessions:
  approve, deny, pick an option or type an answer without leaving the view
  where the work is running. They remain Mesh inbox items answered through
  the Mesh; this is the same store, one glance from the run it is blocking.

### Fixed
- **The Send button in Agent Space sent a placeholder instead of your
  message.** Clicking Send passed the click event itself as the prompt, so
  the agent received the text "[object PointerEvent]" and never saw what you
  typed. Pressing Enter always worked, which is why this looked like an
  agent problem rather than a button problem.

## [1.24.3] - 2026-08-28

### Fixed
- **An agent finishing a ticket now really hands it back.** The rule that
  `done` returns a ticket to NautBot, and that only NautBot may set
  `complete`, was enforced only when an agent wrote through chat. An agent
  working inside a run writes through the MCP tool, which had neither, so a
  finished ticket stayed owned by the agent that finished it and nobody was
  told it needed review. Both rules now live at the single write every
  caller passes through.

## [1.24.2] - 2026-08-28

### Added
- **Flow Watch.** A new right-pane view showing every running agent session
  as a collapsible row: status dot, click to open the live terminal output.
  Watching is read-only by construction; there is no way to type into a
  flow from the watch. Built because the first dogfood run of the ticket
  loop launched an agent nobody could see.

## [1.23.1] - 2026-08-28

Found in the first minutes of dogfooding 1.23.0.

### Fixed
- **Agents can be addressed by their display name.** "Claudi" is a display
  name; the handle is "claude". Waking an agent by its display name found no
  profile, and a ticket assigned to a display name was owned by a string no
  agent would ever match, so the loop no-opped silently. Spoken names now
  resolve to handles everywhere one is accepted, and an unknown name is
  refused with the roster so the caller can correct itself.

## [1.23.0] - 2026-08-27

**The supervised self-building loop.** NautBot can now run a ticket end to end
from chat: assign it, wake the agent, watch the work come back, verify it in a
sandbox, put a cross-model panel on it, and land it through a risk-scored
merge gate. Every step is guarded and every guard has a kill-switch. Plus the
four bridge changes the iOS client asked for.

### Added
- **Ticket pull loop.** Agents fetch their assigned tickets from
  `GET /v1/tickets/mine`; identity is resolved server-side from the session
  token, and the Foundation teaches every launched agent the loop. Setting a
  ticket to `done` hands it back to NautBot automatically.
- **`wake_agent`.** NautBot nudges an agent to check its tickets: typed into
  a live idle session, or a cold launch in its scratch workspace when none
  exists. Never types into a busy session.
- **Merge gate.** `merge_ticket` scores the diff deterministically (size,
  breadth, sensitive paths, test shrinkage, unverified); a score of 8 or more
  parks an approval in the Mesh inbox and waits for a human. Conflicts abort
  clean. `unmerge_ticket` reverts any landed ticket in one commit.
- **Kill-switches.** `freeze_merges`, `read_only`, `approve_everything`, and
  per-agent quarantine: one audited flag drops a whole enforcement layer, and
  only the owner flips them.
- **xFusion panels.** `xfusion_opinion`, `xfusion_debate` (multi-round, no
  judge, self-terminates on convergence), `xfusion_review` (a panel tries to
  refute that a done ticket is finished), `xfusion_refute` (a panel tries to
  kill a proposed merge).
- **Tester joint.** `verify_ticket` runs the repo's `.xnaut/verify.json` plan
  in a sandbox; a green record drops the merge gate's unverified risk, a red
  one refuses the merge.
- **One roster.** The worktree modal picks agents (`@handle · role`), not
  runtimes, and a profile-picked task gets the composed Foundation prompt.
- **Mobile bridge (the 1.22.2 list).** Push notifications behind a swappable
  seam (ntfy today, APNs-ready: set `push_ntfy_topic` in mobile.json) firing
  on inbox asks/approvals and agent state changes; read-only vault routes
  (`/api/vaults`, search, note); per-device tokens with one-phone revocation
  (`/api/devices`).

### Changed
- **Agents address each other by tag.** The agent-facing roster no longer
  names runtimes or models; a model that knows which model it is arguing with
  postures instead of answering.

### Fixed
- **Removing a zellij session no longer reports an error on success.** The
  delete guard now tolerates both of zellij's "already gone" phrasings.

## [1.22.1] - 2026-08-26

**1.22.0 shipped half of the mobile work and one regression.** Its tag was cut
before nine commits that were already written, so what went out claimed the
phone features and did not have them. This release is those nine commits.

### Fixed
- **xNAUT-created zellij sessions had no keybindings at all.** The generated
  layout used `keybinds clear-defaults=true`, which does not mean "unbind these
  seven keys", it means "remove every binding". A session created by 1.22.0 had
  no detach, no pane switching, no tab switching and no scroll mode, which
  leaves you stuck inside it. Only the seven colliding keys are unbound now, and
  a test fails if `clear-defaults` ever comes back. **Sessions created under
  1.22.0 keep the broken layout**: delete `~/.config/xnaut/layouts/*.kdl` and the
  next launch rewrites them.
- **Sessions with a capital letter in the name were rejected.** The validator
  compared the name against its own lowercased form, so `cx-Bucky` and every
  other real session failed. It now refuses only what actually cannot work:
  empty, over-long, leading-dash, slashes and control characters.
- Removing a session that was already gone answered `400` instead of `204`.
- Attaching a durable session from the phone no longer opens a window on the
  Mac, and no longer pins the session to phone width. Opt back in with
  `?surface=1`.

### Added
- **`Authorization: Bearer` on every bridge route.** The iOS client cannot
  authenticate without it, so 1.22.0 could not be connected to at all. Every
  existing `?token=` call still works.
- **The phone can answer a blocked agent.** `GET /api/inbox`,
  `POST /api/inbox/:id/decide`, `POST /api/inbox/:id/answer`.
- Opening a zellij session creates its PTY at the caller's grid, so the phone
  gets a terminal sized for the phone.

## [1.22.0] - 2026-08-26

### Added
- **Agents can work the ticket board.** An agent has three new tools:
  `list_tickets`, `create_ticket`, `update_ticket`. They go through the same
  write path the app uses, so a ticket an agent files is indistinguishable from
  one you filed: ticket JSON, an event, a git commit. Two rails hold the board
  honest. An agent may set a ticket to **done** and hand it back, and only
  NautBot may set it to **complete**, which means tested, checked and approved.
  A ticket body is only ever appended to, never rewritten, so an agent cannot
  tidy the history away.
- **An agent's exe.dev computer shows up beside it.** A VM an agent spins up
  used to exist only as a line in the transcript. The right pane now lists each
  exe.dev machine with its state and its ssh line: **terminal** mounts the VM's
  own web terminal right there in the pane, **web** opens its public HTTPS
  hostname in a tab.
- **Zellij sessions on the phone.** The mobile bridge lists your zellij sessions
  live-first, opens one as a tab, and removes one for good. A terminal tab now
  remembers the session behind it, so a durable session is marked as durable in
  the list instead of looking like a bare shell.

### Fixed
- Zellij's default keybindings no longer eat shell history and agent TUI keys:
  Ctrl p/n/o/t/h/s/q are unbound in the generated layout. Removing a session
  kills it as well as deleting it, so the row stops coming back.

## [1.21.2] - 2026-08-25

### Fixed
- **Adding a bundled plugin no longer fails with "No such file".** The check
  that runs when you add a plugin started the server from the raw catalog
  command instead of the resolved one, so exe.dev and NautGate Audit reported
  `//mcp/exe.py: No such file` in the installed app even though the script was
  bundled correctly. The check now launches exactly what a run launches.

## [1.21.1] - 2026-08-25

### Added
- **exe.dev in the plugin library.** `mcp/exe.py`, standard library only: an
  agent can create a persistent Linux VM with root and a public HTTPS hostname,
  run commands on it, and delete it. An alternative to GitVM, whose sandboxes
  are ephemeral and self-destruct on a timeout. Token scoping is exe.dev's own,
  so a key can be minted per run with a command whitelist and an expiry.

### Fixed
- **Bundled plugin scripts now start from the installed app.** A catalog entry
  pointing at `mcp/exe.py` only resolved when the working directory happened to
  be the source checkout, so in the released app the server failed to start and
  read as a broken plugin. The `mcp/` scripts ship inside the bundle and the
  path is resolved at launch. Anything you typed yourself is untouched.

## [1.21.0] - 2026-08-24

### Added
- **Two default agents out of the box.** NautBot is your guide and control layer
  for xNAUT. The Librarian knows your docs and data: it searches everything you
  have written, researches and drafts documents into the vault with the right
  structure, and turns ideas into diagrams in the open document.
- **Vault workspace.** A file browser for the linked project opens any file in
  the centre, code syntax-highlighted and markdown rendered. A Changes panel
  shows uncommitted work grouped by folder, files not yet pushed, recent commits
  tagged with the release they shipped in, and every worktree; any file or
  commit opens its diff in the centre. A per-project chat keeps each project's
  own thread, with a Clear Chat button. Diagrams render inside the document as
  mermaid.
- **Review queue** split into what needs you and what does not. Agents wake with
  a project brief.

### Fixed
- **Chat kept answering from the wrong model.** The provider you pick now holds
  through the fallback completion instead of dropping to the global default. The
  document assistant defaults to a funded, tool-capable route.
- A referenced note that does not exist is handled gracefully instead of
  erroring over your answer.
- Voice dictation reports a silent microphone plainly instead of inserting a
  phantom word.
- **NautGate.** A session whose launch binding expired is stopped instead of
  retrying in a loop, and a Max-plan agent stops rather than looping.
- Local provider URLs load from saved settings, fixing a stale model dropdown.
- Diagrams drawn from chat no longer get orphaned. Test and gitignore cleanups.

## [1.20.1] - 2026-08-21

### Fixed
- **Agents lost every tool on Anthropic routes.** Agent Space pushed its own
  "Thinking…" placeholder into the thread and then sent the thread as history,
  so every turn ended with an assistant message. That is a prefill, which the
  Anthropic lane refuses with a 400; the tool loop died on it and fell back to a
  plain completion, so the agent answered but could not act. LM Studio accepts a
  prefill, which is why switching provider looked like the fix. The placeholder
  no longer reaches the history, and a turn now drops any trailing assistant
  message before it is sent.

### Added
- **The NautGate join.** An export can now carry NautGate's own decision
  receipts beside xNAUT's execution record, and the offline verifier checks both
  chains and the cross-reference between them. Neither product depends on the
  other: with no gateway configured the export is byte-for-byte what it was.
- **NautGate in the plugin library.** `mcp/nautgate.py`, standard library only,
  with tools to list receipts, fetch one as an evidence bundle, and verify a
  bundle offline.

## [1.20.0] - 2026-08-20

### Added
- **The execution record.** Every tool call and every agent run appends a
  canonical record (JCS, RFC 8785) with a domain-separated SHA-256 and a hash
  link to the one before it, so changing record N breaks every link after it.
  Before this the three append-only logs held 2, 49 and 2 entries between them:
  the recording had never been wired.
- **Checkpoints signed on the HSM.** A Merkle root over a session's records,
  signed with SHA256_WITH_RSA on the Securosys TSB and chained to the previous
  checkpoint, which is what closes the two holes a chain alone leaves: age, and
  a truncated tail that still verifies. A checkpoint is only issued if the chain
  verifies first; a hardware signature over a broken root would look
  authoritative anyway.
- **Sealing, now meaning encryption.** Tool-call arguments (commands, paths,
  diffs) are encrypted locally with AES-256-GCM under a per-session data key,
  fresh nonce per blob. The data key's 32 bytes are wrapped by an HSM-held KEK.
  Deleting that one key file makes every blob of the session unreadable by
  anyone, us included, while the chain over it still verifies. Deleting lines
  from an append-only log cannot do that.
- **An offline verifier and a bundle export.** `mcp/xnaut_verify.py` checks an
  exported bundle with no xNAUT, no network and no HSM: chain links, Merkle
  roots, checkpoint chaining and the RSA signatures, using the standard library
  only. A redacted bundle, with every plaintext argument removed, still
  verifies, because nothing needed to verify it was in the arguments.
- **Plugin credentials live in the keychain** instead of the config directory
  (XNAUT-213). A plain read migrates an existing credential on first use.

### Fixed
- **The `claude --mcp-config` temp file was mode 644** with every plugin
  credential in it, readable by any process on the machine.
- **A record no longer carries a plaintext preview of its arguments.** The
  hashed record held the first 200 characters, which is the part a secret is
  usually in.
- **Reading a blob after a shred says so.** A shredded session left no trace, so
  a read handed back raw ciphertext as if it were the arguments. Garbage
  displayed as evidence is worse than an error saying the key is gone.

### Changed
- **The work session log no longer claims to be signed or tamper-evident.** Its
  Merkle chain is computed locally with no outside signature, so whoever holds
  the log can rewrite an entry and recompute the chain. It detects an edited
  report; it is not evidence against the party holding it. The HSM evidence
  chain above is the feature that carries that claim.

## [1.19.0] - 2026-08-20

### Added
- **Delivery: tests, releases and a report, per project.** Start Work binds the
  ticket to the pane it opened, so the log you are watching belongs to the
  ticket you started. Tests renders every verification run step by step with the
  command, the exit code and the log tail. Releases lists the tags and the
  tickets inside them. Report joins commits to tickets and names the ones
  sitting in review or done with nothing committed behind them. Sixteen
  verification records were already on disk, fifteen of them failed, and nothing
  in the app could show you why.
- **Proof of work, derived from the repository** (XNAUT-207). Ticket joined to
  commit on the id in the subject line, commit joined to release with
  `git tag --contains`. It reports the gap too, which is the useful part: of 149
  commits in five days, 29 carried a ticket id.
- **The route that answered you is named in the thread.** The gateway replies
  with the model it actually used, which is not always the model you picked. Six
  headers were being discarded, so a substituted model looked identical to the
  one you chose.

### Fixed
- **A deletion can no longer hide in the attestation log** (XNAUT-211). Each
  receipt carries its sequence number and the hash of the receipt before it, and
  the HSM signs over that link rather than over the bare digest, so the newest
  signature commits to the whole history. Deleting a line, reordering,
  truncating the head or editing a receipt in place all break the walk, and the
  browser verifier checks the chain rather than each signature alone.
- The bundle guard compared paths case-sensitively, and the sidebar smoke marker
  outlived the button it named.

## [1.18.1] - 2026-08-19

### Fixed
- **A Claude agent stopped paying per token for a plan you already have.**
  Handing the agent a gateway key alongside the gateway address overrode Claude
  Code's own logged-in session, so the subscription lane was never reached and
  every request billed the metered key. When that balance hit zero every Claude
  agent went silent at once. The agent now gets the address and no credential.

## [1.18.0] - 2026-08-19

### Added
- **Ask a model whether it can actually call a tool** (XNAUT-196, XNAUT-197).
  The picker lists hundreds of models and said nothing about which of them can
  run a tool call. One request settles it, and when the route refuses it quotes
  the upstream's own sentence. It probes the route, not the model: the same
  model id behaves differently through a subscription relay than through an API
  key, and it is the route that breaks.
- **A rule that asks instead of only refusing** (XNAUT-189). A veto can hold a
  tool call, send the question to your inbox and leave the agent waiting.
- **A plan you can annotate and answer** (XNAUT-192). Click the block you mean,
  attach a numbered note, then Approve or Request changes. The agent blocks
  until you answer and picks up the notes attached to the exact lines.
- **Two agents reaching for the same file is noticed while both are still
  running** (XNAUT-190), rather than found afterwards in the diff.
- **One response contract for every `xnaut_*` MCP tool** (XNAUT-193), and a
  headless agent can be watched in a session of its own.

### Fixed
- **Both chat surfaces run the same turn path** (XNAUT-194). The chat pane
  posted messages and nothing else while Agent Space ran the full tool loop.
- **A reply that lost its tools says so** (XNAUT-195), naming the model and
  quoting the upstream verbatim. That silence cost four days of hunting a bug
  that was one line in a log.
- **SSH gets a real channel** (XNAUT-200), so a session types and answers
  instead of connecting and going quiet.
- **Triggers match output rather than escape codes** (XNAUT-199). Creating a
  trigger never reached the backend at all before this.
- **Five surfaces stopped claiming more than the code did** (XNAUT-202). A
  capability shown as ON that was off in the runtime is a lie the app was
  telling on its own behalf.
- The veto hook read the envelope the harness actually sends, and four dead
  commands were removed (XNAUT-132, XNAUT-198). The policy editor had no way in.
- A mobile session with no tap is a phone with no mirror (XNAUT-201).

## [1.17.2] - 2026-08-18

### Fixed
- **Interface zoom stopped moving every click** (XNAUT-188). Context menus
  opened a third of the way down the screen and a terminal click selected three
  or four lines below the pointer. Any zoom other than 100% put the pointer and
  the app in different coordinate systems.

## [1.17.1] - 2026-08-18

### Added
- **Voice dictation in both composers** (XNAUT-187). Local whisper speech to
  text, nothing leaves the machine. The button had been there for months calling
  a browser API that WebKit does not implement, so it had never once recorded
  anything.
- Attestation receipts can be published from the plugin library.

### Fixed
- A project row stops offering dead zellij sessions you cannot attach to.

## [1.17.0] - 2026-08-18

### Added
- **Securosys Attestation plugin.** Sign digests on a Securosys Primus HSM and
  keep the receipts: hardware attestation for agent work, releases, or any
  artifact. Standalone by design; no NautGate required. A stdlib-only MCP
  server (`mcp/securosys-attest.py`) talks straight to the TSB REST API
  (`synchronousSign`); receipts append to `attestations.jsonl` in app support.
  Verified end to end against a CloudHSM SBX partition, and receipts verify in
  any browser at xnaut.dev/attest. The library shows the real Securosys mark
  (brand icons can now be data-URI images, not only simple-icons paths).

### Fixed
- **An agent allowed the network can actually bind a port.** The policy
  translated "network" into egress only, so a dev server died on bind.


## [1.16.2] - 2026-08-16

### Fixed
- **A build thread survives its second turn.** Every follow-up message in a
  codex build died the instant it started with `error: unexpected argument
  '--approve-for-me'`: `codex exec resume` takes a different argument set from
  `codex exec` and accepts neither `--sandbox` nor `--approve-for-me`. The
  first turn worked and the agent then looked mute, which made multi-turn work
  impossible. Resume has its own flags now, proven by running the real CLI
  twice — launch, take the session id from its own output, resume — for both
  codex and claude.


## [1.16.1] - 2026-08-16

### Fixed
- **Windows builds again.** 1.16.0 shipped with both macOS DMGs and the cask
  but no `.exe` and no `.msi`: the Windows leg died at link time with
  `LNK1181: cannot open input file 'sqlite3.lib'`. `rusqlite` was linking the
  SYSTEM SQLite, which macOS ships and Windows does not, so the failure could
  only ever appear in CI. It builds SQLite from source now (`bundled`), which
  is the same library on all three platforms.


## [1.16.0] - 2026-08-16

The agent system, rebuilt. This is **step one of two**: everything below is how
you talk to an agent and what an agent can do. Step two is the Task side —
one roster, so a task picks an AGENT rather than a runtime, and every run
reaches the Mesh with its task in the inbox (XNAUT-163). That migration is a
bigger job because the two paths grew separately, and it is deliberately not in
this release.

### Added
- **Chat first.** A message goes to the agent's own baseline model. No worktree,
  no zellij session, no coding CLI. Asking for a status is a question, and it is
  answered like one.
- **A build handshake.** When a request needs code, the agent says so and xNAUT
  asks WHERE. The run happens in a worktree under `.worktrees/`, never in the
  checkout you have open, and the repository is asked for once per thread.
- **Plugins (XNAUT-147).** A library of 162 MCP servers: 32 we have run
  ourselves, plus 130 compiled from the public registries. Nine work with no
  configuration at all; twenty more need only a credential. Enabling one is
  what makes it reach a run — claude via `--mcp-config`, codex via `-c
  mcp_servers` — and each is handed to ONE agent rather than pooled.
- **Agents can use their plugins in chat (XNAUT-161).** A turn opens the MCP
  servers that agent holds and offers their tools to the model. "How many repos
  do we have?" is answered from Forgejo's own tools.
- **Agents fix their own connectors.** Given a package that ships no
  executable, an agent inspects npm, searches for one that does, repairs the
  plugin and connects it, rather than handing back the error.
- **A canvas (XNAUT-162)** and **documents** in a split of the main screen,
  ported from Cockpit's concept canvas. The agent sends the whole graph or the
  whole document; boxes you move keep their positions; one step of undo.
  "Save to vault" writes into the work vault with its frontmatter.
- **The Librarian is an agent**, not a pane: it searches, reads and writes the
  vault through tools, and its old conversations were migrated into its threads.
- **The Mesh inbox (XNAUT-156).** Agents ask, request approval, notify and
  leave you tasks; a question parks until you answer it, and survives a restart.
- **Skills library (XNAUT-158)** with starring and grouping by source.
- **App zoom.** Cmd/Ctrl + = / - / 0 scales the whole interface, persisted.

### Fixed
- Failures say what the CLI actually said. A run that dies now prints its
  stderr instead of "open Terminal to see what it did".
- codex runs outside a git repository (`--skip-git-repo-check`); a scratch
  workspace is not a repo and it used to refuse to start.
- Zellij sessions are per run and exit when the run ends. Reusing one name
  meant the second message attached to the first run's finished session and
  ran nothing, while the chat replayed the old output.
- The right pane no longer swallows right-clicks: a view that holds a native
  webview takes it down when it is not on screen.
- The newest message is always readable; the composer sits beside the thread
  rather than floating over it.
- Credentials typed into a plugin persist. Connect is one backend call that
  writes, verifies the server starts, enables it and hands it over.


## [1.15.0] - 2026-08-14

### Added
- **Page tabs inside browser panes** (XNAUT-149). Every browser pane — split
  or full tab — gets a strip above the address bar: a `+` on the left opens
  another page, each page is a closable chip, and switching chips swaps
  native webviews with their state preserved (inactive pages park offscreen,
  the same mechanism as inactive tabs). Closing the last page closes the
  pane cleanly.

### Changed
- **The globe opens one browser tab.** The first click creates it; further
  clicks focus it instead of stacking new tabs. Shift-click still forces an
  additional tab. Pages are meant to multiply inside the pane, not in the
  tab strip.

### Fixed
- Removed a leftover bright-pink diagnostic outline that shipped on every
  browser pane.

## [1.14.1] - 2026-08-14

### Fixed
- **The Observatory listed nothing and the sidebar opened a black terminal
  while sessions were live** (XNAUT-140). A launchd-launched app inherits
  `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, where a bare `zellij` does not exist.
  `list_sessions` spawned exactly that, so the app concluded no sessions
  existed; clicking a project then ran `sh -c "zellij attach …"` under the same
  PATH, which died on spawn and left a black terminal with a blinking cursor.
  A terminal-launched instance inherits a full PATH, which is why the same
  binary behaved differently depending on how it was opened. Every zellij
  spawn now goes through the resolver with the Homebrew fallback — which sat
  in the same module under a comment claiming the list commands already used
  it — and the sidebar attach exports PATH the way the PM panel's startShell
  always has. Verified under the app's exact PATH, read off the running
  process.

## [1.14.0] - 2026-08-14

Autonomous verification, and a build stage that stops reporting success it did
not have.

### Added
- **Sandbox Verify: autonomous test-and-deliver.** The module is wired to the
  ticket panel (XNAUT-19) and Loops runs are bridged into GitVM sandbox
  verification (XNAUT-38 Phase 3), so a run is verified where it ran instead of
  reported as finished.
- **The decision log**, with an agent that summarises it over the build log and
  a brief that reads it back. Settled entries are marked so the summariser stops
  reporting fixed bugs as live.
- **Agents read what past runs learned** (XNAUT-129 step A), with Engram recall
  scoped strictly to the current project.
- **Build slices declare output ports** and are gated on delivering them, with
  typed handoff between slices made real rather than decorative (XNAUT-128).
- **Per-account usage labelling** in the footer (XNAUT-24).
- **Constrained Vault document tools** in the xNAUT MCP (XNAUT-14).
- The develop → test → fix loop is closed and the release is gated on it
  (XNAUT-122).

### Fixed
- **The update banner offered to update a version to itself, and covered the
  top bar.** `CURRENT_VERSION` was a hand-maintained constant last bumped at
  1.5.0, so every release after it concluded an update existed, including the
  one already running. The version now comes from `app.getVersion()`, and when
  that cannot be answered the check stays silent rather than offering an update
  it cannot justify. Separately the banner was `position:fixed` with no layout
  offset: measured with `elementFromPoint`, all nine top-bar controls returned
  the banner instead of themselves, and the only way out was the small dismiss
  button. It now sits in the flow above the bar. The second defect landed on
  every user the moment a genuine update existed.
- **A work log did not survive closing the app** (XNAUT-139). The session was
  always written to disk; only the in-memory pointer died with the process, so
  the monitor silently stopped recording, the file stayed marked active
  forever, and the hours went missing from the PM Space dashboard. On restart
  the app now offers the session back, one at a time, newest first. It never
  resumes by itself: time passes between the close and the relaunch that nobody
  worked, and where several logs were left open, adopting the newest silently
  would close whichever one was real. Resuming appends a marker entry so the
  gap is visible rather than folded into the hours.
- **Local AI providers ignored the configured endpoint.** `ask_ai` hardcoded
  `localhost:1234` for LM Studio and `localhost:11434` for Ollama, so a machine
  running LM Studio on another port got "connection refused" naming a port that
  was never configured anywhere. The endpoint now comes from settings, and the
  failure message names the endpoint that was actually dialled.
- **Build slices have a real failure state** instead of restarting forever
  (XNAUT-93).
- A new NAUT-Flow case no longer copies the previous one (XNAUT-17).
- The Knowledge Graph names the command and the path when a scan returns
  nothing, instead of reporting a null property error.

### Removed
- **AntBot.** No longer used. Its three commands, ACL entries, settings rows,
  provider option, auto-start wiring and startup detection are gone. Both AI
  paths used to try AntBot before the configured provider, which is why a
  missing binary was the first half of every AI error. "Explain command" used to
  type `antbot agent -m '…'` into the user's terminal, so it only worked if the
  CLI happened to be installed and it put a command in their shell history they
  did not write; it now asks the configured provider and answers in the chat
  panel.

### Testing
- The browser leg grew from 17 tests to 63: every top-bar surface asserting the
  container it names, a raw-object and stuck-spinner sweep, the update banner,
  the work-log resume prompt, the project workspace, and both Designer `@smoke`
  scenarios — which had come back untested from every run on every machine
  until now. 367 Rust tests. All 275 registered commands are ACL-covered.
- Every new test was mutation-checked: the rule it guards was broken on purpose
  and the test was confirmed to go red.
- `scripts/gui-smoke.sh` now refuses to drive an xNAUT it did not start. `APP=`
  looked like it named the bundle to drive; it did not, and it took over a
  running app instead.

## [1.13.10] - 2026-08-10

### Fixed
- **The "Settings" menu item could not be pressed by name.** The accessibility
  matcher works by substring, and three other controls contain that name: the
  "Open Settings" button added in 1.13.8, plus macOS's own "System Settings…"
  and "Show System Settings in Finder", which belong to the frontmost app's
  accessibility tree in every Mac app. Rather than guess between them the
  release test refused to press it, so the seven Settings sections went
  untested on every run since. The item is now labelled "xNAUT settings",
  which collides with none of them and still contains its visible text.
- A guard test now covers every control the smoke test presses, not just the
  close buttons. A fix in 1.13.8 made a control unaddressable in 1.13.9 and
  nothing caught it until someone ran the test by hand against the shipped
  build; that specific gap is closed.

## [1.13.9] - 2026-08-10

### Fixed
- **The Settings sections were unreachable without a mouse.** All seven were
  bare `div`s with no role, no keyboard tab stop and no accessible name, so a
  screen reader could not announce them and a keyboard user could not get to
  them. They are real buttons now.
- Every modal close button announced itself as `×`, thirteen of them
  identically. Each is named for the dialog it closes.

## [1.13.8] - 2026-08-10

### Fixed
- **An unconfigured Project Management module read as a failure.** It painted a
  red "module is disabled" error box on a fresh install, which describes a
  broken app rather than one waiting to be set up. It now offers setup.
  (XNAUT-124)

## [1.13.7] - 2026-08-10

### Fixed
- The UI could be hijacked into light colours on a Mac set to Light appearance.
- Two TypeErrors thrown during ordinary use of the interface.
- `npm test` was dead on macOS: it had a Linux Playwright path hardcoded.

## [1.13.6] - 2026-08-10

### Fixed
- **Windows, actually this time.** v1.13.5 gated the local runtime's module and
  its definitions but left two call sites compiled on Windows, so the build
  still failed on `cannot find function spin_up_local`. Both are gated now.
  Verified before tagging by compiling the crate with every `cfg(unix)` flipped
  to a never-true cfg and `cfg(windows)` to an always-true one, which reproduces
  the Windows compile locally: zero errors, only dead-code warnings for the
  functions Windows does not use. That check is what the previous three attempts
  were missing.

## [1.13.5] - 2026-08-10

### Fixed
- **Windows builds again.** The Designer's local runtime is built out of `lsof`,
  `setsid`, a login shell and Unix process groups, none of which exist on
  Windows, so v1.13.4's Windows leg failed to compile and that release shipped
  with no `.exe` and no `.msi`. The module is now `#![cfg(unix)]`, every call
  site is gated, and `is_local()` returns false on Windows so designs there take
  the sandbox path. Local mode is macOS and Linux; saying so once is better than
  a half-working port.

## [1.13.4] - 2026-08-09

**The Designer works without a sandbox.** Until now it required the GitVM CLI,
an API key and Tailscale reach, which is 48Nauts infrastructure. For everyone
else the Designer opened, said "Starting the sandbox…", and never recovered.
New designs now run locally by default.

### Added
- **Local runtime for the Designer (XNAUT-118).** A design is served from your
  own machine on a loopback port. No warm-up, no lease, no rsync, no tunnel, no
  teardown. It is small because the design agent already ran here and wrote
  straight into the vault; the sandbox was only ever where the result was
  *served*. Publish becomes a near no-op and stop cannot lose work, because the
  vault is the working copy.
- **A Local / Sandbox switch** in the design header, per design rather than
  global, so one project can have a throwaway built locally and a client-facing
  one on a shareable URL. New designs default to local; existing designs keep
  the runtime they were created under, so none of them abandon a running
  sandbox.
- **A durable log per design**, in the same store as the build log, with a
  deadline on every sandbox call. A hang is now a reported failure with a
  duration attached rather than a spinner.

### Fixed
- **`gitvm run` forced a PTY the app could never have.** It requested a TTY
  unconditionally; xNAUT launches it with no controlling terminal, so OpenSSH
  refused and exited 255 before the remote command ran. xNAUT then ignored that
  exit status and probed an empty port for seven minutes. Two of these calls
  were found still hung from 2 August, so this had been failing for at least a
  week. Found by Codex; every experiment that missed it had a terminal.
- **The dev server never detached.** Backgrounding it inside a subshell meant
  the remote command never returned, so `gitvm run` held its SSH channel open
  for the caller's entire deadline while the page was already answering HTTP
  200. `setsid -f` in both runtimes.
- **A timed-out step that actually worked is no longer a failure.** Spin-up
  probes the URL before giving up, because whether something is serving is
  observable from outside. Narrowed so it recovers only a caller-side deadline:
  a failed rsync or install must stay failed even if a stale placeholder answers.
- **xNAUT competed with the agent for the dev server.** Locally the agent runs
  on the same machine and starts its own server to screenshot its work, so two
  servers existed for one project and the canvas showed ours: a finished site on
  :4399 behind a holding page on :53097. Spin-up now adopts a server whose
  working directory is the design folder, and the panel re-checks during a run
  so the canvas switches to the real site while the agent is still working.
  Adoption makes the port prove it speaks HTTP first, since the agent process
  shares that working directory and opens sockets of its own.
- **`lsof` could freeze the Designer.** Adoption shells out to `lsof`, which
  walks every mount and blocks in uninterruptible I/O on a wedged network share,
  on a five-second poll. Both calls now run behind a 5s deadline. Found when an
  SMB mount jammed and the test suite hung on that call for over a minute.
- **`[object Object]` in the canvas.** Backend progress arrives as strings and
  agent lines as objects; the renderer handled both, the status line did not.
  Worse than cosmetic: a status that never cleared kept the canvas on a spinner,
  so a finished site could not appear at all.
- **A design could get permanently stuck on its own sandbox**, and the holding
  page is evicted the moment a real project appears.

## [1.13.3] - 2026-08-09

### Fixed
- **Windows builds again, properly this time.** v1.13.1 broke it with a bash
  retry in a step shared with Windows, and v1.13.2's attempted fix forced that
  step to bash, which put MSYS perl ahead of Strawberry Perl and broke the
  OpenSSL build instead. The DMG retry only ever mattered on macOS, so the build
  step is now split by platform and the Windows one is the plain command it
  always was.

## [1.13.2] - 2026-08-09

### Fixed
- **Windows builds again.** The v1.13.1 release added a retry around macOS DMG
  packaging, written as a bash function in a step that runs on every platform.
  Windows runners default to PowerShell, which cannot parse it, so the Windows
  leg failed before it compiled anything and 1.13.1 shipped with no `.exe` or
  `.msi`. The build step is now pinned to bash on all platforms.

## [1.13.1] - 2026-08-09

### Fixed
- **A design could get permanently stuck on its own sandbox.** Opening it
  reported "Sandbox did not answer, destroying it and retrying" about a sandbox
  that was answering: running on the control plane, four hours left on the
  lease, HTTP 200 on its public URL. The design's own record had lost the
  sandbox id while the sandbox's state file still had it, and that combination
  wedges permanently, because reconnecting needs the first and creating a new
  one is refused by the second. xNAUT now adopts a sandbox it finds running,
  provided the control plane still knows it and the URL actually answers, and
  takes the lease from the server rather than assuming a fresh one.
- The retry message now says why the sandbox did not start. It was a fixed
  string, so a genuine failure looked exactly like a slow boot.

## [1.13.0] - 2026-08-09

Eight changes to the build stage, and a note on where they came from: none of the
ideas were ours. Three unrelated projects — Human-Agent-Society/CORAL (Apache
2.0), lamalab-org/corral (BSD 3-Clause) and cdknorow/coral (Apache 2.0) — each
had solved a piece of this, and one of them corrected a design we were a day from
shipping. Every borrowed mechanism names its source in its file header.

### Added
- **A durable build log.** Every build writes one append-only JSONL file to
  `~/Library/Application Support/xnaut/looms/logs/<build-id>.jsonl` (macOS) with
  a level, source and timestamp per event, and it is kept after the run. The
  Build run pane gained **Manager · Log · Files** sub-tabs: the Log tab filters
  by level with live counts, filters by source, searches, tails, and can reopen
  any earlier build. Before this the manager held a single status string that
  the next event overwrote, so a build's entire decision history existed for a
  few seconds and was then gone.
- **A Files view per slice.** Changed files with line counts and an inline diff,
  measured from the slice's **merge base** rather than the working tree — an
  agent that has already committed shows a clean `git status` while having
  written hundreds of lines, so a working-tree view reports it as idle. The base
  is chosen by closest fork rather than by name; against `main` a ten-file slice
  measured as 77 files and +8021 lines.
- **The manager keeps its history.** The Build run pane shows a levelled feed of
  what the manager decided and why, instead of its most recent sentence.

- **The acceptance gate reports a score, not a verdict.** It always ran real
  checks and then collapsed them to an exit code, so "four checks failing" and
  "forty checks failing" were the same answer and nothing watching could tell
  progress from thrashing. It now reports passed-of-total. A gate that crashes
  and emits no check lines scores *null*, never zero — a crash is the absence of
  a measurement, not a bad one.
- **Scores describe a commit.** The gate runs inside a throwaway detached
  worktree of one commit, so a score cannot drift because the agent saved a file
  mid-run.
- **Agents are interrupted when they stall, not on a timer.** The old behaviour
  nudged everyone every five minutes, which breaks the concentration of an agent
  that is working and leaves a stuck one alone for four more. The score history
  is tracked and an agent is nudged only when it stops improving — and the nudge
  quotes the exact failing checks, since the gate already knows them.
- **Agents on a project share notes.** A directory of markdown notes, symlinked
  into every build worktree, so three agents cannot each independently discover
  the same broken assumption. Scoped per project and permanent, in the vault —
  readable in Obsidian, with a git history.
- **Build slices can declare dependencies.** The planner used to be told to avoid
  them, which capped parallelism at whatever happened to be independent. Work now
  splits the way it actually divides: independent slices run at full width,
  dependent ones wait, and if a foundation fails everything built on it is marked
  unreachable and never starts. A waiting slice holds no worktree, so an
  unreachable one leaves nothing behind.
- **A bad dependency graph is rejected before anything runs** — cycles named,
  missing blockers reported, depth capped. A language model writes these plans,
  so a cycle is not a hypothetical.
- **Codex session cost.** We already read how much of your Codex *plan* was
  consumed; now each session's tokens and an estimated cost, read from Codex's
  own transcripts. Cached input is priced separately — on long sessions it
  dominates, and ignoring it would overstate cost roughly tenfold. The figure is
  a list-price estimate, not a bill.
- **Last activity per project in the sidebar** — the newest of the last commit
  and the most recently modified file, so a project being actively edited does
  not read as nine days stale.

### Fixed
- **Agents never received their goal.** The instruction was word-split on its way
  through the launch chain and the agent CLI took only the first positional, so
  every agent started with the prompt `Read` and the rest was discarded. Some
  explored the worktree, found the goal file themselves and carried on; others
  asked "Read what?" and stopped — which behaviour you got was luck, and it made
  a missing prompt look like a flaky agent. The prompt now travels in a file, so
  no quoting has to survive the chain.
- **Every agent was reported as dead.** The liveness probe shelled out to
  `pgrep -f`, which fails for *every* pattern under some locales — it exits
  non-zero with empty output while the spawn itself succeeds. The caller answers
  "dead" by force-killing the session, so healthy agents were destroyed and their
  slices eventually marked failed. Now uses `ps` with basename matching, and
  every inconclusive answer is treated as alive.
- **Healthy agents were nudged and restarted.** The stall detector read the
  acceptance gate — a *completion* metric — as a *progress* metric, so an agent
  writing code for four minutes without flipping a check looked stalled. Nudges
  now require the score flat AND nothing written, where "written" includes the
  agent's own status log (which git cannot see, because most repos ignore
  `*.log`).
- **A build could report success while missing a slice.** Recovering an
  in-flight build rebuilt it from live terminal sessions, so a slice held on a
  dependency — which by design has no session yet — was silently dropped. The
  build then went green and merged without it. The plan is now persisted and
  recovery reads that; a build recovered without a plan refuses to consolidate
  rather than merge an unknown fraction of itself.
- **Consolidation was a no-op that reported success.** The integrator's banner
  broke the terminal layout it was launched through, so nothing merged, nothing
  was pushed and no PR was opened — while the UI announced all three. The
  On-green checklist no longer ticks from slice colour either; a checkmark now
  means the repository actually changed.

- **A build could neither finish nor fail.** A dead agent was restarted forever,
  so a hopeless slice stayed "running" — and because consolidation waits for
  nothing to be running, the build sat there looking healthy. It now gives up
  after two restarts and says which slice died and why, and consolidation refuses
  to merge a build with a dead slice rather than shipping the survivors on top of
  a foundation that never landed.
- **The status pills stopped blinking in chorus.** One tab per session is now
  enforced where tabs are created rather than in each caller, so a double click
  no longer produces duplicates; and the pills use the sidebar's state
  vocabulary, where exactly one state animates. Previously both "working" and
  "waiting" pulsed, so idle sessions read as agents mid-thought.
- **The project list stopped flashing every three seconds.** The status poll
  rebuilt every row instead of updating the dots, which also restarted the
  animation on each tick.
- **Opening a new session starts the agent.** It created an empty terminal
  instead — the same command was used for attaching and for opening.
- **Sessions can be killed from the project page.** There was no control at all.

### Removed
- The agent pills in the top bar. The sidebar already shows each project's agent
  state, so the strip restated it in a second place.


## [1.12.0] - 2026-08-07

Backfilled 2026-08-08 — this release shipped without an entry. Reconstructed from
its 23 commits rather than from memory, so it describes what the commits did.

### Added
- **Run Claude Code and Codex against a local model.** Opt-in harness routing:
  when a local endpoint is configured and reachable, an agent is pointed at it
  instead of a dead default — the full harness, just not Anthropic. An agent is
  never handed a base URL that does not answer, because a refused socket makes
  Claude retry silently rather than fail.
- **All three harnesses in the + menu**, each routed its own way — Claude Code,
  Codex and Pi. Pi uses its own provider config; Codex needs its own model
  provider setting rather than an environment variable.
- **"Local (your LLM)" in the NautFlow and loom model pickers.**
- **Terminal tabs are Zellij-backed and outlive the app (XNAUT-66).** A named tab
  runs inside Zellij and detaches on close, so quitting no longer kills a
  long-running session. Gated behind an opt-in while pane nesting is unfinished.
- **The Validator's acceptance gate is actually executed.** `sandbox_verify`
  became runnable and appends `95-Build-Gate.py` as the final verify step —
  before this, the only references to the gate in the tree were the prompt that
  wrote it and the reset that deleted it.

### Fixed
- **A fresh install started with a dead UI.** `clearChatDisplay` dereferenced a
  missing element, reached only when no chat session had been saved, so the
  exception stopped `setupEventListeners()` and `createNewTab()` from ever
  running. `alert()` is a no-op in Tauri's WKWebView, so the error was invisible.
- **Claude local launch failed to spawn** — an empty working directory, plus PATH
  and probe fixes.
- **Every harness URL comes from Settings**; nothing is hardcoded.
- A model mismatch that surfaced as a bare HTTP 400 is now caught and named.
- `write_file` creates parent directories.
- The Agent harness switch sits in AI Providers, not the Tasks Mode tab.

### Changed
- Release CI updates the Homebrew cask on every tag, and installs `tauri-cli`
  with `--locked` — without it, transitive dependencies re-resolved per build and
  a `zune-jpeg` bump broke the release.

## [1.11.1] - 2026-08-03

### Fixed
- **A new design could never finish its first build.** Spin-up probed the sandbox URL once, right after starting the dev server, and treated a 502 as a dead sandbox — destroying it and retrying. But a 502 there means the tunnel is fine and `npm install` is still running, so each retry threw away the install and hit the same wall: the chat looped on "Sandbox did not answer — destroying it and retrying…" while the design folder stayed empty. A gateway error is now a reason to wait (with the wait shown in the chat), and only a genuine no-answer destroys and retries. The in-sandbox wait for the dev server to bind also went from 20 seconds to 300 — twenty was never enough to install a project's dependencies.


## [1.11.0] - 2026-08-02

### Added
- **Designer (XNAUT-61).** A tab in the project workspace that turns a sentence into a running application on a real, TLS-terminated hostname — no publishing step. A design is a real project (Astro / Reveal.js / Next.js by kind), scaffolded and edited by an agent, built and served by a GitVM sandbox, with the live site in the canvas and the chat beside it. Many designs per project, archived not deleted, source of record in the work vault.
- **Mobile companion (XNAUT-32).** A bridge that mirrors terminal sessions, files, git status, artifacts, the task runner, and the Observatory + Multi-Agent Manager to a phone over Tailscale.
- **NautFlow Validator.** A validation report in the centre pane with a conversational Validator, per-dimension PASS/FAIL against the whole documentation chain, assisted fixes, and a Build gate that blocks on FAIL.
- **Observatory.** Every live Zellij session listed and click-to-attach, durable run rows that survive a reload, local/sandbox runtime labels, and a "last project" tile.
- **Help overlay (XNAUT-46)** and **command snippets in a dropdown (XNAUT-47).**
- **Work-log clock in the topbar** — one click to start, red while recording, click again to stop and read the summary.

### Fixed
- **The long-running UI freeze (#54) — both halves.** Terminal output emits are coalesced into one merged event per 16 ms, so a streaming agent no longer saturates the WKWebView main thread; and the PTY reader now runs on a dedicated OS thread instead of the tokio pool, where a blocking `read()` parked one async worker per open session and showed up as multi-second keystroke stalls. These were two independent mechanisms found separately, and this is the first release that carries both.
- **Agent runs finish when the agent finishes.** `claude -p` can stall for minutes after its final message during MCP/hook teardown; runs now complete on the result event or a stream-idle shortcut instead of waiting for the process to exit.
- **Sandboxes are proven, not assumed.** Spin-up starts the dev server, probes the public URL, and only reports success on a real 2xx/3xx — otherwise it destroys the sandbox rather than leaving an orphan ingress hostname. Sandbox size and TTL now come from the template manifest instead of hardcoded values.
- A corrupt or null field in one project manifest no longer blanks the whole Projects board.
- Buttons that used `prompt()` / `alert()` did nothing at all — those are no-ops in Tauri's WKWebView.

### Removed
- **Ralph, the app's first agent flow.** Every part of it had a better owner: `loom_run` for dispatch, the Observatory and NautGate for cost, the fetched model catalog for model choice, NautFlow for the pipeline. Its one unreplaced idea — run the acceptance criteria and loop until green — is tracked as XNAUT-64.
- **The Live Error Monitor.** It only collected while its panel was open, its deduplication never fired, its AI analysis needed an API key this setup does not use, and it could not see agent runs, builds or sandboxes at all.


### Added
- **30 more bundled themes + bundled themes actually wired in.** warp-themes.js was never included in index.html — the 20 "bundled Warp themes" were dead code. It now loads before app.js, merges into THEME_PRESETS (defaults/customs win on name clash), and Settings shows a "Bundled Themes" group. Added 30 converted from iTerm2-Color-Schemes (MIT): Catppuccin (Mocha/Macchiato/Frappé/Latte), Gruvbox Dark/Light, Nord + Nord Light, Rosé Pine (3), Kanagawa Wave/Dragon, One Half Dark/Light, Ayu Dark/Mirage, Tokyo Night + Moon, Solarized Dark, Snazzy, Oceanic Next, Zenburn, Tomorrow Night, GitHub Light, Monokai Pro, Flexoki Dark, Alabaster, Embers Dark, Vesper — 50 bundled themes total.

## [1.9.2] - 2026-07-13

### Fixed
- **Freeze diagnostics ("frozen but alive").** Every backend panic is now appended to `~/Library/Application Support/xnaut/rust-panics.log` with thread + file:line. Since `panic = "abort"` was removed (1.8.9), a panicking tokio task dies silently and can leave the app frozen with a dead IPC bridge while every thread idles — observed 2026-07-13 (right pane stuck loading, new terminals black, hook server accepting but never answering). The tripwire names the culprit post-mortem and writes straight to disk, immune to the dead bridge.
- **Per-chunk terminal output logging is now opt-in** (`window.XNAUT_VERBOSE = true` in DevTools). An agent spinner emits ~10 chunks/sec; each chunk was console.log'd with its full base64 payload AND forwarded to debug.log over IPC — 4,446 log entries in 5 minutes from one busy terminal.
- NAUT-Flow editors preserve in-progress text during periodic project polling, window focus refreshes, and delayed initial Vault reads instead of replacing drafts with the stage template.

### Added
- Project Management MCP clients can list, search, read, create, and conflict-safely update Markdown documents inside a selected project's `work/Development/<project>` Vault scope. Paths are constrained to visible relative `.md` files and updates require the SHA-256 returned by the preceding read.

## [1.9.1] - 2026-07-12

### Added - Agent Loops
- **Visual Agent Loop builder.** Rete-powered node editing, versioned definitions, validation, bounded retries, approval gates, model governance, cost limits, and reusable delivery/triage templates.
- **LoopBuilder Agent.** Conversational requests compile into editable Agent Loop drafts and automatically repair validator feedback before saving.
- **Run launch context and console.** Starting a run now requires repository, branch, and ticket scope. Runs expose their durable event stream, current node state, and an explicit waiting-for-worker state.
- **Lifecycle controls.** Active definitions can be deactivated, and active runs provide an Emergency Stop that durably cancels pending and running nodes.

### Added - Project Docs and MCP
- **Project-scoped Docs.** Projects now include a Docs tab directly after NAUT-Flow. It reuses the Vault editor, defaults to the project's `Development/<project>` subtree, and provides an explicit All Vault switch without moving files.
- **Consolidated navigation.** The redundant left-side Vault entry is hidden once Project Management has at least one project; it remains available as a fallback when the module is disabled, unconfigured, or empty.
- **Shared Librarian.** Project Docs uses the existing right-pane Librarian and conversation history instead of opening another embedded chat.
- **Local Project Management MCP.** The xNAUT localhost agent server exposes Streamable HTTP-compatible project/ticket tools for listing projects, listing tickets, and creating or revision-safe updating tickets.
- **MCP connection details.** Project Settings shows the local endpoint and per-app-session bearer token and can copy a ready connection object.

### Fixed
- Agent Loop runs no longer appear to be actively executing when a ready node has not been claimed by a worker.
- Project Docs watchers are disposed when leaving the tab, while periodic project refreshes no longer reset the open document workspace.

## [1.9.0] - 2026-07-11

### Added - Agent system
- **Agent Library.** Reusable Markdown-backed Agent profiles define persona, role, skills, tools, constraints, outputs, access scopes, status, and runtime model assignment.
- **AgentFather.** A guarded setup assistant creates Agents on demand, separates creation from AgentFather's own settings, and requires explicit acknowledgement for privileged project access.
- **Runnable profiles.** Agent profiles can be tested and run with their assigned provider/model or the global default while preserving conversation history.

### Added - Forge issue review
- **In-app issue and PR detail.** Forgejo and GitHub work items open in an xNAUT review workspace with issue context, source links, and an Agent-assisted RCA surface.
- **Review isolation.** RCA conversations no longer display Librarian history, and issue analysis uses the shared document-style Markdown renderer.

### Added - NautFlow project workspace
- **Project lifecycle UI.** Project overview, contributors, quality gates, connected systems, commercial baselines, and stage ownership are presented in one full workspace.
- **Artifact stages.** Idea, Concept, Business Case, Product Requirements, Architecture, Data Model, API Design, Security Review, Development Plan, Sprint Stories, Tickets, Build, Test/Review, Release, and Engram Learning each have focused document workspaces.
- **Versioning and promotion.** Stage artifacts support multiple versions, preview/edit controls, active-document selection, and promotion into the next stage with Agent validation.
- **Agent document access.** Right-pane Agents receive live context for the visible artifact, can read linked Vault documents, and may update only the authorized active document with immediate editor refresh.

### Changed - Release boundaries
- **Forgejo-first project tracking.** xNAUT's Git-backed control repository and Forgejo are the primary project/ticket path; GitHub remains an optional forge integration.
- **Planned Change Management.** OpenSpec-style Change records, canonical project baseline reconciliation, and GitVM sandbox orchestration remain tracked by `XNAUT-5` and are not claimed as shipped in 1.9.0.

### Added - Frontier models and MCP drawing
- **Remote model discovery.** Agent Chat can enumerate models from the OpenAI-compatible endpoint selected in Settings, including OpenAI and OpenRouter, while retaining per-conversation model overrides.
- **Provider-aware Agent Chat.** The right-pane model selector groups models discovered from configured local, OpenAI, and OpenRouter providers and routes each request through the provider associated with the selected model.
- **Persistent Agent model selection.** The last provider and model selected in right-pane Agent Chat are stored in xNAUT's authoritative settings and survive Agent changes, workspace navigation, pane remounts, and application restarts instead of falling back to the global model. Browser storage remains only as a migration fallback.
- **Complete document actions.** Vault write requests receive an 8,192-token completion budget so full Markdown documents are not truncated into invalid tool JSON; malformed or incomplete tool output now reports the actual failure.
- **Live workspace context.** Right-pane Agents receive a fresh, non-persisted snapshot of the currently visible NAUT-Flow document, can read referenced Vault artifacts, and may write only the exact active artifact when their profile permits it. Successful writes update the visible editor immediately.
- **Local Excalidraw MCP.** xNAUT clones, builds, and starts the official MIT-licensed Excalidraw MCP server locally, binds it to loopback, and connects over Streamable HTTP. A hosted endpoint and bearer key remain optional overrides.
- **Capability-aware tools.** xNAUT discovers the drawing tools exposed by the configured local or remote server instead of assuming a fixed tool set.

### Added - Optional Project Management module
- **Opt-in setup.** Project Management stays disabled by default. Enabling it in Settings guides the user through creating a local Git control repository, optionally creating a private repository on a configured Forge, or connecting an existing xNaut control repository.
- **Git-backed records.** The module stores versioned project and ticket JSON, append-only workflow events, and machine-readable schemas outside source repositories. Every mutation creates a scoped Git commit without including unrelated files.
- **Agent-ready commands.** Project and ticket list/create/update commands provide the initial constrained service boundary for future xNaut agents and MCP access. Ticket updates use optimistic revisions to prevent silent overwrites.

### Fixed - Project Management setup
- **Explicit Forge credentials and ownership.** Private remote setup now distinguishes organization repositories from repositories owned by the token's personal account, accepts a setup token, and explains the required write scopes.
- **Existing repository recovery.** Connect Existing can attach an already-created local control repository to an existing SSH or HTTPS remote, including recovery after Forge API creation failed.

### Added - Project and ticket workspace
- **Unified Projects navigation.** The existing Projects/PM entry opens the Git-backed workspace when the module is enabled, eliminating the duplicate PM and Tickets screens while retaining the lightweight legacy view when the module is disabled.
- **Project registry and ticket board.** Create control projects and tickets, filter by project or text, switch between board and table views, and drag tickets through Inbox, Ready, In Progress, Review, Blocked, and Done.
- **Ticket details.** Edit type, priority, status, owner, description, and linked Vault documents with optimistic revision checks. Ticket creation, updates, status moves, and deletion remain individual Git commits.
- **Traceability and synchronization.** Ticket activity is loaded from append-only workflow events. The workspace shows branch/commit/ahead/behind state and can pull/rebase/push the private control repository, including the first push to an empty remote.
- **Vault handoff.** Ticket document references can open the requested work or personal Vault note directly in xNaut.
- **Existing-project and legacy migration.** The workspace automatically imports xNaut's current project registry with stable project keys and source paths. Legacy client scope/contact records are attached to matching projects, legacy todos become tickets, and unmatched todos are preserved under a Legacy Migration project. Original local stores remain untouched as a recovery copy.

## [1.8.12] - 2026-07-06

### Added — Markdown Vault
- **Vault Librarian workspace.** Opening the Vault now switches the far-right pane to Librarian Conversations, with conversation history and an in-pane **+** action for starting a new Librarian thread.
- **Manual notes from templates.** The Vault create panel can create notes from `Templates/*.md`, with title/date substitutions for reusable Concept, Business, Development, and future templates.
- **Direct Markdown import.** Readable Obsidian links and absolute `.md` paths can be imported deterministically into `_inbox/` without waiting for model-generated JSON.

### Fixed — Markdown Vault
- **Reliable note creation.** Explicit `vault_create` / `vault_write` JSON pasted into the Librarian executes directly, and agent-created notes refresh the visible Vault tree immediately instead of requiring an app restart.
- **Local-model action handling.** Qwen/LM Studio chat calls disable hidden reasoning for action requests, always end Qwen repair/follow-up prompts with a user query, cap chat output, and use a stream idle timeout so Vault actions do not hang indefinitely.
- **Settings consistency.** AI Settings save back into the Rust chat settings store so the Librarian uses the model selected in Settings.
- **Preview readability.** Vault Preview hides YAML frontmatter while preserving it in Edit mode, adds proper spacing before headings, and renders polished Markdown tables with headers, borders, striping, and hover feedback.
- **Vault document scrolling.** The note preview and editor have independent scroll containers so long documents remain usable in the right pane.

## [1.8.11] - 2026-07-05

### Fixed — Markdown Vault
- **Vault tree actions.** Replaced fragile menu dialogs with an inline action strip so note/folder actions stay visible and clickable.
- **Vault tree context menus.** Kept tree menus interactive while preserving drag/move behavior for organizing notes.
- **Vault controls.** Restored visible create and refresh controls in the Vault rail.

## [1.8.10] - 2026-07-03

### Added — Knowledge Graph ("the orb")
- **Vault graph.** ⋯ menu → **Knowledge Graph** opens a tab that scans a folder of `.md` notes, parses `[[wikilinks]]` into a force-directed graph, and renders it as an Obsidian-style orb (2D and 3D, slow auto-rotating nebula). Rust `graph_scan` walks every subfolder (skipping dotfiles, 6k cap); notes match by filename stem like Obsidian.
- **Code graph.** The **Vault / Code** selector switches the same pane to scan a codebase — files = nodes, **relative imports** (`./`, `../`, `#include "…"`) = edges (bare packages skipped) — colored **by file type** with a legend. Rust `code_scan`.
- **Cosmic styling.** Per-cluster coloring (union-find, golden-angle hues starting in blue so no red flood), a layered nebula-fog backdrop with a transparent canvas, a dim twinkling starfield of orphan/unlinked nodes, and gentle cyan "signal" particles travelling along backlinks.
- **Timeline build-up.** ▶ Timeline reveals notes/files in file-date (mtime) order so the universe assembles itself; a scrubber lets you scrub through it.

## [1.8.9] - 2026-06-25

### Fixed
- **App-crash (SIGABRT) while working in a terminal.** The app-wide debug-log trimmer sliced its buffer as a UTF-8 `String` at a raw byte offset; once the log passed 2 MB and the cut landed mid-character (terminal output contains emoji/multi-byte chars), it panicked — and with `panic = "abort"` that aborted the whole process. The trimmer now slices on bytes (safe at any offset). Caught via the crash report's WebKit→Rust `didPostMessage` backtrace.
- **Removed `panic = "abort"` from the release profile** — a panic in any `#[tauri::command]` is now caught and returned as an error instead of crashing the entire app. Defense-in-depth against this whole class of crash.

## [1.8.8] - 2026-06-25

### Added
- **Snippet Export / Import** in the Command Snippets panel. `↑ Export` writes a dated `xnaut-snippets-YYYY-MM-DD.json` (version, categories, snippets); `↓ Import` merges from a JSON file, skipping duplicates by id and unioning categories. The feature shipped on `main` (v1.5.1) but had never been merged into the tasks-mode branch — this brings it back.

## [1.8.7] - 2026-06-24

### Fixed
- **Modal Cancel/close actually closes.** The Project-details (PM intake) and Worktree modals styled their overlay `display:flex`, which overrode the `hidden` attribute — so Cancel/X/Escape set the flag but the modal stayed up (you had to quit the app). Added `[hidden] { display:none !important }` to both overlays.
- Removed an undeclared `workflows` reference in the `window.xnaut` debug export that threw a `ReferenceError` at the end of app.js load. (Surfaced by the new app-wide debug log.)

## [1.8.6] - 2026-06-24

### Fixed
- **New projects land in the right folder.** Creating a project via the **+** button now places it at `<project_root>/<Development category folder>/<name>` (e.g. `factory/02-Development/<name>`), matching the chat scaffold flow — instead of directly under the project root. "Open as project" still registers a folder where it already lives.
- **Removing projects works.** The sidebar "Remove from list" (right-click) no longer no-ops on the native `confirm()`; and **Internal** projects in the Projects panel now have a two-click "Remove from list". Both only drop the registry entry — the folder stays on disk.

## [1.8.5] - 2026-06-24

### Added
- **App-wide debug log** — every frontend `console.{log,info,warn,error}` plus uncaught errors and unhandled promise rejections are captured to `~/Library/Application Support/xnaut/debug.log` (size-capped). One readable file for diagnosing issues without opening DevTools; `xnautDebugLogPath()` / `xnautDebugLogClear()` hooks.

### Fixed
- **Browser address bar** — pressing Enter to navigate now `stopPropagation()`s so the keystroke isn't swallowed by global keyboard handlers; navigation errors surface on the input instead of being silently dropped.

## [1.8.4] - 2026-06-24

### Added — Unified Projects & per-project tasks
- **One project area.** The PM panel is now **"Projects"** and lists every project (from the Tasks registry), badged **Internal** (gray) or **Client** (mint). A project opened via "Open as project" lands here as Internal automatically — no separate list.
- **Per-project tasks/reminders** — add, check off, delete; backed by a central store (`project-todos.json`, keyed by task id). Available in two places sharing the same data: the **Projects detail** and the **right-pane Task List** (4th icon), which now has an add box and a Plan Mode shortcut.
- **Plan Mode for any project** — the chat + live `PLAN.md` workspace is reachable from every project, not just Client/PM ones.
- **"Move to external →"** button on Internal projects opens the intake (company/rate/contacts) and flips them to Client; "Remove from PM" reverts a Client project to Internal (tasks intact).
- **Projects top-bar icon** — a clipboard-check icon opens the Projects panel directly.

## [1.8.3] - 2026-06-24

### Added
- **Browser panes open local files** — the address bar now accepts `file://` URLs and bare absolute / `~/` paths (e.g. `~/…/preflight-report.html`), so local HTML (reports, docs) renders inside xNaut instead of only http(s).
- **Pre-Flight Check harness** (`scripts/preflight.mjs`, `just preflight`) — a standalone health/regression check that verifies build, tests, ACL command coverage, JS syntax, version consistency, CSP, and live services (CDN/LLM/Engram/Forge), then writes `preflight-report.html`.

## [1.8.2] - 2026-06-24

### Added
- **Open existing project** — right-click a folder in the right-pane tree → "Open as project" (registers it and switches to its workspace); the chat agent also gained an `open_project` action.
- **Drag a file/folder** from the right-pane tree onto a terminal to insert its path (folder single-click no longer auto-injects).
- **Opt+B / Opt+M** keybindings to split the focused terminal with a browser / markdown pane (the Cmd+Alt menu accelerators weren't firing through the webview).

### Fixed
- Forge Tasks 404 — `forge_list_issues`/`get_issue` now normalize the repo input (full clone URL, `owner/repo`, or `repo.git`) to a bare repo name before building the API path.

## [1.8.1] - 2026-06-24

### Added — Project workspaces, Plan Mode & a dependency-free Markdown stack

**Project-scoped tabs (Orca/CMUX model)** — every tab now belongs to a project workspace. Selecting a project card on the left shows only that project's tabs at the top; the global views (Tasks/Automations/PM/Search) live in a shared **Home** workspace. Clicking a project switches to its existing tabs (restoring the last-active one) and only creates a tab the first time — a terminal `cd`'d into the project folder, or an attach to its zellij session. The selected project card gets a mint highlight, the nav highlight clears while a project is active, and the status dot now means "has open tabs in this session."

**Plan Mode** — a two-pane planning workspace launched from a PM project ("Plan Mode" button): chat on the left (solution-architect persona, Engram-grounded), a live `PLAN.md` document on the right. The agent maintains the document only in the right pane (wrapped in `===PLAN DOCUMENT===` sentinels so embedded code/diagrams survive); the chat stays conversational. The doc pane has an **Edit/Preview** toggle and is fed back to the agent each turn so it extends the current content instead of restarting.

**Dependency-free Markdown** — the markdown editor and Plan doc no longer use the CDN TipTap editor (which fails to load in this WebKit). New shared renderer `markdown-render.js` (`window.xnautMarkdown`) powered by **marked** (UMD): GFM tables/task-lists, **raw-HTML passthrough**, **highlight.js** syntax highlighting, and **Mermaid** diagrams. Opening a `.md` file shows a rendered document with an Edit/Preview toggle and `Cmd+S` save.

**Quality-of-life**
- `Cmd +/-/0` font zoom — terminals *and* markdown docs (context-aware).
- Neon-mint pulsing-X "thinking" spinner, shown in chat while the model generates and on agent status pills.
- Persistent chat history (localStorage, keyed by project) surviving tab close + restart.
- Copy button on every chat message.
- Right-pane root picker — click the Files icon to switch the tree between Home, Project Root, and the current project.
- New-tab (+) moved to the far left of the tab bar as a mint circle.
- PM: create a project inline from the intake dropdown ("+ New project…"), dialog-free two-click "Remove from PM", briefcase icon for the PM nav row, dedup-by-name on create.
- New "Generate Plan" German Projektplan document template.

### Fixed
- LLM streaming aborted long generations after 60s — removed the overall request timeout on the streaming client (kept a 15s connect timeout); raised doc-generation to a 300s cap.
- Native `prompt()` / `confirm()` are no-ops in Tauri's WebKit — replaced the PM new-project and remove flows with inline UI.
- Stale project path after a folder move (right pane "Path does not exist") and manual projects opening a `~` shell instead of their folder.

## [1.8.0] - 2026-05-28

### Added — Orca + hunk port (Phases 1–8)

A multi-day port of features mined from two reference apps: [stablyai/orca](https://github.com/stablyai/orca) (the agent IDE) and [modem-dev/hunk](https://github.com/modem-dev/hunk) (the agent-aware diff viewer).

**Design system (Phase 1)** — Orca's paired token model (every `--background` ships with its `--foreground`), monochrome chrome with state-only color, dark + light themes via `data-theme` attribute, 3-elevation rule, agent state dot vocabulary, git decoration palette matching VS Code.

**Worktree-per-agent (Phase 2)** — `worktree.rs` with Orca's exact recipe (`--no-track` + `push.autoSetupRemote=true` in the new worktree, platform-aware path comparison, preflight clean check before non-force removal). Top-bar Worktrees button opens a manager with create / list / launch-agent / remove.

**Agent registry + launch (Phase 3)** — `agents.rs` with 5 prompt-injection strategies (`argv`, `flag-prompt`, `flag-prompt-interactive`, `flag-interactive`, `stdin-after-start`) covering every coding CLI's quirk. User-editable TOML at `~/.config/xnaut/agents.toml` auto-seeded with claude, codex, gemini, grok, opencode. Stubbed `preflight_trust` for cursor/copilot/codex.

**Agent status overlay (Phase 4)** — `status.rs` tracks per-session state using Orca's vocabulary (`working / blocked / waiting / done` + UI-only `idle / permission / interrupted`). Top-bar pill strip with the literal Orca rendering rules (yellow spinner / emerald check / red filled / gray-40%). Output-silence decay: 2s of PTY silence → idle; 30 min stale → dropped. Click a pill to jump to the agent's tab.

**Agent hook listener (Phase 5)** — `agent_hooks.rs` runs an axum HTTP listener on `127.0.0.1:<random>` with 1 MB body cap, 5s timeout, per-session bearer tokens. Spawned agents receive `XNAUT_HOOK_URL` + `XNAUT_HOOK_TOKEN` env vars so their hook scripts can push state changes. Infrastructure-only — per-agent hook script writers deferred.

**Browser panes (Phase 6)** — `browser.rs` uses Tauri 2's child-webview API (`unstable` feature) to float native webviews over a DOM placeholder. Address bar with back/forward/reload/URL, drag-drop support, sandboxed (no Tauri API injected). Three menu items: New Browser Tab button, Split → Browser (`Cmd+Alt+B`). Includes the macOS chrome-offset Y fix for child webview positioning.

**Markdown editor (Phase 7)** — TipTap 2.10 lazy-loaded from jsdelivr ESM. Extensions: starter-kit, image, link, task-list (nested), table (resizable), placeholder, `tiptap-markdown` for round-trip markdown ↔ HTML. File open/save via existing Tauri commands, image paste + drag-drop, bubble toolbar on selection with bold/italic/strike/code/H1–H3/lists/blockquote/link. Top-bar Markdown button and Split → Markdown (`Cmd+Alt+M`).

**Diff viewer with inline annotations (Phase 8)** — the killer feature ported from hunk:
- `diff.rs` parses `git diff HEAD` (or `show`/`against-ref`) into structured JSON (file → hunks → lines with side-aware numbering)
- `notes.rs` reads/writes `<worktree>/.xnaut/notes.json` in hunk's `agent-context` shape: `{ version, summary, files: [{ path, annotations: [{ oldRange, newRange, summary, rationale, tags, confidence, source, author, createdAt }] }] }`. Range-on-both-sides anchoring renders correctly in split or unified
- `notify` crate watches the file; changes emit `notes-changed` and the pane re-renders in <100ms
- `agent_notes_broker.rs` extends Phase 5's listener with hunk's 11-verb vocabulary on `/v1/notes`: `list / get / review / comment-add / comment-apply / comment-list / comment-rm / comment-clear`. `reveal:true` emits a `diff-reveal` event so the viewer scrolls to the new note
- Three-dock render pattern from hunk: split-view notes dock right (new-side) or left (old-side); notes that don't match any hunk surface as a file-level group at the top of the file section
- `skills/xnaut-review/SKILL.md` instructs external agents on the action vocabulary, payload shapes, and the "don't comment on every hunk" / "navigate before commenting" soft rules
- `skill_path` / `skill_list` commands (mirror of `hunk skill path`) so agents can locate the bundled skill files

### Fixed
- Tauri 2 ACL: every command added in Phases 2–8 needed an entry in `permissions/default.toml` plus the `allow-all-commands` set. Without these, custom commands returned "Command not found" at runtime even when registered in `invoke_handler!`. Documented in the `xnaut-tauri-acl-recipe` memory.
- Tauri child webview Y coords on macOS counted from NSWindow top (including title bar), so address bars were painted over. `getChromeOffsetY()` measures `outerHeight − innerHeight` and adds it to viewport coords; fallback constant 28.
- Cmd+W now closes the active tab (was: closed the whole window because the default Window menu intercepted it). Cmd+Shift+W closes the window. Cmd+D / Cmd+Shift+D split right/down via View-menu items with `CmdOrCtrl` accelerators.
- Removed `transparent: true` from window config — without `macOSPrivateApi` it produced invisible / black / white windows in dev.

## [1.7.0] - 2026-06-13

### Added — PM Space
- PM section (sidebar): external client projects alongside internal ones
- Project intake with Plow (lead tool) opportunity picker — client, contacts, value pulled from the lead, never retyped
- Financials computed from the Merkle worklog: hours, burn (hours x project rate), margin vs offer
- Per-project rate (CHF/h) set at intake
- Client document generation from editable German templates (Offerte, SLA, Architektur, Meeting-Notes) via the configured LLM, written to <project>/client/
- Project-scoped chat: chat panel grounded in the project's intake data

## [1.6.0] - 2026-06-12

### Added — Tasks Mode
- Native Chat panel wired to any OpenAI-compatible endpoint (LM Studio, Ollama, NautGate, cloud) with streaming, Engram (Brain) memory grounding, and chat-driven project/task scaffolding
- Project creation end-to-end: folder under the configured factory root, git init, repo on Forgejo/GitHub/GitLab, baseline prompt to CLAUDE.md/AGENTS.md, agent launched in a named Zellij session via NautGate
- Forge Tasks panel: browse issues/PRs on Forgejo/GitHub/GitLab, one-keystroke "Start" into a Create Worktree modal (issue context preloaded for the agent)
- Automations: scheduled agent runs with precheck command, grace window, fresh/reuse sessions
- Project pane (right): Files tree, ripgrep search (git-grep fallback), full Git source control (outgoing commits, AI commit messages, push split-button with Create PR, side-by-side diffs)
- Projects sidebar (left) with pinning and plan-usage strip
- Settings: new "Tasks Mode" section (LLM endpoint with save-and-test, Engram toggle, project root and categories, forge hosts)
- Pi joins the agent registry; agents launch with NautGate routing env

### Removed
- Legacy file-navigator tree (superseded by the project pane Files view)

## [1.5.0] - 2026-04-18

### Added

**Work Session Logger**
- Record terminal commands with timestamps and duration tracking
- SHA-256 Merkle tree hash chain for tamper-evident proof
- QR code verification (scan to verify work is authentic)
- Professional HTML/PDF reports with tool usage summary
- Tool detection: groups Besen, AntBot, Claude Code, Docker, Terraform, etc.
- Duration per command and per tool

**AI Explainer**
- "Explain Screen" in 3-dot menu — AI reads terminal output and explains what's happening
- Uses AntBot (local-first) with fallback to configured provider

**AI Theme Generator**
- Describe a vibe (e.g., "ocean blue", "cyberpunk neon") and AI creates a matching color theme
- Mini terminal preview showing generated colors
- Auto-saves to custom themes

**Privacy Monitor (ClawProxy Integration)**
- Transparent LLM API proxy integration
- Detects leaked API keys, credentials, PII in prompts
- Real-time privacy indicator in status bar (green/yellow/red)
- Privacy panel with API call stats, cost, and alert details
- Routes AI traffic through ClawProxy when available

**AntBot Auto-Start**
- Settings toggle to auto-start AntBot gateway on xNAUT launch
- "Start Now" button in Settings > AI

**Cross-Platform**
- Windows x64 support (.msi and .exe installers)
- macOS Intel support
- Platform-specific directory tracking (lsof/proc/PowerShell)

**Theme System v2**
- 5 curated default themes (Jellybeans, Default Dark, Dracula, Solarized Light, Monokai)
- Separated sections: Default, AI Generated, Imported (with delete on custom)
- Import from Warp YAML and JSON theme files
- Full app theming — editor, chrome, borders all follow theme
- Color pickers with live preview

**Bundled Nerd Fonts**
- JetBrains Mono NF, Fira Code NF, Cascadia Code NF, Source Code Pro NF
- Ligature toggle in Settings

**UI Improvements**
- Binary tree split panes (proper close + resize)
- Clean 3-icon top bar (sidebar toggle, new tab, 3-dot menu)
- Redesigned command snippets (compact cards, favorites, search, A-Z index, explain)
- File browser position toggle (left/right)
- Sidebar toggle icon (SVG, Warp-style)

**Auto-Update**
- Tauri updater plugin with signed releases
- GitHub Actions release workflow (macOS + Windows)
- Blue banner notification with one-click update

### Fixed
- Split pane close now uses binary tree collapse (siblings promote correctly)
- Directory tracking via lsof (no more shell hook flashing)
- ACL permissions for all new commands
- WebView CSP — AI calls routed through Rust backend
- Theme generator handles AntBot's line-wrapped JSON responses

---

## [1.3.0] - 2026-04-15

### Added

**Warp-Style Settings Panel**
- Full-screen settings page (Cmd+,) with nav menu and search
- AI section: configure local providers (Ollama, LM Studio, AntBot) and cloud providers (Anthropic, OpenAI, OpenRouter, Perplexity)
- Model auto-detection: fetches available models from local LLM endpoints
- Appearance section: 12 built-in themes with full ANSI palettes and live preview
- Keyboard Shortcuts section: click-to-rebind with conflict detection
- Nautify section: shell and SSH configuration
- Triggers section: pattern matching on terminal output

**Built-in File Editor**
- Click files in navigator to open in syntax-highlighted editor panel
- Syntax highlighting via highlight.js for 25+ languages
- Line numbers with scroll sync in both view and edit modes
- Markdown preview mode with rendered HTML
- Save with Cmd+S, unsaved changes indicator, close confirmation
- Toggle between Edit and Preview modes

**Warp-Style File Navigator**
- Left-side tree view with expand/collapse folders
- Lazy-loaded directory contents
- File type icons and search filter
- Right-click context menu: Send to Terminal, Open in Editor, Copy Path, cd into folder
- Current directory shown in status bar footer

**AI Integration**
- AntBot integration: local-first AI agent via CLI (no cloud needed)
- Direct Ollama and LM Studio chat routing (no backend proxy)
- Model selection per provider in Settings

**Terminal Improvements**
- URL detection: clickable links in terminal output (addon-web-links)
- Split panes: up to 16 panes, iTerm2-style with hover close buttons
- Autocomplete: history-based command suggestions (Tab to accept)
- Shell integration: OSC 133 prompt detection foundation
- Directory tracking: status bar shows current CWD and git branch (via lsof)
- Startup banner: shows on first terminal, clears after 3 seconds

**Native macOS Integration**
- Native menu bar: About xNAUT, Edit (Cmd+C/V/X), View, Window
- Cmd+, opens Settings (standard macOS shortcut)
- About dialog with version, copyright, GitHub link

**Infrastructure**
- CI/CD pipeline: reusable GitHub Actions workflows (clippy, fmt, test, audit)
- Organization-wide workflow templates at 48Nauts-Operator/ci-workflows

### Changed
- Removed AI chat panel (redundant with terminal-based AI tools like AntBot, Claude Code)
- Removed LLM dropdown from status bar (moved to Settings > AI)
- Cleaned up status bar: only File Navigator, Error Monitor, Snippets, Ralph, SSH icons
- Split pane close button uses hover-to-show (no per-pane headers)

### Fixed
- ACL permissions for read_file, write_file, check_antbot, ask_antbot commands
- File path insertion targets focused pane in split view
- Context menu click handlers (mouseup instead of onclick to prevent race)
- Directory tracking updates shared status bar directly
- Shell hooks injected with 2-second delay to prevent startup interference
- Multiple null reference crashes from removed UI elements
- CSS grid overflow on split panes (min-height:0 cascade)
- Pane refit after close with multiple passes

---

## [1.2.0] - 2026-03-23

### Added — Ralph Ultra Integration (Phase 1-3)

**Backend (Rust)**
- `ralph.rs` module with 12 new Tauri commands:
  - PRD management: `ralph_read_prd`, `ralph_write_prd`, `ralph_backup_prd`, `ralph_list_backups`, `ralph_restore_backup`
  - CLI detection: `ralph_detect_clis`, `ralph_check_cli_health`
  - AC test execution: `ralph_run_ac_test`
  - Config persistence: `ralph_read_config`, `ralph_write_config`
  - Temp file management: `ralph_write_temp_file`, `ralph_cleanup_temp_file`
- `create_command_session` command for spawning AI CLIs in PTY (non-interactive program execution)
- Exit code capture on PTY close — `terminal-closed:{id}` events now include `exitCode` field

**Frontend (JavaScript)**
- Ralph orchestrator engine (8 ES modules in `src/js/ralph/`):
  - Task type detection (14 categories via keyword scoring)
  - Model capability matrix with 3 execution modes (balanced, super-saver, fast-delivery)
  - Cost tracking with estimated vs actual, persisted to disk
  - Learning recorder with weighted scoring (reliability 40%, efficiency 35%, speed 25%)
  - Execution planner with mode comparisons
  - Prompt builder for Claude, Aider, and Codex CLIs
  - Main orchestrator: load project, detect CLIs, plan, execute stories, test ACs, retry/advance
- Ralph UI panel with project path input, execution mode selector, run/pause/stop/test controls, story list, cost summary, and live log monitor
- Tauri bridge module for ES module access to `invoke`/`listen`

**UI**
- Ralph panel toggle via robot icon in status bar or `Ctrl+Shift+R` keyboard shortcut
- `ralph.css` stylesheet matching xNAUT dark design tokens

### Fixed

- **Split pane exit handling** — typing `exit` in a split pane now properly closes that pane and re-layouts remaining panes (was going stale/unresponsive)
- **Snippet copy/run with quotes** — commands containing double quotes (e.g., `gcloud compute ssh --zone "europe-west6-a"`) no longer get truncated at the first quote when using copy or run buttons
- **Claude Code nested session error** — PTY now strips `CLAUDECODE` environment variable so Claude Code can run inside xNAUT without false nested-session detection
- **PtyConfig deserialization** — added `#[serde(default)]` so `cols`/`rows` fields default to 80x24 when not provided by frontend

### Changed

- Split pane limit increased from 4 to 6 — new 5-pane (3+2) and 6-pane (3x2 grid) layouts
- Version bumped to 1.2.0 across `Cargo.toml`, `tauri.conf.json`, and UI badge

## [1.1.0] - 2025-10-06

### Initial Release

- Multiple PTY sessions with tab management
- Split pane support (up to 4 panes with vertical/horizontal splits)
- SSH connection support with config file integration
- AI chat integration (Anthropic, OpenAI, OpenRouter, Perplexity)
- Workflow recording and playback
- Smart triggers and notifications
- Session sharing capabilities
- xterm.js v5.5.0 terminal rendering
- Tauri v2 ACL security model with proper permissions
- macOS native app bundle (~8MB binary)
