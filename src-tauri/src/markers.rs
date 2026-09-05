// The address path in xNAUT: standing conventions an agent looks up instead of
// asking about (ENGRAMOSS-10, XNAUT-245 item 6a).
//
// Ported from Engram-OSS (ENGRAMOSS-9, `api/lib/markers.js::scan/resolve/
// rootIndex`, commit e70ded7, MIT), whose own anchor convention and
// superseded-by vocabulary come from xChuCx/agent-memory (MIT). The anchor
// syntax and the marker grammar are deliberately IDENTICAL, so one set of
// vault files serves both readers and neither owns the truth.
//
// WHERE WE DEPART, and why: Engram serves this over HTTP. xNAUT reads the
// files directly. An agent launch must not be able to fail because another
// service is down, and the index is derived and disposable by design, so a
// second reader of the same markdown costs nothing and removes a dependency
// from the one code path that has to work every time.
//
// The problem it solves, from André watching the first working run on
// 2026-08-29: the agent stopped to ask whether to work on its own branch.
// Answered in seconds with a human present; unattended that is a run parked
// all night. Branch naming, worktree rules, commit style, the test command,
// what "done" requires: these are project facts, not per-ticket decisions,
// and they belong in the prompt once rather than in a question every run.
//
// Two constraints from the research, both load-bearing (see the design doc,
// work:Development/Engram-OSS/features/2026-08-30_Marker-Index-Address-Path.md):
//
//   1. Exactly ONE level of index. A second routing level measurably hurts.
//   2. NO summarization anywhere in this path. The index is pointers; the
//      sections are served verbatim. A summary is how a constraint gets
//      silently dropped, and a silently dropped constraint is the whole
//      failure mode this sprint exists to remove.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One anchored section of a knowledge file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Section {
    /// `<project>/<section>`, or one level deeper.
    pub marker: String,
    /// The first non-empty line after the anchor. This is what the root index
    /// shows, so it has to say what the section answers.
    pub hook: String,
    /// Everything from the anchor to the next anchor or end of file, verbatim.
    pub body: String,
    /// Set when the anchor carries `superseded-by:`. Resolving a superseded
    /// marker answers with the successor and a notice, never a silent miss.
    pub superseded_by: Option<String>,
    pub file: String,
}

/// `<!-- @marker: xnaut/conventions -->` or
/// `<!-- @marker: xnaut/old superseded-by: xnaut/new -->`, alone on its line.
///
/// Hand-parsed rather than regex: the grammar is three tokens and the crate
/// already carries `regex`, but a parser that cannot backtrack is easier to
/// be sure of than a pattern with two optional groups.
fn parse_anchor(line: &str) -> Option<(String, Option<String>)> {
    let inner = line
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    let rest = inner.strip_prefix("@marker:")?.trim();
    let mut parts = rest.split_whitespace();
    let marker = parts.next()?;
    if !valid_marker(marker) {
        return None;
    }
    let superseded_by = match parts.next() {
        None => None,
        Some("superseded-by:") => {
            let successor = parts.next()?;
            if !valid_marker(successor) {
                return None;
            }
            Some(successor.to_string())
        }
        // Anything else on the line means this is not an anchor we understand.
        // Refusing is right: a half-parsed anchor is a marker that resolves to
        // the wrong section.
        Some(_) => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((marker.to_string(), superseded_by))
}

/// `<project>/<section>` with at most one extra level, lowercase and dashes.
/// The depth cap is the research constraint, enforced rather than documented.
fn valid_marker(marker: &str) -> bool {
    let parts: Vec<&str> = marker.split('/').collect();
    (2..=3).contains(&parts.len())
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// Split one file into its anchored sections. A file with no anchors yields
/// nothing, which is why pointing this at a whole vault is safe.
pub fn sections_in(body: &str, file: &str) -> Vec<Section> {
    let lines: Vec<&str> = body.lines().collect();
    let anchors: Vec<(usize, String, Option<String>)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| parse_anchor(line).map(|(m, s)| (i, m, s)))
        .collect();
    anchors
        .iter()
        .enumerate()
        .map(|(n, (start, marker, superseded_by))| {
            let end = anchors
                .get(n + 1)
                .map(|(next, _, _)| *next)
                .unwrap_or(lines.len());
            let body_lines = &lines[start + 1..end];
            Section {
                marker: marker.clone(),
                hook: body_lines
                    .iter()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or(&"")
                    .trim()
                    .to_string(),
                body: body_lines.join("\n").trim().to_string(),
                superseded_by: superseded_by.clone(),
                file: file.to_string(),
            }
        })
        .collect()
}

/// Every marker under a set of roots. BTreeMap so the index is ordered the
/// same way on every machine: a prompt that reorders between runs defeats the
/// prompt cache and makes two runs incomparable.
pub fn scan(roots: &[PathBuf]) -> BTreeMap<String, Section> {
    let mut index = BTreeMap::new();
    for root in roots {
        for file in walk(root) {
            let Ok(body) = std::fs::read_to_string(&file) else {
                continue;
            };
            for section in sections_in(&body, &file.to_string_lossy()) {
                index.insert(section.marker.clone(), section);
            }
        }
    }
    index
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Same skips as the Engram scanner. `node_modules` is not paranoia:
        // walking one cost the vault indexer a full minute (XNAUT-241).
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    out
}

/// What `resolve` can answer. A miss is a variant rather than an error so the
/// caller has to handle it; the point of an address path is that a lookup
/// never quietly returns the wrong thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Section(Section),
    /// The marker was superseded. The successor's body, plus the notice.
    Superseded {
        notice: String,
        section: Section,
    },
    /// No such marker. Carries the nearest names so the agent can retry once
    /// instead of falling back to asking.
    Missing {
        marker: String,
        nearest: Vec<String>,
    },
}

pub fn resolve(index: &BTreeMap<String, Section>, marker: &str) -> Resolved {
    let Some(section) = index.get(marker) else {
        // Containment has to run BOTH ways. Asking for `xnaut/tests-command`
        // when the marker is `xnaut/tests` is the likely typo, and testing
        // only `key.contains(query)` misses it, which sends the agent back to
        // asking. Compare the last segments so the shared project prefix does
        // not make everything a neighbour.
        let tail = marker.split('/').next_back().unwrap_or_default();
        let nearest: Vec<String> = index
            .keys()
            .filter(|k| {
                let other = k.split('/').next_back().unwrap_or_default();
                !tail.is_empty()
                    && !other.is_empty()
                    && (other.contains(tail) || tail.contains(other))
            })
            .take(5)
            .cloned()
            .collect();
        return Resolved::Missing {
            marker: marker.to_string(),
            nearest,
        };
    };
    match &section.superseded_by {
        None => Resolved::Section(section.clone()),
        Some(successor) => match index.get(successor) {
            Some(next) => Resolved::Superseded {
                notice: format!("{marker} was superseded by {successor}"),
                section: next.clone(),
            },
            // A dangling successor answers with the old section and says so,
            // rather than reporting a miss for a marker that plainly exists.
            None => Resolved::Superseded {
                notice: format!(
                    "{marker} names successor {successor}, which does not exist; \
                     serving the superseded section"
                ),
                section: section.clone(),
            },
        },
    }
}

/// The chapter list a launched agent receives: pointers only, one line each,
/// active sections for one project, ordered.
///
/// Returns None when the project has no markers, so the composer can leave the
/// block out entirely rather than injecting an empty heading. An empty section
/// in a prompt teaches the model that the mechanism is decorative.
pub fn root_index(index: &BTreeMap<String, Section>, project: &str) -> Option<String> {
    let prefix = format!("{}/", project.to_ascii_lowercase());
    let entries: Vec<&Section> = index
        .values()
        .filter(|s| s.marker.starts_with(&prefix) && s.superseded_by.is_none())
        .collect();
    if entries.is_empty() {
        return None;
    }
    let mut out = String::from(
        "\n## Standing conventions for this project\n\n\
         These are decided. Do not ask about them, and do not improvise an answer: \
         resolve the marker and follow what it says. Each line is a marker and what \
         it answers. Call the `xnaut_resolve_marker` tool with the marker to read \
         the section verbatim.\n\n",
    );
    for section in &entries {
        out.push_str(&format!("- `{}` {}\n", section.marker, section.hook));
    }
    out.push('\n');
    Some(out)
}

/// The vault roots xNAUT scans. Same default as the Engram scanner.
pub fn roots() -> Vec<PathBuf> {
    crate::vault::vault_root("work").ok().into_iter().collect()
}

/// Which project's conventions a run in this directory should get.
///
/// `--git-common-dir` rather than `--show-toplevel`: inside a worktree the
/// toplevel is the worktree itself, so `.worktrees/safety-net` would ask for
/// the conventions of a project called `safety-net` and silently get none.
/// The common dir is the main repo's `.git` in both cases, and every agent
/// launch happens in a worktree.
pub fn project_key_for(dir: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &dir.to_string_lossy(),
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let git_dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    Some(
        git_dir
            .parent()?
            .file_name()?
            .to_string_lossy()
            .to_ascii_lowercase(),
    )
}

/// The conventions block for a launch in `dir`, or nothing when the project
/// has no markers. Nothing is the common case today and must stay cheap and
/// silent: a project that has not written its conventions down is not an
/// error, it just gets the behaviour it had before this existed.
pub fn block_for_dir(dir: &Path) -> Option<String> {
    index_for(&project_key_for(dir)?)
}

/// The index for one project, read fresh. Cheap enough not to cache: the files
/// in git are the truth, and a cache is how an edited convention goes on being
/// answered with the old text.
pub fn index_for(project: &str) -> Option<String> {
    root_index(&scan(&roots()), project)
}

#[derive(serde::Serialize)]
pub struct MarkerAnswer {
    pub marker: String,
    pub found: bool,
    /// The section verbatim. Never summarized: see the module header.
    pub body: String,
    pub file: String,
    /// Present when the marker was superseded, or when the lookup missed and
    /// there are near names worth trying.
    pub notice: String,
    pub nearest: Vec<String>,
}

/// The one tool an agent needs beside the index.
#[tauri::command]
pub fn resolve_marker(marker: String) -> MarkerAnswer {
    match resolve(&scan(&roots()), &marker) {
        Resolved::Section(s) => MarkerAnswer {
            marker: s.marker,
            found: true,
            body: s.body,
            file: s.file,
            notice: String::new(),
            nearest: Vec::new(),
        },
        Resolved::Superseded { notice, section } => MarkerAnswer {
            marker: section.marker.clone(),
            found: true,
            body: section.body,
            file: section.file,
            notice,
            nearest: Vec::new(),
        },
        Resolved::Missing { marker, nearest } => MarkerAnswer {
            marker,
            found: false,
            body: String::new(),
            file: String::new(),
            notice: "no such marker".into(),
            nearest,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
# Conventions

<!-- @marker: xnaut/branching -->
Branch off the live release lineage, never main.

Use `--no-track`, and set the push remote to forgejo.

<!-- @marker: xnaut/commits -->
No em-dashes anywhere, including commit messages.

<!-- @marker: xnaut/old-tests superseded-by: xnaut/tests -->
Run `cargo test`.

<!-- @marker: xnaut/tests -->
Run `cargo test --bin xnaut`, all of it.
";

    fn index() -> BTreeMap<String, Section> {
        let mut map = BTreeMap::new();
        for section in sections_in(DOC, "conventions.md") {
            map.insert(section.marker.clone(), section);
        }
        map
    }

    #[test]
    fn a_section_runs_to_the_next_anchor_and_keeps_every_line() {
        // The no-summarization constraint, enforced. If a section ever gets
        // truncated to its hook, the second line of `branching` (the push
        // remote, which is the part people get wrong) disappears silently.
        let branching = &index()["xnaut/branching"];
        assert_eq!(
            branching.hook,
            "Branch off the live release lineage, never main."
        );
        assert!(
            branching.body.contains("push remote to forgejo"),
            "the section lost everything after its first line: {:?}",
            branching.body
        );
        assert!(
            !branching.body.contains("em-dashes"),
            "a section ran into the next one"
        );
    }

    #[test]
    fn a_superseded_marker_answers_with_the_successor_and_says_so() {
        // The whole point of an address path: a lookup never quietly returns
        // nothing, and never quietly returns the stale answer.
        match resolve(&index(), "xnaut/old-tests") {
            Resolved::Superseded { notice, section } => {
                assert!(notice.contains("xnaut/tests"), "{notice}");
                assert!(
                    section.body.contains("--bin xnaut"),
                    "served the old section"
                );
            }
            other => panic!("expected a supersede, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_marker_offers_the_nearest_names_instead_of_a_bare_miss() {
        // A miss that says only "no" sends the agent back to asking, which is
        // the behaviour this whole module exists to remove.
        match resolve(&index(), "xnaut/tests-command") {
            Resolved::Missing { nearest, .. } => {
                assert!(nearest.contains(&"xnaut/tests".to_string()), "{nearest:?}");
            }
            other => panic!("expected a miss, got {other:?}"),
        }
    }

    #[test]
    fn the_root_index_is_pointers_and_never_the_sections() {
        // Constraint 2 from the research. If the index ever inlines bodies it
        // stops being an index and becomes the memory dump it replaced, and
        // the token budget goes with it.
        let out = root_index(&index(), "xnaut").expect("markers exist");
        assert!(out.contains("`xnaut/branching`"));
        assert!(
            out.contains("Branch off the live release lineage"),
            "the hook belongs in the index"
        );
        assert!(
            !out.contains("push remote to forgejo"),
            "the index inlined a section body instead of pointing at it"
        );
        assert!(
            !out.contains("xnaut/old-tests"),
            "a superseded marker must not be advertised in the chapter list"
        );
    }

    #[test]
    fn a_project_with_no_markers_gets_no_block_at_all() {
        // An empty heading in a prompt teaches the model the mechanism is
        // decorative, which is worse than not having it.
        assert!(root_index(&index(), "nautgate").is_none());
    }

    #[test]
    fn the_marker_grammar_caps_the_depth_the_research_caps() {
        assert!(valid_marker("xnaut/tests"));
        assert!(valid_marker("xnaut/build/tests"));
        assert!(!valid_marker("xnaut"), "one level is not an address");
        assert!(
            !valid_marker("xnaut/build/unit/tests"),
            "a second routing level hurts"
        );
        assert!(
            !valid_marker("xNAUT/Tests"),
            "markers are lowercase so they are quotable"
        );
    }

    /// The real vault, opt-in. Ignored because it depends on this machine's
    /// files, but it is the only check that the thing agents actually receive
    /// is the thing this module thinks it produces:
    ///
    ///   cargo test --bin xnaut -- --ignored --nocapture the_real_vault
    #[test]
    #[ignore]
    fn the_real_vault_index_fits_the_budget() {
        let Some(block) = index_for("xnaut") else {
            panic!(
                "no xnaut markers in {:?}; the conventions doc is missing",
                roots()
            );
        };
        println!("{block}");
        // ENGRAMOSS-10 caps the root index at ~1,000 tokens. Roughly four
        // characters to a token is close enough to catch the failure that
        // matters, which is somebody pasting a section body into a hook.
        let tokens = block.len() / 4;
        println!("~{tokens} tokens, {} bytes", block.len());
        assert!(
            tokens < 1000,
            "the chapter list has grown into a memory dump: ~{tokens} tokens"
        );
        assert!(
            block.contains("xnaut/branching"),
            "the branch question is the one that parked a run"
        );
        assert!(
            block.contains("xnaut/done"),
            "the done-vs-complete rule must be addressable"
        );
    }

    #[test]
    fn a_line_that_is_not_an_anchor_is_never_treated_as_one() {
        // A half-parsed anchor resolves a marker to the wrong section, which
        // is strictly worse than not finding it.
        assert!(parse_anchor("<!-- @marker: xnaut/tests -->").is_some());
        assert!(parse_anchor("<!-- @marker: xnaut/a superseded-by: xnaut/b -->").is_some());
        assert!(parse_anchor("<!-- @marker: xnaut/a supersededby xnaut/b -->").is_none());
        assert!(parse_anchor("<!-- @marker: xnaut/tests --> trailing").is_none());
        assert!(parse_anchor("text <!-- @marker: xnaut/tests -->").is_none());
        assert!(parse_anchor("<!-- marker: xnaut/tests -->").is_none());
        assert!(parse_anchor("<!-- @marker: xnaut/a xnaut/b -->").is_none());
    }
}
