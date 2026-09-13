// The Researcher: NautFlow's one live line to the outside world (XNAUT-356).
//
// André asked whether the Council — a five-member debating room that is its own
// tool — should be embedded in xNAUT. It should not, and the reason is worth
// keeping next to the code: NautFlow IS a council already, run in sequence
// (Analyst, Architect, Security, then a Reviewer deliberately on a second
// provider). Putting a second council in front of it debates the same question
// twice out of the same knowledge. The single thing Council has that NautFlow
// does not is a member who looks OUTSIDE: a researcher on a search provider,
// answering with sources.
//
// So this module is that member and nothing else. No Council URL, no Council
// API, no import of its prompts: the Researcher here is an ordinary xNAUT agent
// profile (role `researcher`) whose provider happens to search the web, and
// this module is the narrow path from a NautFlow stage to it.
//
// Two commands rather than one on purpose. `research_status` answers from the
// profile store and settings alone, so a stage card can say who will do the
// research — or why nobody will — without spending a token or a second on the
// network. `research_brief` is the call that actually costs something.

use serde::{Deserialize, Serialize};

use crate::agent_profiles::AgentProfile;
use crate::ai::Citation;
use crate::settings::Settings;

/// The profile role NautFlow resolves the Researcher on, exactly as
/// `agent_profile_list` reports it. Same rule as every other persona since
/// XNAUT-355: the role is the contract, the handle is a name.
pub const RESEARCH_ROLE: &str = "researcher";

/// Who will do the research, or why nobody will.
///
/// `available: false` is an ANSWER, not an error: with no Perplexity key the
/// stage is supposed to run exactly as it did before and say on its card that
/// the Researcher was unavailable. Returning `Err` here would have made a
/// missing optional key look like a broken app.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchStatus {
    pub available: bool,
    /// Empty when available; otherwise a sentence a person can act on.
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

/// What the Researcher found, with the sources it stood on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchBrief {
    pub available: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub sources: Vec<Citation>,
}

impl ResearchBrief {
    fn unavailable(status: ResearchStatus) -> Self {
        Self {
            available: false,
            reason: status.reason,
            handle: status.handle,
            provider: status.provider,
            model: status.model,
            answer: String::new(),
            sources: Vec::new(),
        }
    }
}

/// The profile that holds the researcher role, if any holds it.
///
/// Case-insensitive on the role for the same reason the persona resolver is: a
/// profile edited by hand in the agent library may say `Researcher`, and a
/// capital letter is not a reason to lose the stage's only outside voice.
pub fn researcher_profile(profiles: &[AgentProfile]) -> Option<&AgentProfile> {
    profiles
        .iter()
        .find(|profile| profile.role.trim().eq_ignore_ascii_case(RESEARCH_ROLE))
}

/// Where an OpenAI-compatible chat completion lives for a search provider.
///
/// `None` means "this provider is not one we know how to call for research",
/// which is a different answer from "no key" and is reported as such. Kept
/// beside the research code rather than reaching into `commands.rs`: that
/// mapping is private to `ask_ai` and covers providers research has no use for.
pub fn chat_completions_url(provider: &str) -> Option<String> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "perplexity" => Some("https://api.perplexity.ai/chat/completions".to_string()),
        "openrouter" => Some("https://openrouter.ai/api/v1/chat/completions".to_string()),
        _ => None,
    }
}

/// The key for a provider, from the settings file the app already writes.
///
/// `llm_providers` first — that is where the AI settings screen mirrors every
/// key it holds, Perplexity included — then the primary `llm` block when it
/// names this same provider. A blank string is treated as absent, because a
/// provider row saved with an empty field is exactly the "no key" case.
pub fn provider_api_key(settings: &Settings, provider: &str) -> Option<String> {
    let want = provider.trim().to_ascii_lowercase();
    settings
        .llm_providers
        .iter()
        .find(|entry| entry.name.trim().to_ascii_lowercase() == want)
        .and_then(|entry| entry.api_key.clone())
        .or_else(|| {
            (settings.llm.provider.trim().to_ascii_lowercase() == want)
                .then(|| settings.llm.api_key.clone())
                .flatten()
        })
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Resolves who researches, from the two stores that decide it.
pub fn status_from(profiles: &[AgentProfile], settings: &Settings) -> ResearchStatus {
    let Some(profile) = researcher_profile(profiles) else {
        return ResearchStatus {
            available: false,
            reason: "no researcher profile — add one in the Agent Library".to_string(),
            ..ResearchStatus::default()
        };
    };
    let base = ResearchStatus {
        available: false,
        reason: String::new(),
        handle: profile.handle.clone(),
        provider: profile.provider.clone(),
        model: profile.model.clone(),
    };
    if chat_completions_url(&profile.provider).is_none() {
        return ResearchStatus {
            reason: format!(
                "@{} researches on {}, which xNAUT cannot call for research",
                profile.handle, profile.provider
            ),
            ..base
        };
    }
    if provider_api_key(settings, &profile.provider).is_none() {
        return ResearchStatus {
            reason: format!(
                "no {} key — add one in Settings › AI",
                display_provider(&profile.provider)
            ),
            ..base
        };
    }
    ResearchStatus {
        available: true,
        ..base
    }
}

/// "perplexity" is a brand; the card says Perplexity.
fn display_provider(provider: &str) -> String {
    let provider = provider.trim();
    match provider.to_ascii_lowercase().as_str() {
        "perplexity" => "Perplexity".to_string(),
        "openrouter" => "OpenRouter".to_string(),
        _ => provider.to_string(),
    }
}

/// What the Researcher is told it is for.
///
/// Deliberately narrow: the stage agent writes the document, so an answer that
/// starts drafting the PRD is worse than useless. Sources are demanded in the
/// prose as well as taken from the response body, because a model that names
/// its source inline gives the stage something to quote.
fn research_system_prompt() -> String {
    "You are a research analyst answering for another agent, not for a human reader. \
Search before you answer. Report what is actually published: precedent, prior art, comparable products, market and competitor context, standards, and dated facts. \
Be concise and specific — at most 400 words, organised as short bullet points, each carrying the source it came from. \
Prefer primary and recent sources, and say plainly where the evidence is thin, contradictory, or missing. \
Never invent a source, never pad, and do not write the document the other agent is writing."
        .to_string()
}

/// Who is available to research, without touching the network.
#[tauri::command]
pub fn research_status() -> Result<ResearchStatus, String> {
    let profiles = crate::agent_profiles::agent_profile_list()?;
    Ok(status_from(&profiles, &crate::settings::load_or_default()))
}

/// Ask the Researcher a question and hand back the answer with its sources.
///
/// `Err` is reserved for a genuinely broken call — an unreadable profile store,
/// a provider that refused the request. "Nobody holds the role" and "no key" are
/// `available: false` with a reason, so a NautFlow stage can carry on running
/// exactly as it did before the Researcher existed.
#[tauri::command]
pub async fn research_brief(
    question: String,
    context: Option<String>,
) -> Result<ResearchBrief, String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("a research brief needs a question".to_string());
    }
    let profiles = crate::agent_profiles::agent_profile_list()?;
    let settings = crate::settings::load_or_default();
    let status = status_from(&profiles, &settings);
    if !status.available {
        return Ok(ResearchBrief::unavailable(status));
    }
    let Some(url) = chat_completions_url(&status.provider) else {
        return Ok(ResearchBrief::unavailable(status));
    };
    let Some(api_key) = provider_api_key(&settings, &status.provider) else {
        return Ok(ResearchBrief::unavailable(status));
    };

    let client = crate::ai::AiClient::new(crate::ai::AiConfig {
        provider: crate::ai::AiProvider::Custom,
        api_key,
        model: status.model.clone(),
        base_url: Some(url),
    });
    let response = client
        .ask(crate::ai::AiRequest {
            prompt: question,
            context,
            terminal_output: None,
            system_info: None,
            system: Some(research_system_prompt()),
        })
        .await
        .map_err(|error| format!("{} research failed: {error}", status.provider))?;

    Ok(ResearchBrief {
        available: true,
        reason: String::new(),
        handle: status.handle,
        provider: status.provider,
        model: status.model,
        answer: response.response,
        sources: response.citations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{LlmProviderSettings, LlmSettings};

    fn profile(handle: &str, role: &str, provider: &str, model: &str) -> AgentProfile {
        AgentProfile {
            handle: handle.to_string(),
            display_name: handle.to_string(),
            tagline: String::new(),
            purpose: String::new(),
            runtime_id: "claude".to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            chat_model: String::new(),
            reasoning_effort: String::new(),
            execution: crate::agent_profiles::AgentExecution::Local,
            role: role.to_string(),
            capabilities: vec![],
            notifications: true,
            accent_color: "#ffffff".to_string(),
            policy: crate::policy::AgentPolicy::default(),
            default_project: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn settings_with(providers: Vec<LlmProviderSettings>, llm: LlmSettings) -> Settings {
        Settings {
            llm,
            llm_providers: providers,
            ..Settings::default()
        }
    }

    fn keyed() -> Settings {
        settings_with(
            vec![LlmProviderSettings {
                name: "perplexity".to_string(),
                endpoint: "https://api.perplexity.ai".to_string(),
                api_key: Some("pplx-test".to_string()),
                enabled: true,
            }],
            LlmSettings::default(),
        )
    }

    /// The role is the contract. An Analyst is not a fallback researcher, and a
    /// hand-edited `Researcher` still counts.
    #[test]
    fn the_researcher_is_found_by_role_whatever_its_case() {
        let profiles = vec![
            profile("analyst", "analysis", "anthropic", "claude-sonnet-5"),
            profile("digger", "Researcher", "perplexity", "sonar-pro"),
        ];
        assert_eq!(
            researcher_profile(&profiles).map(|p| p.handle.as_str()),
            Some("digger")
        );
        assert!(researcher_profile(&profiles[..1]).is_none());
    }

    /// No profile, no key, and wrong provider are three different answers, and
    /// each one has to say which it is: the stage card shows this sentence, and
    /// "unavailable" with no cause is a support ticket.
    #[test]
    fn an_unavailable_researcher_says_which_thing_is_missing() {
        let none = status_from(&[], &keyed());
        assert!(!none.available);
        assert!(none.reason.contains("Agent Library"), "{}", none.reason);

        let keyless = status_from(
            &[profile(
                "researcher",
                "researcher",
                "perplexity",
                "sonar-pro",
            )],
            &Settings::default(),
        );
        assert!(!keyless.available);
        assert!(
            keyless.reason.contains("Perplexity key"),
            "{}",
            keyless.reason
        );
        // Even unavailable, the card can name who WOULD research.
        assert_eq!(keyless.handle, "researcher");

        let offline = status_from(
            &[profile("researcher", "researcher", "ollama", "llama3")],
            &keyed(),
        );
        assert!(!offline.available);
        assert!(offline.reason.contains("ollama"), "{}", offline.reason);
    }

    #[test]
    fn a_seeded_researcher_with_a_key_is_available() {
        let status = status_from(
            &[profile(
                "researcher",
                "researcher",
                "perplexity",
                "sonar-pro",
            )],
            &keyed(),
        );
        assert_eq!(
            status,
            ResearchStatus {
                available: true,
                reason: String::new(),
                handle: "researcher".to_string(),
                provider: "perplexity".to_string(),
                model: "sonar-pro".to_string(),
            }
        );
    }

    /// The AI settings screen writes the key into `llm_providers`; an older
    /// install that only set the primary `llm` block still has a usable key, and
    /// a row saved with an empty field is the "no key" case, not a key.
    #[test]
    fn the_key_is_read_from_either_place_and_blank_means_absent() {
        assert_eq!(
            provider_api_key(&keyed(), "PerPlexity").as_deref(),
            Some("pplx-test")
        );
        let primary = settings_with(
            vec![],
            LlmSettings {
                provider: "perplexity".to_string(),
                api_key: Some("pplx-primary".to_string()),
                ..LlmSettings::default()
            },
        );
        assert_eq!(
            provider_api_key(&primary, "perplexity").as_deref(),
            Some("pplx-primary")
        );
        let blank = settings_with(
            vec![LlmProviderSettings {
                name: "perplexity".to_string(),
                endpoint: String::new(),
                api_key: Some("   ".to_string()),
                enabled: true,
            }],
            LlmSettings::default(),
        );
        assert_eq!(provider_api_key(&blank, "perplexity"), None);
    }

    /// Research goes to a SEARCH provider. Council is a standalone tool and
    /// stays one — there is no host here to point at it — and a chat provider
    /// with no live index is not a research endpoint either.
    #[test]
    fn research_only_knows_endpoints_that_search() {
        assert_eq!(
            chat_completions_url("perplexity").as_deref(),
            Some("https://api.perplexity.ai/chat/completions")
        );
        assert_eq!(chat_completions_url(" OpenRouter ").is_some(), true);
        assert_eq!(chat_completions_url("anthropic"), None);
        assert_eq!(chat_completions_url(""), None);
    }
}
