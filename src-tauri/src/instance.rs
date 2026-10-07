// What THIS copy of xNAUT is for, and who it is, on a desk with several of them.
//
// André, 2026-09-13: two bugs in one day came out of the same blind spot. The
// Studio and tron were running different builds of the same app against the
// same board, and nothing in the record said which of them had done a thing.
// A ledger line read "sweep_dispatch nautbot XNAUT-358 launched claude" and was
// equally true on both machines, so the drift was invisible until an agent
// stalled on a key prompt on the owner's desk.
//
// The fix has two halves and this module is the first: a machine states what it
// is FOR, and every record it writes is signed with who wrote it and which
// build did the writing.
//
// ---- the role, and why it replaces a boolean --------------------------------
//
// `loops.dispatch_here` (XNAUT-358) answered exactly one question: may this
// machine start agents. It was the right answer to the wrong question, because
// the machines differ in more than one way. tron dispatches AND verifies; the
// Studio is where the owner plans and reviews and must never start local workers;
// a headless box exists to verify and nothing else. One boolean cannot say
// that, and three booleans would be three chances to configure a contradiction.
//
// So: one role, three values, and the capability questions are METHODS on it
// rather than separate switches. A role that cannot be read is a workstation,
// which is the conservative direction — a machine whose purpose is unclear must
// not start agents on somebody's desk. The ticket keeps waiting and the ledger
// says why, which is a visible failure rather than a dangerous one.
// Approved remote swarms are a distinct case: dispatch::approved_dispatch_policy
// permits the workstation to coordinate them without changing its automatic role.
//
// The old key stays honoured and stays on disk for one release (see
// `adopt_role`), so a settings file written here still reads correctly in a
// build that predates this module.
//
// ---- the id, and why it is not the hostname ---------------------------------
//
// XNAUT-369 sets the rule the realm will need: an instance is a KEY, never a
// hostname. Hostnames are reassigned, duplicated across networks and edited by
// people; a machine that is renamed is still the same instance, and two
// machines called `mac-mini.local` on different networks are not. The id is
// minted once, on first start, and saved. `stamp().machine` still carries the
// hostname because it is the thing a human recognises in a panel — but nothing
// joins on it.

use serde::{Deserialize, Serialize};

/// What this machine is for. The names are the owner's (XNAUT-370) and the
/// capabilities below are the whole of what the role decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Dispatches and verifies. The fleet: tron, and any box whose job is to
    /// run the work rather than to be looked at.
    Fleet,
    /// Plans and reviews; never picks up tickets or starts local swarm workers.
    /// Explicitly approved remote groups use dispatch::approved_dispatch_policy.
    /// The owner's desk. It does not
    /// verify either: a verification is a run, and the reason this role exists
    /// is that runs must not start here.
    Workstation,
    /// Verifies only. A headless box that warms sandboxes and checks handbacks,
    /// with no board of its own to dispatch from.
    Sandbox,
}

impl Role {
    /// May this machine autonomously pick up tickets? Only the fleet.
    /// Approved remote groups have a separate destination-aware policy.
    pub fn dispatches(self) -> bool {
        matches!(self, Self::Fleet)
    }

    /// May this machine start verifications? The fleet and a sandbox box.
    pub fn verifies(self) -> bool {
        matches!(self, Self::Fleet | Self::Sandbox)
    }

    /// May this machine's SWEEP take issues in from a forge (XNAUT-382)? Only
    /// the fleet.
    ///
    /// Not because intake is expensive, but because it writes NEW tickets to a
    /// board that is shared between machines. Its "never twice" guard is a
    /// `source_id` already on that board, and each machine only pushes after
    /// it writes, so two sweeps ticking at the same moment both see no ticket
    /// and both file one. One machine doing this unattended is the whole
    /// point of the fleet role.
    ///
    /// This gates the TICK and not `issue_intake_run_now`: a person pressing
    /// Run now on their own desk is attended, deliberate, and is how a
    /// workstation is meant to pull an issue in.
    pub fn files_issues(self) -> bool {
        matches!(self, Self::Fleet)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fleet => "fleet",
            Self::Workstation => "workstation",
            Self::Sandbox => "sandbox",
        }
    }

    /// The role this string names, or `None` when it names none of them.
    ///
    /// `None` is deliberately distinct from a default: the caller has to decide
    /// what an unreadable role means, and every caller here decides the same
    /// conservative way. See `resolve`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fleet" => Some(Self::Fleet),
            "workstation" => Some(Self::Workstation),
            "sandbox" => Some(Self::Sandbox),
            _ => None,
        }
    }
}

/// Who wrote a record, what that machine is for, and which build wrote it.
///
/// Carried on every ledger line and every run manifest. Three small strings on
/// records that already hold ten is a price worth paying for being able to ask
/// "which machine did this, on which build" of the record itself rather than of
/// somebody's memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// The minted instance key, or "" when this machine has not started the app
    /// yet. Empty means UNKNOWN, the same way `ledger::Entry::session` does.
    pub id: String,
    pub role: String,
    pub version: String,
    /// The hostname, for a human reading a panel. Nothing joins on it.
    pub machine: String,
}

/// The role this process is running under, reading settings fresh.
///
/// Fresh rather than cached on purpose: the owner may change the role in
/// settings while the app is up, and a gate that answers from a value read at
/// startup would keep dispatching for the rest of the day after being told to
/// stop. The read is a small JSON file, and every caller here is already on a
/// path that reads settings (the sweep does it once a tick).
pub fn role() -> Role {
    resolve(&crate::settings::load_or_default())
}

/// The role a given settings object names.
///
/// Three cases, and the middle one is the interesting one:
///
///   * a role that parses — that role, whatever it says;
///   * an EMPTY role — `Fleet`, because that is what `dispatch_here: true`
///     meant and it is the default every machine has had until now. A fresh
///     install must behave exactly as it did yesterday;
///   * a role that does not parse — `Workstation`. Written by a newer build, or
///     typed by hand and misspelled; either way this process cannot say what
///     the machine is for, and the safe answer to that is "do not start
///     anything here".
pub fn resolve(settings: &crate::settings::Settings) -> Role {
    if let Ok(from_env) = std::env::var("XNAUT_INSTANCE_ROLE") {
        if !from_env.trim().is_empty() {
            return Role::parse(&from_env).unwrap_or(Role::Workstation);
        }
    }
    let configured = settings.instance.role.trim();
    if configured.is_empty() {
        return Role::Fleet;
    }
    Role::parse(configured).unwrap_or(Role::Workstation)
}

/// The signature to put on a record written right now.
///
/// The env overrides are not test scaffolding: a sandbox instance is configured
/// by its environment and has no settings file to edit, which is the same
/// reason `ledger::path` reads `XNAUT_LEDGER_PATH`. They do also let a test
/// assert on a stamp without writing the owner's settings.json.
pub fn stamp() -> Stamp {
    let settings = crate::settings::load_or_default();
    let id = match std::env::var("XNAUT_INSTANCE_ID") {
        Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => settings.instance.id.trim().to_string(),
    };
    Stamp {
        id,
        role: resolve(&settings).as_str().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        machine: crate::run_control::hostname(),
    }
}

/// Bring a settings object up to date IN MEMORY: mint nothing, write nothing,
/// just carry `loops.dispatch_here` across to `instance.role` so every reader
/// of this object sees the right role from the first load.
///
/// Returns whether anything changed, so the one caller that is allowed to
/// persist (`adopt`, at app start) knows there is something to save.
///
/// The old key is READ and left in place. It is an alias for one release: a
/// settings file written by this build still reads correctly in the build
/// before it, which matters on a desk where two machines run different
/// versions against the same board — which is the exact drift this ticket
/// exists to make visible.
pub fn adopt_role(settings: &mut crate::settings::Settings) -> bool {
    if !settings.instance.role.trim().is_empty() {
        return false;
    }
    let Some(dispatch_here) = settings.loops.dispatch_here else {
        return false;
    };
    settings.instance.role = if dispatch_here { Role::Fleet } else { Role::Workstation }.as_str().to_string();
    true
}

/// Mint this instance's id if it has none, adopt the legacy role key, and save.
///
/// Called ONCE, from the app's setup. Deliberately not from `load_or_default`:
/// that function is called from tests and from short-lived tool processes, and
/// a migration that writes the owner's settings.json as a side effect of
/// reading it is how one version's save strips another's keys.
pub fn adopt() {
    let mut settings = crate::settings::load_or_default();
    let mut changed = adopt_role(&mut settings);
    if settings.instance.id.trim().is_empty() {
        settings.instance.id = uuid::Uuid::new_v4().to_string();
        changed = true;
    }
    if !changed {
        return;
    }
    if let Err(error) = crate::settings::save(&settings) {
        eprintln!("[instance] could not persist the instance id/role: {error}");
    }
}

/// This instance, for the Observatory's header badge (XNAUT-391).
#[tauri::command]
pub fn instance_stamp() -> Stamp {
    stamp()
}

/// `XNAUT_INSTANCE_ID` and `XNAUT_INSTANCE_ROLE` are process-global, so every
/// test that sets either queues behind this one mutex rather than reading a
/// sibling's value mid-assertion. Shared with `ledger.rs`, which stamps from
/// them — two locks would have let exactly the cross-talk they exist to stop.
#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(role: &str, dispatch_here: Option<bool>) -> crate::settings::Settings {
        let mut settings = crate::settings::Settings::default();
        settings.instance.role = role.into();
        settings.loops.dispatch_here = dispatch_here;
        settings
    }

    #[test]
    fn the_role_decides_dispatch_and_verification_and_nothing_else_does() {
        // The whole point of one role instead of three booleans: the capability
        // table is stated once, here, and cannot be configured into a
        // contradiction.
        assert!(Role::Fleet.dispatches() && Role::Fleet.verifies());
        assert!(!Role::Workstation.dispatches(), "the owner's desk never picks up unattended tickets");
        assert!(!Role::Workstation.verifies(), "a verification is a run too");
        assert!(!Role::Sandbox.dispatches(), "a sandbox box has no board to dispatch from");
        assert!(Role::Sandbox.verifies());
        // Issue intake (XNAUT-382) writes NEW tickets to a shared board, and
        // its never-twice guard is a source_id already on that board. Two
        // sweeps ticking at once would both see none and both file.
        assert!(Role::Fleet.files_issues());
        assert!(
            !Role::Workstation.files_issues(),
            "a second machine filing the same issue is a duplicate ticket"
        );
        assert!(
            !Role::Sandbox.files_issues(),
            "a sandbox box verifies; it does not put work on the board"
        );
    }

    #[test]
    fn a_machine_that_never_configured_anything_still_dispatches() {
        let _guard = env_lock();
        std::env::remove_var("XNAUT_INSTANCE_ROLE");
        // `dispatch_here` defaulted to true and every machine has had that
        // default until today. A fresh install must behave exactly as it did
        // yesterday, or this ticket silently stops the fleet.
        assert_eq!(resolve(&settings_with("", None)), Role::Fleet);
    }

    #[test]
    fn the_old_boolean_is_read_as_a_role_and_left_on_disk() {
        let _guard = env_lock();
        std::env::remove_var("XNAUT_INSTANCE_ROLE");
        let mut off = settings_with("", Some(false));
        assert!(adopt_role(&mut off), "there was something to migrate");
        assert_eq!(off.instance.role, "workstation");
        assert_eq!(
            off.loops.dispatch_here,
            Some(false),
            "the alias stays on disk for one release, so the previous build still reads it"
        );

        let mut on = settings_with("", Some(true));
        assert!(adopt_role(&mut on));
        assert_eq!(on.instance.role, "fleet");
    }

    #[test]
    fn an_explicit_role_wins_over_the_old_boolean_and_migration_is_idempotent() {
        let _guard = env_lock();
        std::env::remove_var("XNAUT_INSTANCE_ROLE");
        // Someone who has set a role has said the newer, richer thing. A stale
        // `dispatch_here: true` left beside it must not drag them back to
        // fleet, or the migration would undo the owner's own choice on every
        // load.
        let mut settings = settings_with("workstation", Some(true));
        assert!(!adopt_role(&mut settings), "nothing to migrate over an explicit role");
        assert_eq!(resolve(&settings), Role::Workstation);
    }

    #[test]
    fn a_role_this_build_cannot_read_does_not_dispatch() {
        let _guard = env_lock();
        std::env::remove_var("XNAUT_INSTANCE_ROLE");
        // Written by a newer build, or misspelled by hand. This process cannot
        // say what the machine is for, and the safe answer to that is "start
        // nothing here": the ticket waits and the ledger says why, which is a
        // visible failure instead of agents launching on the owner's desk.
        let unknown = resolve(&settings_with("courier", None));
        assert_eq!(unknown, Role::Workstation);
        assert!(!unknown.dispatches());
    }

    #[test]
    fn the_environment_configures_a_box_that_has_no_settings_file() {
        let _guard = env_lock();
        std::env::set_var("XNAUT_INSTANCE_ROLE", "sandbox");
        // A settings file saying fleet loses to the environment: the env var is
        // how a headless instance is configured, and it is the more specific
        // statement about what this particular process is.
        assert_eq!(resolve(&settings_with("fleet", None)), Role::Sandbox);
        std::env::set_var("XNAUT_INSTANCE_ROLE", "nonsense");
        assert_eq!(resolve(&settings_with("fleet", None)), Role::Workstation);
        std::env::remove_var("XNAUT_INSTANCE_ROLE");
    }

    #[test]
    fn a_stamp_always_names_the_build_that_wrote_it() {
        let _guard = env_lock();
        std::env::remove_var("XNAUT_INSTANCE_ROLE");
        std::env::set_var("XNAUT_INSTANCE_ID", "inst-test");
        let stamp = stamp();
        assert_eq!(stamp.id, "inst-test");
        assert_eq!(stamp.version, env!("CARGO_PKG_VERSION"));
        assert!(
            Role::parse(&stamp.role).is_some(),
            "a stamp never carries a role nothing can read: {stamp:?}"
        );
        std::env::remove_var("XNAUT_INSTANCE_ID");
    }
}
