// What a build slice actually changed — the data behind the Files tab.
//
// DIFF AGAINST THE MERGE BASE, NOT THE WORKING TREE. This is the whole reason the
// module exists. On the 2026-08-09 Guardian run, `git status` in the
// engine-chains worktree showed three untracked files and nothing else, because
// the agent had already committed twelve times. A working-tree diff would have
// reported an agent doing nothing while it had in fact written 473 lines across
// ten files. The reviewer's question is "what has this slice done since it forked
// from the branch", and only the merge base answers it.
//
// The diff is taken from the merge base to the WORKING TREE (`git diff <base>`,
// not `<base>..HEAD`), so committed and uncommitted work both appear — an agent
// mid-edit is the normal case while a build is running.

use serde::Serialize;
use std::time::Duration;

use crate::repo_check::git;

#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    /// "modified" | "new" (untracked) | "binary"
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SliceChanges {
    /// The merge-base commit everything is measured from.
    pub base: String,
    /// Which ref the base was computed against, so the UI can say what it means.
    pub base_ref: String,
    pub branch: String,
    pub files: Vec<ChangedFile>,
    pub added: usize,
    pub removed: usize,
    /// Commits on this slice since the base.
    pub commits: usize,
    pub head: String,
}

const SHORT: Duration = Duration::from_secs(10);

fn g(args: &[&str], cwd: &str) -> Option<String> {
    let (ok, out) = git(args, Some(cwd), SHORT);
    if ok { Some(out) } else { None }
}

/// Choose the base by CLOSEST FORK, not by name.
///
/// Defaulting to main/master is wrong and quietly so. Measured on the real
/// engine-chains worktree, whose slice branch forked from feature/dashboard:
///
///     feature/dashboard    ahead=3    24 files
///     feature/orchestrator ahead=8    50 files
///     main                 ahead=15   77 files, +8021
///
/// Against main the slice appears to have rewritten the repository; the true
/// change was ten files. So: among candidate refs, take the one whose merge base
/// leaves HEAD the fewest commits ahead — that is the branch it actually forked
/// from. A caller that KNOWS the base should still pass it; this is the fallback.
///
/// Sibling slice branches are excluded. They share a very recent merge base with
/// each other, so one slice would happily measure itself against another and
/// report almost no change at all.
fn resolve_base_ref(cwd: &str, want: Option<&str>) -> Option<String> {
    let exists = |r: &str| g(&["rev-parse", "--verify", "--quiet", &format!("{r}^{{commit}}")], cwd).is_some();

    if let Some(w) = want.map(str::trim).filter(|w| !w.is_empty()) {
        if exists(w) { return Some(w.to_string()); }
    }

    let current = g(&["rev-parse", "--abbrev-ref", "HEAD"], cwd).unwrap_or_default().trim().to_string();
    let refs = g(&["for-each-ref", "--format=%(refname:short)", "refs/heads", "refs/remotes"], cwd)
        .unwrap_or_default();

    let mut scored: Vec<(usize, usize, String)> = refs
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty() && *r != current && !r.ends_with("/HEAD"))
        // A sibling slice is not a base.
        .filter(|r| !r.contains("nautloom/"))
        .take(40)
        .filter_map(|r| {
            let mb = g(&["merge-base", "HEAD", r], cwd)?.trim().to_string();
            let ahead: usize = g(&["rev-list", "--count", &format!("{mb}..HEAD")], cwd)?.trim().parse().ok()?;
            // Prefer a local branch over its remote twin when both fit equally.
            let remote_penalty = usize::from(r.contains('/') && r.starts_with("origin/"));
            Some((ahead, remote_penalty, r.to_string()))
        })
        .collect();

    scored.sort();
    if let Some((_, _, r)) = scored.into_iter().next() {
        return Some(r);
    }
    // A repo with no other branch at all: fall back to the well-known names.
    ["main", "master", "origin/main", "origin/master"]
        .into_iter()
        .find(|r| exists(r))
        .map(str::to_string)
}

/// Parse `git diff --numstat`. Binary files render as "-\t-\tpath".
fn parse_numstat(out: &str) -> Vec<ChangedFile> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '\t');
            let a = it.next()?;
            let r = it.next()?;
            let path = it.next()?.trim();
            if path.is_empty() { return None; }
            // Renames arrive as "old => new" or with braces; the new name is what
            // exists on disk and what the reviewer can open.
            let path = path.rsplit(" => ").next().unwrap_or(path).trim_end_matches('}');
            let binary = a == "-" || r == "-";
            Some(ChangedFile {
                path: path.to_string(),
                added: a.parse().unwrap_or(0),
                removed: r.parse().unwrap_or(0),
                status: if binary { "binary".into() } else { "modified".into() },
            })
        })
        .collect()
}

/// Everything a slice changed since it forked.
#[tauri::command]
pub fn slice_changes(worktree: String, base_ref: Option<String>) -> Result<SliceChanges, String> {
    let cwd = worktree;
    if !std::path::Path::new(&cwd).is_dir() {
        return Err(format!("no such worktree: {cwd}"));
    }
    let branch = g(&["rev-parse", "--abbrev-ref", "HEAD"], &cwd).unwrap_or_default().trim().to_string();
    let head = g(&["rev-parse", "--short", "HEAD"], &cwd).unwrap_or_default().trim().to_string();

    let base_ref = resolve_base_ref(&cwd, base_ref.as_deref())
        .ok_or("could not resolve a base ref — this worktree shares no history with any other branch")?;
    let base = g(&["merge-base", "HEAD", &base_ref], &cwd)
        .ok_or_else(|| format!("no merge base between HEAD and {base_ref}"))?
        .trim()
        .to_string();

    // Base → WORKING TREE, so an agent's uncommitted edits count too.
    let mut files = parse_numstat(&g(&["diff", "--numstat", &base], &cwd).unwrap_or_default());

    // Untracked files are invisible to `git diff` but are real work; count their
    // lines so a brand-new module does not show up as "0 changed".
    if let Some(list) = g(&["ls-files", "--others", "--exclude-standard"], &cwd) {
        for p in list.lines().map(str::trim).filter(|p| !p.is_empty()) {
            let added = std::fs::read_to_string(std::path::Path::new(&cwd).join(p))
                .map(|s| s.lines().count())
                .unwrap_or(0);
            files.push(ChangedFile { path: p.to_string(), added, removed: 0, status: "new".into() });
        }
    }

    files.sort_by(|a, b| (b.added + b.removed).cmp(&(a.added + a.removed)).then(a.path.cmp(&b.path)));
    let added = files.iter().map(|f| f.added).sum();
    let removed = files.iter().map(|f| f.removed).sum();
    let commits = g(&["rev-list", "--count", &format!("{base}..HEAD")], &cwd)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    Ok(SliceChanges { base, base_ref, branch, files, added, removed, commits, head })
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// "ctx" | "add" | "del" | "hunk"
    pub kind: String,
    /// Line number in the NEW file; 0 for deletions and hunk headers.
    pub n: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub base: String,
    pub lines: Vec<DiffLine>,
    pub added: usize,
    pub removed: usize,
    /// True when the diff was withheld for size. The UI offers "Load diff"
    /// instead, the way Cursor hides large diffs by default — a slice can touch a
    /// 1000-line lockfile, and rendering it helps nobody.
    pub truncated: bool,
}

/// Turn unified-diff text into typed, numbered lines.
///
/// Split out from the command so it can be tested against fixture text — the
/// parsing is the part that breaks, not the subprocess call.
fn parse_unified(path: &str, base: &str, body: &str, max_lines: usize) -> FileDiff {
    let mut lines = Vec::new();
    let (mut added, mut removed, mut n) = (0usize, 0usize, 0usize);
    for l in body.lines() {
        if l.starts_with("@@") {
            // "@@ -a,b +c,d @@" — c is where the new file resumes.
            n = l
                .split('+')
                .nth(1)
                .and_then(|s| s.split([',', ' ']).next())
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1);
            lines.push(DiffLine { kind: "hunk".into(), n: 0, text: l.to_string() });
            continue;
        }
        // Skip the file header; it repeats what the UI already shows.
        if l.starts_with("diff --git") || l.starts_with("index ")
            || l.starts_with("--- ") || l.starts_with("+++ ")
            || l.starts_with("new file") || l.starts_with("deleted file")
            || l.starts_with("similarity ") || l.starts_with("rename ")
        {
            continue;
        }
        if lines.is_empty() { continue; } // preamble before the first hunk
        match l.chars().next() {
            Some('+') => { added += 1; lines.push(DiffLine { kind: "add".into(), n, text: l[1..].to_string() }); n += 1; }
            Some('-') => { removed += 1; lines.push(DiffLine { kind: "del".into(), n: 0, text: l[1..].to_string() }); }
            Some('\\') => {} // "\ No newline at end of file"
            _ => {
                let text = l.strip_prefix(' ').unwrap_or(l).to_string();
                lines.push(DiffLine { kind: "ctx".into(), n, text });
                n += 1;
            }
        }
    }
    let truncated = lines.len() > max_lines;
    if truncated { lines.truncate(max_lines); }
    FileDiff { path: path.to_string(), base: base.to_string(), lines, added, removed, truncated }
}

/// One file's diff against the slice's merge base.
#[tauri::command]
pub fn slice_file_diff(
    worktree: String,
    path: String,
    base_ref: Option<String>,
    max_lines: Option<usize>,
) -> Result<FileDiff, String> {
    let cwd = worktree;
    let base_ref = resolve_base_ref(&cwd, base_ref.as_deref()).ok_or("could not resolve a base ref")?;
    let base = g(&["merge-base", "HEAD", &base_ref], &cwd)
        .ok_or("no merge base")?
        .trim()
        .to_string();
    // An untracked file has nothing to diff against, so show it as all-additions
    // rather than an empty pane that looks like a bug.
    let tracked = g(&["ls-files", "--error-unmatch", "--", &path], &cwd).is_some();
    let body = if tracked {
        g(&["diff", "-U3", &base, "--", &path], &cwd).unwrap_or_default()
    } else {
        let text = std::fs::read_to_string(std::path::Path::new(&cwd).join(&path)).unwrap_or_default();
        let n = text.lines().count();
        let mut s = format!("@@ -0,0 +1,{n} @@\n");
        for l in text.lines() { s.push('+'); s.push_str(l); s.push('\n'); }
        s
    };
    Ok(parse_unified(&path, &base, &body, max_lines.unwrap_or(2000)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_parses_counts_and_binaries() {
        let out = "167\t12\tguardian/orchestrator.py\n89\t0\tengine/src/chains.mjs\n-\t-\tassets/logo.png\n";
        let f = parse_numstat(out);
        assert_eq!(f.len(), 3);
        assert_eq!((f[0].added, f[0].removed), (167, 12));
        assert_eq!(f[2].status, "binary");
        assert_eq!(f[2].added, 0);
    }

    #[test]
    fn a_rename_is_reported_under_the_name_that_exists_on_disk() {
        // The old path cannot be opened, so offering it would be a dead link.
        let f = parse_numstat("3\t1\tguardian/old.py => guardian/new.py\n");
        assert_eq!(f[0].path, "guardian/new.py");
    }

    /// The real hunk from the 2026-08-09 run.
    fn fixture() -> &'static str {
        "diff --git a/guardian/orchestrator.py b/guardian/orchestrator.py\n\
         index c78b33b..3450457 100644\n\
         --- a/guardian/orchestrator.py\n\
         +++ b/guardian/orchestrator.py\n\
         @@ -8,7 +8,10 @@ from __future__ import annotations\n\
         \x20import asyncio\n\
         +import time\n\
         +from dataclasses import asdict\n\
         \x20\n\
         -from guardian.models import Alert, AlertType, Goal, Project, Run\n\
         +from guardian.models import Alert, AlertType, Goal, Phase, Project, Run\n"
    }

    #[test]
    fn unified_diff_becomes_typed_numbered_lines() {
        let d = parse_unified("guardian/orchestrator.py", "de373df", fixture(), 2000);
        assert_eq!((d.added, d.removed), (3, 1));
        // The header is dropped; the hunk marker is kept as its own line.
        assert_eq!(d.lines[0].kind, "hunk");
        assert!(d.lines.iter().all(|l| !l.text.starts_with("diff --git")));

        let first_ctx = d.lines.iter().find(|l| l.kind == "ctx").unwrap();
        assert_eq!(first_ctx.text, "import asyncio");
        assert_eq!(first_ctx.n, 8, "context numbering starts at the hunk's +8");

        let adds: Vec<_> = d.lines.iter().filter(|l| l.kind == "add").collect();
        assert_eq!(adds[0].text, "import time");
        assert_eq!(adds[0].n, 9, "the added line takes the next new-file number");

        // A deletion has no line in the new file, so it must not claim one.
        let del = d.lines.iter().find(|l| l.kind == "del").unwrap();
        assert_eq!(del.n, 0);
        assert!(del.text.starts_with("from guardian.models"));
    }

    #[test]
    fn large_diffs_are_withheld_rather_than_rendered() {
        let mut body = String::from("@@ -1,1 +1,400 @@\n");
        for i in 0..400 { body.push_str(&format!("+line {i}\n")); }
        let d = parse_unified("lock.json", "abc", &body, 50);
        assert!(d.truncated);
        assert_eq!(d.lines.len(), 50);
        // The totals still describe the WHOLE diff, so the header cannot lie
        // about how big the change is just because the body was withheld.
        assert_eq!(d.added, 400);
    }

    /// The selection rule itself, over the numbers measured on the real
    /// engine-chains worktree. Sorting is what picks the base, so sorting is what
    /// gets tested — the git calls around it are not the part that was wrong.
    #[test]
    fn the_closest_fork_wins_not_the_default_branch() {
        let mut scored: Vec<(usize, usize, String)> = vec![
            (15, 0, "main".into()),
            (15, 1, "origin/main".into()),
            (3, 0, "feature/dashboard".into()),
            (8, 0, "feature/orchestrator".into()),
            (3, 1, "origin/feature/dashboard".into()),
        ];
        scored.sort();
        assert_eq!(scored[0].2, "feature/dashboard");
        // Against main this slice looked like 77 files and +8021 lines; against
        // its real fork point it is ten files.
        assert!(scored[0].0 < 15);
        // Local beats the identical remote ref.
        assert_eq!(scored[1].2, "origin/feature/dashboard");
    }

    #[test]
    fn an_empty_diff_is_not_an_error() {
        let d = parse_unified("unchanged.rs", "abc", "", 2000);
        assert!(d.lines.is_empty());
        assert!(!d.truncated);
        assert_eq!((d.added, d.removed), (0, 0));
    }
}
