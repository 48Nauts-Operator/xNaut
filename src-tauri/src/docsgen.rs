// PM Space v1.7 client-document generation. Seeds user-editable German markdown
// templates under ~/.config/xnaut/templates/ and fills them via the configured LLM.

use std::path::PathBuf;

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn templates_dir() -> PathBuf {
    config_dir().join("templates")
}

// ─── Seed templates ──────────────────────────────────────────────────────────
// {{placeholder}} markers are hints for the LLM, not string-substituted.

const TEMPLATE_OFFERTE: &str = r#"# Offerte

**48Nauts** · {{date}}

**An:** {{client_company}}

**Betreff:** Offerte — {{project_name}}

## Ausgangslage

{{scope}}

## Leistungsumfang

- (Leistungen aus dem Scope ableiten, als konkrete Arbeitspakete formulieren)

## Aufwand & Kosten

| Position | Aufwand | Kosten |
| --- | --- | --- |
| (Arbeitspaket) | (h) | (CHF) |

Stundensatz: {{rate_chf_per_hour}} CHF/h
Gesamtbetrag (Richtofferte): {{offer_amount_chf}} CHF, exkl. MwSt.

## Annahmen & Abgrenzung

- (Was ist enthalten, was explizit nicht)

## Termine

- (Grober Zeitplan: Start, Meilensteine, Abschluss)

## Gültigkeit

Diese Offerte ist 30 Tage ab Ausstellungsdatum gültig.

---

48Nauts · hello@48nauts.com
"#;

const TEMPLATE_SLA: &str = r#"# Service Level Agreement (SLA)

## Vertragsparteien

- Auftraggeber: {{client_company}}
- Auftragnehmer: 48Nauts

## Leistungsbeschreibung

{{scope}}

## Supportzeiten

- (z. B. Mo–Fr, 09:00–17:00 Uhr, ausgenommen schweizerische Feiertage)

## Reaktionszeiten

| Priorität | Beschreibung | Reaktionszeit |
| --- | --- | --- |
| P1 | Totalausfall, kein Workaround | (z. B. 4 Stunden) |
| P2 | Wesentliche Einschränkung, Workaround vorhanden | (z. B. 1 Arbeitstag) |
| P3 | Geringe Einschränkung, Anfrage | (z. B. 3 Arbeitstage) |

## Verfügbarkeit

- (Zugesicherte Verfügbarkeit, Messmethode, Ausnahmen)

## Wartungsfenster

- (Geplante Wartungsfenster, Ankündigungsfrist)

## Vergütung

Stundensatz: {{rate_chf_per_hour}} CHF/h (Aufwände ausserhalb der Pauschale)

## Laufzeit & Kündigung

- (Vertragsbeginn, Mindestlaufzeit, Kündigungsfrist)
"#;

const TEMPLATE_PLAN: &str = r#"# Projektplan — {{project_name}}

**48Nauts** · {{date}} · {{client_company}}

## Ziel & Umfang

{{scope}}

## Vorgehen / Phasen

| Phase | Ergebnis | Aufwand (h) |
| --- | --- | --- |
| (z. B. Discovery) | (Ergebnis dieser Phase) | (h) |
| (z. B. Architektur & Setup) | | |
| (z. B. Umsetzung) | | |
| (z. B. Test & Abnahme) | | |

## Meilensteine

- (Meilenstein — Datum)

## Lieferobjekte

- (Was am Ende übergeben wird)

## Annahmen & Abhängigkeiten

- (Voraussetzungen, Zulieferungen des Kunden, externe Abhängigkeiten)

## Risiken

- (Risiko — Massnahme)

## Aufwand & Budget

Stundensatz: {{rate_chf_per_hour}} CHF/h
Richtbudget: {{offer_amount_chf}} CHF, exkl. MwSt.
"#;

const TEMPLATE_ARCHITEKTUR: &str = r#"# Architekturdokumentation — {{project_name}}

## Kontext & Ziel

{{scope}}

## Systemübersicht

- (Gesamtbild: Hauptsysteme, Akteure, grobe Datenflüsse)

## Komponenten

- (Pro Komponente: Zweck, Technologie, Verantwortlichkeit)

## Schnittstellen & Integrationen

- (APIs, Protokolle, Drittsysteme, Authentisierung)

## Datenhaltung & Datenschutz

- (Speicherorte, Aufbewahrung, Verschlüsselung)
- Hinweis: Bei Medizinaldaten bzw. besonders schützenswerten Personendaten
  gelten die Anforderungen des DSG — wo relevant explizit adressieren.

## Risiken

- (Technische und organisatorische Risiken inkl. Massnahmen)

## Roadmap

- (Ausbaustufen, nächste Schritte)
"#;

const TEMPLATE_MEETING_NOTES: &str = r#"# Meeting Notes — {{project_name}}

**Datum:** {{date}}

**Teilnehmer:** {{contacts}}

## Themen

- (Besprochene Punkte)

## Entscheide

- (Getroffene Entscheidungen)

## Nächste Schritte

| Schritt | Verantwortlich | Termin |
| --- | --- | --- |
| (Aufgabe) | (Name) | (Datum) |
"#;

/// Creates the templates dir and seeds the default templates. Existing files
/// are never overwritten — user edits survive.
pub fn ensure_templates() -> Result<PathBuf, String> {
    let dir = templates_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let seeds: [(&str, &str); 5] = [
        ("plan.md", TEMPLATE_PLAN),
        ("offerte.md", TEMPLATE_OFFERTE),
        ("sla.md", TEMPLATE_SLA),
        ("architektur.md", TEMPLATE_ARCHITEKTUR),
        ("meeting-notes.md", TEMPLATE_MEETING_NOTES),
    ];
    for (name, body) in seeds {
        let path = dir.join(name);
        if !path.exists() {
            std::fs::write(&path, body)
                .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        }
    }
    Ok(dir)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Template names are plain stems — alphanumeric + dash only, so they can
/// never traverse out of the templates dir.
fn valid_template_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Strips a surrounding markdown code fence (``` or ```markdown) if the whole
/// reply is wrapped in one. Anything else passes through untouched.
fn strip_code_fence(s: &str) -> &str {
    let t = s.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t;
    };
    let Some(rest) = rest.strip_suffix("```") else {
        return t;
    };
    // Drop the fence info string ("markdown", "md", or empty) up to the first
    // newline; a fenced block with no newline isn't a wrapped document.
    match rest.find('\n') {
        Some(i) => rest[i + 1..].trim(),
        None => t,
    }
}

/// Picks `<stem>-<date>.md`, falling back to `-2` … `-9` suffixes when the
/// exists-predicate says the name is taken. Pure so it's testable.
fn unique_doc_filename(
    stem: &str,
    date: &str,
    exists: impl Fn(&str) -> bool,
) -> Result<String, String> {
    let base = format!("{stem}-{date}.md");
    if !exists(&base) {
        return Ok(base);
    }
    for n in 2..=9 {
        let candidate = format!("{stem}-{date}-{n}.md");
        if !exists(&candidate) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "too many documents named {stem}-{date}*.md — clean up the client/ folder"
    ))
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn docgen_templates() -> Result<Vec<String>, String> {
    let dir = ensure_templates()?;
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| format!("failed to read {}: {e}", dir.display()))?;
    let mut stems: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    stems.sort();
    Ok(stems)
}

#[tauri::command]
pub async fn docgen_generate(
    state: tauri::State<'_, crate::state::AppState>,
    task_id: String,
    template: String,
) -> Result<String, String> {
    // 1. Validate + load the template.
    if !valid_template_name(&template) {
        return Err("invalid template name (alphanumeric and dashes only)".into());
    }
    let dir = ensure_templates()?;
    let template_path = dir.join(format!("{template}.md"));
    if !template_path.exists() {
        return Err("unknown template".into());
    }
    let template_body = std::fs::read_to_string(&template_path)
        .map_err(|e| format!("failed to read {}: {e}", template_path.display()))?;

    // 2. Build the project context.
    let pm = crate::pm::load_pm_projects()
        .into_iter()
        .find(|p| p.task_id == task_id)
        .ok_or_else(|| "no PM entry".to_string())?;
    let task = crate::tasks::load_tasks()
        .into_iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| format!("unknown task: {task_id}"))?;
    let date = chrono::Local::now().format("%d.%m.%Y").to_string();

    let contacts: Vec<serde_json::Value> = pm
        .contacts
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "email": c.email,
                "role": c.role,
            })
        })
        .collect();
    let context = serde_json::json!({
        "client_company": pm.client_company,
        "project_name": task.name,
        "contacts": contacts,
        "scope": pm.scope,
        "rate_chf_per_hour": pm.rate_chf_per_hour,
        "offer_amount_chf": pm.offer_amount_chf,
        "date": date,
    });
    let context_pretty = serde_json::to_string_pretty(&context)
        .map_err(|e| format!("failed to serialize project context: {e}"))?;

    // 3. Ask the LLM to fill the template.
    let llm = state.settings.lock().await.llm.clone();
    let system = "Du bist Assistent von 48Nauts (Schweizer IT-Beratung). Fülle die folgende Dokumentvorlage vollständig aus. Verwende die Projektdaten. Antworte NUR mit dem fertigen Markdown-Dokument, ohne Kommentar.";
    let user = format!("{template_body}\n\n--- Projektdaten ---\n{context_pretty}");
    let reply = crate::chat::complete_oneshot(&llm, Some(system), &user).await?;

    // 4. Strip a surrounding code fence if the model wrapped its answer.
    let document = strip_code_fence(&reply);

    // 5. Write to <task.path>/client/<template>-<date>.md, suffixing on collision.
    let client_dir = PathBuf::from(&task.path).join("client");
    std::fs::create_dir_all(&client_dir)
        .map_err(|e| format!("failed to create {}: {e}", client_dir.display()))?;
    let file_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let filename = unique_doc_filename(&template, &file_date, |name| {
        client_dir.join(name).exists()
    })?;
    let out_path = client_dir.join(&filename);
    std::fs::write(&out_path, document)
        .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;

    Ok(out_path.to_string_lossy().into_owned())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_name_validation_accepts_plain_stems() {
        assert!(valid_template_name("offerte"));
        assert!(valid_template_name("meeting-notes"));
        assert!(valid_template_name("sla2"));
    }

    #[test]
    fn template_name_validation_rejects_traversal_and_separators() {
        assert!(!valid_template_name("../x"));
        assert!(!valid_template_name("a/b"));
        assert!(!valid_template_name("a\\b"));
        assert!(!valid_template_name("a.b"));
        assert!(!valid_template_name(""));
    }

    #[test]
    fn strip_code_fence_unwraps_fenced_documents() {
        assert_eq!(strip_code_fence("```markdown\n# Doc\nbody\n```"), "# Doc\nbody");
        assert_eq!(strip_code_fence("```\n# Doc\n```"), "# Doc");
        assert_eq!(
            strip_code_fence("  ```md\n# Trimmed\n```  "),
            "# Trimmed"
        );
    }

    #[test]
    fn strip_code_fence_leaves_unfenced_and_partial_input_alone() {
        assert_eq!(strip_code_fence("# Plain doc"), "# Plain doc");
        // Opening fence without closing fence — pass through.
        assert_eq!(strip_code_fence("```\nno close"), "```\nno close");
        // Inner fences inside a normal document stay intact.
        let with_inner = "# Doc\n```rust\nfn x() {}\n```\nmore";
        assert_eq!(strip_code_fence(with_inner), with_inner);
    }

    #[test]
    fn unique_doc_filename_uses_base_name_when_free() {
        let name = unique_doc_filename("offerte", "2026-06-13", |_| false).unwrap();
        assert_eq!(name, "offerte-2026-06-13.md");
    }

    #[test]
    fn unique_doc_filename_suffixes_on_collision() {
        let taken = ["offerte-2026-06-13.md", "offerte-2026-06-13-2.md"];
        let name =
            unique_doc_filename("offerte", "2026-06-13", |n| taken.contains(&n)).unwrap();
        assert_eq!(name, "offerte-2026-06-13-3.md");
    }

    #[test]
    fn unique_doc_filename_errors_after_nine() {
        let err = unique_doc_filename("sla", "2026-06-13", |_| true).unwrap_err();
        assert!(err.contains("sla-2026-06-13"));
    }
}
