// Build slices as a dependency graph.
//
// The planner used to be instructed to avoid dependencies entirely — "split only
// when slices are genuinely independent" — because the runner had no way to
// express "B needs A first". That capped parallelism at whatever happened to be
// independent, and forced sequential work into one oversized slice.
//
// Two ideas, from two unrelated projects that each had only half the answer:
//
//   EXECUTION, from lamalab-org/corral (BSD 3-Clause), `src/corral/episode.py`:
//     a node waits for its parents; if any parent did not succeed the node is
//     recorded UNREACHABLE and never runs. The cascade is free, because
//     "unreachable" is itself a not-succeeded state, so a grandchild is caught
//     by the same rule on the next pass with no traversal.
//
//   VALIDATION, from cdknorow/coral (Apache 2.0), `internal/board/store.go`:
//     dependencies are checked when they are DECLARED — every blocker must
//     exist, cycles are rejected, depth is capped. Our first design detected
//     deadlock at run time instead, which is strictly worse: the plan is already
//     wrong the moment it is written, and by run time worktrees exist and an
//     afternoon is gone.
//
// A language model writes these plans, so a cycle is not a hypothetical edge
// case. `dag_step` keeps a deadlock check anyway, as a backstop — cheap, and it
// degrades to a clear report rather than a build that hangs looking healthy.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A slice, as the guardian sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// Stable identity. The branch name, in practice.
    pub id: String,
    #[serde(default)]
    pub depends: Vec<String>,
    /// waiting | running | done | failed | unreachable | cancelled
    pub status: String,
}

/// What to do this tick.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct DagStep {
    /// Waiting nodes whose every dependency is done — launch these.
    pub ready: Vec<String>,
    /// Waiting nodes with a dependency that failed, was cancelled, or is itself
    /// unreachable. Poison them; the cascade takes care of their children.
    pub unreachable: Vec<String>,
    /// Waiting nodes that can never become ready. Only reachable if validation
    /// was skipped or bypassed — without this the build would hang forever
    /// while every slice reported "waiting".
    pub deadlocked: Vec<String>,
}

/// Statuses that permanently prevent a dependent from running.
fn is_poison(status: &str) -> bool {
    matches!(status, "failed" | "unreachable" | "cancelled")
}

/// Resolves one tick. Pure: the caller applies the result.
///
/// Unknown dependency ids are treated as SATISFIED, not unsatisfiable. A
/// planner naming a branch that is not in the plan is a mistake we want to hear
/// about at validation time, but at run time refusing to launch would strand
/// real work forever, whereas ignoring it degrades to exactly the old
/// behaviour — everything starts at once.
#[tauri::command]
pub fn dag_step(nodes: Vec<DagNode>) -> DagStep {
    let by_id: HashMap<&str, &DagNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out = DagStep::default();

    for node in nodes.iter().filter(|n| n.status == "waiting") {
        let deps: Vec<&DagNode> = node
            .depends
            .iter()
            .filter_map(|d| by_id.get(d.as_str()).copied())
            .collect();

        if deps.iter().any(|d| is_poison(&d.status)) {
            out.unreachable.push(node.id.clone());
            continue;
        }
        if deps.iter().all(|d| d.status == "done") {
            out.ready.push(node.id.clone());
        }
    }

    // Backstop: a waiting node that is neither ready nor unreachable is only
    // fine if it is genuinely still waiting on live work. If nothing upstream
    // can ever finish, say so instead of waiting forever.
    let decided: HashSet<&str> = out
        .ready
        .iter()
        .chain(out.unreachable.iter())
        .map(String::as_str)
        .collect();
    for node in nodes.iter().filter(|n| n.status == "waiting") {
        if decided.contains(node.id.as_str()) {
            continue;
        }
        if !can_ever_run(&node.id, &by_id, &mut HashSet::new()) {
            out.deadlocked.push(node.id.clone());
        }
    }
    out
}

/// True if some path exists by which this node could eventually become ready.
/// A cycle returns false: every member is waiting on another member.
fn can_ever_run(id: &str, by_id: &HashMap<&str, &DagNode>, seen: &mut HashSet<String>) -> bool {
    if !seen.insert(id.to_string()) {
        return false; // back to a node already on this path — a cycle
    }
    let Some(node) = by_id.get(id) else {
        return true; // unknown id: satisfied, per the rule above
    };
    if is_poison(&node.status) {
        return false;
    }
    if node.status != "waiting" {
        return true; // running or done — progress is possible
    }
    let ok = node
        .depends
        .iter()
        .all(|d| can_ever_run(d, by_id, &mut seen.clone()));
    seen.remove(id);
    ok
}

/// One problem with a plan, found before anything runs.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagIssue {
    pub node: String,
    /// unknown_dependency | cycle | too_deep | self_dependency
    pub kind: String,
    pub detail: String,
}

/// Validates a plan at the moment it is written. Empty result means it is safe
/// to run.
///
/// This is the half our own design was missing. Detecting a bad graph at run
/// time tells you after worktrees exist; detecting it here tells you while the
/// only cost is regenerating a plan.
#[tauri::command]
pub fn dag_validate(nodes: Vec<DagNode>, max_depth: Option<usize>) -> Vec<DagIssue> {
    let max_depth = max_depth.unwrap_or(5);
    let by_id: HashMap<&str, &DagNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut issues = Vec::new();

    for node in &nodes {
        for dep in &node.depends {
            if dep == &node.id {
                issues.push(DagIssue {
                    node: node.id.clone(),
                    kind: "self_dependency".into(),
                    detail: format!("\"{}\" depends on itself", node.id),
                });
            } else if !by_id.contains_key(dep.as_str()) {
                issues.push(DagIssue {
                    node: node.id.clone(),
                    kind: "unknown_dependency".into(),
                    detail: format!("\"{}\" depends on \"{dep}\", which is not in this plan", node.id),
                });
            }
        }
    }

    for node in &nodes {
        if let Some(path) = find_cycle(&node.id, &by_id, &mut Vec::new()) {
            // One issue per cycle, reported on its lowest-sorting member, so a
            // three-node loop does not produce three copies of the same finding.
            let mut members: Vec<&str> = path.iter().map(String::as_str).collect();
            members.sort_unstable();
            if members.first() == Some(&node.id.as_str()) {
                issues.push(DagIssue {
                    node: node.id.clone(),
                    kind: "cycle".into(),
                    detail: format!("dependency cycle: {}", path.join(" → ")),
                });
            }
            continue;
        }
        let depth = depth_of(&node.id, &by_id, &mut HashSet::new());
        if depth > max_depth {
            issues.push(DagIssue {
                node: node.id.clone(),
                kind: "too_deep".into(),
                detail: format!("dependency chain is {depth} deep (limit {max_depth})"),
            });
        }
    }
    issues
}

/// Returns the cycle containing `id`, if there is one.
fn find_cycle(id: &str, by_id: &HashMap<&str, &DagNode>, path: &mut Vec<String>) -> Option<Vec<String>> {
    if let Some(pos) = path.iter().position(|p| p == id) {
        let mut cycle = path[pos..].to_vec();
        cycle.push(id.to_string());
        return Some(cycle);
    }
    let node = by_id.get(id)?;
    path.push(id.to_string());
    for dep in &node.depends {
        if let Some(c) = find_cycle(dep, by_id, path) {
            return Some(c);
        }
    }
    path.pop();
    None
}

/// Longest chain of dependencies above this node. Cycle-safe via `seen`.
fn depth_of(id: &str, by_id: &HashMap<&str, &DagNode>, seen: &mut HashSet<String>) -> usize {
    if !seen.insert(id.to_string()) {
        return 0;
    }
    let Some(node) = by_id.get(id) else {
        return 0;
    };
    let deepest = node
        .depends
        .iter()
        .map(|d| depth_of(d, by_id, &mut seen.clone()))
        .max()
        .unwrap_or(0);
    if node.depends.is_empty() {
        0
    } else {
        deepest + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str, depends: &[&str], status: &str) -> DagNode {
        DagNode {
            id: id.into(),
            depends: depends.iter().map(|s| s.to_string()).collect(),
            status: status.into(),
        }
    }

    // ---- the regression bar -------------------------------------------------

    #[test]
    fn a_plan_with_no_dependencies_launches_everything_at_once() {
        // Existing behaviour must be untouched: this is what every plan looks
        // like until the planner starts emitting `depends`.
        let step = dag_step(vec![
            n("a", &[], "waiting"),
            n("b", &[], "waiting"),
            n("c", &[], "waiting"),
        ]);
        assert_eq!(step.ready.len(), 3);
        assert!(step.unreachable.is_empty() && step.deadlocked.is_empty());
    }

    // ---- execution ----------------------------------------------------------

    #[test]
    fn a_chain_releases_one_link_at_a_time() {
        let mut nodes = vec![
            n("a", &[], "running"),
            n("b", &["a"], "waiting"),
            n("c", &["b"], "waiting"),
        ];
        assert!(dag_step(nodes.clone()).ready.is_empty(), "b must wait for a");

        nodes[0].status = "done".into();
        assert_eq!(dag_step(nodes.clone()).ready, vec!["b"]);

        nodes[1].status = "done".into();
        assert_eq!(dag_step(nodes).ready, vec!["c"]);
    }

    #[test]
    fn unrelated_work_never_waits() {
        // The whole point: D is not held up by the A→B→C chain.
        let step = dag_step(vec![
            n("a", &[], "running"),
            n("b", &["a"], "waiting"),
            n("d", &[], "waiting"),
        ]);
        assert_eq!(step.ready, vec!["d"]);
    }

    #[test]
    fn a_diamond_releases_the_join_only_when_both_arms_land() {
        let mut nodes = vec![
            n("root", &[], "done"),
            n("left", &["root"], "done"),
            n("right", &["root"], "running"),
            n("join", &["left", "right"], "waiting"),
        ];
        assert!(dag_step(nodes.clone()).ready.is_empty(), "one arm still running");
        nodes[2].status = "done".into();
        assert_eq!(dag_step(nodes).ready, vec!["join"]);
    }

    #[test]
    fn failure_cascades_without_traversal() {
        // B is poisoned by A directly. C is poisoned by B on the NEXT pass,
        // because "unreachable" is itself a poison state — that is the whole
        // trick, and it means no cascade code exists to get wrong.
        let mut nodes = vec![
            n("a", &[], "failed"),
            n("b", &["a"], "waiting"),
            n("c", &["b"], "waiting"),
        ];
        let first = dag_step(nodes.clone());
        assert_eq!(first.unreachable, vec!["b"]);
        assert!(first.ready.is_empty());

        nodes[1].status = "unreachable".into();
        let second = dag_step(nodes);
        assert_eq!(second.unreachable, vec!["c"]);
    }

    #[test]
    fn a_cancelled_parent_poisons_too() {
        let step = dag_step(vec![n("a", &[], "cancelled"), n("b", &["a"], "waiting")]);
        assert_eq!(step.unreachable, vec!["b"]);
    }

    #[test]
    fn an_unknown_dependency_does_not_strand_the_work() {
        // Degrades to the old behaviour rather than waiting forever.
        let step = dag_step(vec![n("b", &["not-in-plan"], "waiting")]);
        assert_eq!(step.ready, vec!["b"]);
        assert!(step.deadlocked.is_empty());
    }

    #[test]
    fn a_cycle_is_reported_as_deadlocked_not_left_hanging() {
        let step = dag_step(vec![n("a", &["b"], "waiting"), n("b", &["a"], "waiting")]);
        assert!(step.ready.is_empty());
        assert_eq!(step.deadlocked.len(), 2);
    }

    #[test]
    fn waiting_on_live_work_is_not_deadlock() {
        let step = dag_step(vec![n("a", &[], "running"), n("b", &["a"], "waiting")]);
        assert!(step.deadlocked.is_empty(), "b is waiting on real progress");
    }

    // ---- validation ---------------------------------------------------------

    #[test]
    fn a_clean_plan_validates() {
        let issues = dag_validate(
            vec![n("a", &[], "waiting"), n("b", &["a"], "waiting")],
            None,
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn a_cycle_is_rejected_when_written_and_reported_once() {
        let issues = dag_validate(
            vec![
                n("a", &["c"], "waiting"),
                n("b", &["a"], "waiting"),
                n("c", &["b"], "waiting"),
            ],
            None,
        );
        let cycles: Vec<_> = issues.iter().filter(|i| i.kind == "cycle").collect();
        assert_eq!(cycles.len(), 1, "one finding per cycle, not one per member");
        assert!(cycles[0].detail.contains('→'), "names the path: {}", cycles[0].detail);
    }

    #[test]
    fn a_missing_blocker_is_named() {
        let issues = dag_validate(vec![n("b", &["ghost"], "waiting")], None);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, "unknown_dependency");
        assert!(issues[0].detail.contains("ghost"));
    }

    #[test]
    fn self_dependency_is_its_own_finding() {
        let issues = dag_validate(vec![n("a", &["a"], "waiting")], None);
        assert!(issues.iter().any(|i| i.kind == "self_dependency"));
    }

    #[test]
    fn depth_is_capped() {
        let nodes = vec![
            n("a", &[], "waiting"),
            n("b", &["a"], "waiting"),
            n("c", &["b"], "waiting"),
            n("d", &["c"], "waiting"),
        ];
        assert!(dag_validate(nodes.clone(), Some(5)).is_empty());
        let deep = dag_validate(nodes, Some(2));
        assert!(deep.iter().any(|i| i.kind == "too_deep" && i.node == "d"));
    }
}
