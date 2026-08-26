// xFusion (XNAUT-237): N models work one problem together. Selection routes
// one task to one agent; fusion runs a PANEL on the same problem and the
// owner (or NautBot) reads uncorrelated answers. Doctrine, decided with
// André 2026-08-26: fusion convenes for JUDGMENT — review, refute, decide,
// grok — never for routine production; there is never a judge model (a judge
// collapses N models back into one selection at N times the cost); panelists
// see each other as TAGS only (XNAUT-236).
//
// Shaped after disler/fusion-harness (MIT) — /fh-opinion and /fh-debate,
// extensions/fusion-harness/modules/cmd-readonly.ts — with departures worth
// naming: our panelists are xNAUT identities with their own providers rather
// than raw model slots; read-only is enforced in run_turn by the xfusion
// canvas-key prefix rather than a child-process tool flag; and the debate
// self-terminates when no agent moves its position, because fusion-harness's
// own demo spent two full rounds on agents "reinforcing each other".

use serde_json::json;

/// One panelist, resolved from an agent profile to something run_turn takes.
pub struct Panelist {
    pub handle: String,
    llm: crate::settings::LlmSettings,
    system: String,
}

pub struct PanelAnswer {
    pub handle: String,
    pub ok: bool,
    pub text: String,
}

/// The largest panel. Three is where the demo ran and where reading the
/// output still costs less than it saves; five is the hard ceiling.
const MAX_PANEL: usize = 3;
pub const MAX_ROUNDS: usize = 4;

/// Resolves the panel: named handles, or every non-NautBot agent up to
/// MAX_PANEL. NautBot never sits on a panel — it convenes them and reads
/// them, and a convener that also answers is a judge by another name.
/// Quarantined agents are excluded (kill-switches, XNAUT-231).
pub async fn resolve_panel(named: Option<Vec<String>>) -> Result<Vec<Panelist>, String> {
    let app = crate::nudge::app().ok_or("the app is not running")?;
    let state = tauri::Manager::try_state::<crate::state::AppState>(app)
        .ok_or("app state unavailable")?;
    let settings = state.settings.lock().await.clone();
    let switches = crate::switches::load();
    let profiles = crate::agent_profiles::agent_profile_list()?;
    let wanted: Option<Vec<String>> = named.map(|list| {
        list.iter()
            .map(|h| h.trim().trim_start_matches('@').to_ascii_lowercase())
            .collect()
    });
    let mut panel = Vec::new();
    for profile in profiles {
        if profile.handle == crate::agent_profiles::RESERVED_NAUTBOT_HANDLE {
            continue;
        }
        if switches.is_quarantined(&profile.handle) {
            continue;
        }
        if let Some(wanted) = &wanted {
            if !wanted.contains(&profile.handle) {
                continue;
            }
        }
        // The same per-profile provider resolution the agent-chat path uses
        // (agent_profiles.rs): global settings unless the profile names a
        // provider, chat model override on top.
        let provider = profile.provider.trim();
        let mut llm = if provider.is_empty() || provider == "global" {
            settings.llm.clone()
        } else {
            match crate::chat::provider_llm(&settings, provider) {
                Some(llm) => llm,
                None => continue, // an unconfigured provider cannot answer
            }
        };
        let chat_model = profile.chat_model_or_model().trim().to_string();
        if !chat_model.is_empty() {
            llm.model = chat_model;
        }
        if llm.model.trim().is_empty() {
            continue;
        }
        panel.push(Panelist {
            system: crate::composer::chat_system(&profile),
            handle: profile.handle,
            llm,
        });
        if panel.len() == MAX_PANEL && wanted.is_none() {
            break;
        }
    }
    if panel.len() < 2 {
        return Err(
            "a panel needs at least two answerable agents (NautBot and quarantined agents excluded; every panelist needs a configured model)"
                .to_string(),
        );
    }
    Ok(panel)
}

/// One concurrent read-only round: every panelist answers `prompt`
/// independently. The `xfusion:` canvas-key prefix is what makes the turn
/// read-only inside run_turn, and capabilities are empty so no plugin opens.
pub async fn panel_round(panel: &[Panelist], prompt: &str) -> Vec<PanelAnswer> {
    let turns = panel.iter().map(|p| {
        let messages = vec![
            json!({ "role": "system", "content": p.system }),
            json!({ "role": "user", "content": prompt }),
        ];
        let key = format!("xfusion:{}", p.handle);
        async move {
            match crate::agent_tools::run_turn(&p.llm, &p.llm.model, messages, None, &[], &key).await
            {
                Ok(outcome) => PanelAnswer {
                    handle: p.handle.clone(),
                    ok: true,
                    text: outcome.text,
                },
                Err(error) => PanelAnswer {
                    handle: p.handle.clone(),
                    ok: false,
                    text: error,
                },
            }
        }
    });
    futures_util::future::join_all(turns).await
}

/// A rebuttal round: each panelist gets its OWN prompt (the handoff differs
/// per agent — nobody receives itself). Same read-only key, same concurrency.
pub async fn panel_rebuttal(round: &[(&Panelist, String)]) -> Vec<PanelAnswer> {
    let turns = round.iter().map(|(p, prompt)| {
        let messages = vec![
            json!({ "role": "system", "content": p.system.clone() }),
            json!({ "role": "user", "content": prompt.clone() }),
        ];
        let key = format!("xfusion:{}", p.handle);
        async move {
            match crate::agent_tools::run_turn(&p.llm, &p.llm.model, messages, None, &[], &key).await
            {
                Ok(outcome) => PanelAnswer {
                    handle: p.handle.clone(),
                    ok: true,
                    text: outcome.text,
                },
                Err(error) => PanelAnswer {
                    handle: p.handle.clone(),
                    ok: false,
                    text: error,
                },
            }
        }
    });
    futures_util::future::join_all(turns).await
}

/// The first line of an answer shaped `POSITION: <one sentence>`, normalised
/// for comparison. The debate's convergence test compares each agent's
/// position to ITS OWN previous round — mechanical, no judge.
pub fn position_of(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| {
            let upper = line.to_ascii_uppercase();
            upper.starts_with("POSITION:") || upper.starts_with("**POSITION")
        })
        .and_then(|line| line.split_once(':'))
        .map(|(_, rest)| {
            rest.trim_matches(['*', ' '])
                .to_ascii_lowercase()
        })
}

/// True when every agent that answered holds the same position it held last
/// round. Rounds after this point are agents reinforcing each other, which
/// the fusion-harness demo showed is pure cost.
pub fn converged(previous: &[PanelAnswer], current: &[PanelAnswer]) -> bool {
    if previous.is_empty() {
        return false;
    }
    current.iter().filter(|a| a.ok).all(|now| {
        previous
            .iter()
            .find(|before| before.handle == now.handle && before.ok)
            .and_then(|before| {
                Some((position_of(&before.text)?, position_of(&now.text)?))
            })
            .is_some_and(|(before, now)| before == now)
    })
}

/// The cross-handoff block for a debate round: every OTHER agent's complete
/// prior answer, labelled by TAG only. Never the model, never the runtime
/// (XNAUT-236) — an agent that knows who it argues with postures instead of
/// answering.
pub fn handoff_for(handle: &str, answers: &[PanelAnswer]) -> String {
    answers
        .iter()
        .filter(|a| a.handle != handle && a.ok)
        .map(|a| format!("## [@{}] — prior position\n{}", a.handle, a.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(handle: &str, text: &str) -> PanelAnswer {
        PanelAnswer {
            handle: handle.into(),
            ok: true,
            text: text.into(),
        }
    }

    #[test]
    fn a_position_line_is_extracted_and_normalised() {
        assert_eq!(
            position_of("POSITION: Reject the claim.\nmore text"),
            Some("reject the claim.".into())
        );
        assert_eq!(
            position_of("**Position:** Hold.\n"),
            Some("hold.".into())
        );
        assert_eq!(position_of("no position here"), None);
    }

    #[test]
    fn unchanged_positions_converge_and_a_move_does_not() {
        let round1 = vec![
            answer("a", "POSITION: reject\nwhy"),
            answer("b", "POSITION: reject\nother why"),
        ];
        let round2_same = vec![
            answer("a", "POSITION: reject\nrestated"),
            answer("b", "POSITION: reject\nreinforced"),
        ];
        let round2_moved = vec![
            answer("a", "POSITION: accept\nnew evidence"),
            answer("b", "POSITION: reject\nheld"),
        ];
        assert!(converged(&round1, &round2_same));
        assert!(!converged(&round1, &round2_moved));
        assert!(!converged(&[], &round1), "round one never converges");
    }

    #[test]
    fn the_handoff_is_tags_only_and_excludes_self() {
        let answers = vec![answer("a", "POSITION: x"), answer("b", "POSITION: y")];
        let block = handoff_for("a", &answers);
        assert!(block.contains("[@b]"));
        assert!(!block.contains("[@a]"), "an agent never receives itself");
    }
}
