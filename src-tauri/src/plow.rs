// Read-only bridge into Plow (AgenticKMU) — deliberately shells out to `docker exec
// agentic-kmu-postgres psql` with row_to_json output instead of adding a Postgres crate:
// local-only, read-only, zero new deps, and the transport is isolated here so a later
// swap to Plow's REST API touches only this module.

use serde::Serialize;
use serde_json::json;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct PlowOpportunity {
    pub id: String,
    pub name: String,
    pub stage: String,
    pub value_chf: Option<f64>,
    pub probability_pct: Option<f64>,
    pub expected_close_date: Option<String>,
    pub description: Option<String>,
    pub source: Option<String>,
    /// companies.name via LEFT JOIN on primary_company_id.
    pub company: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlowContact {
    pub name: String,
    pub email: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlowOpportunityDetail {
    #[serde(flatten)]
    pub opportunity: PlowOpportunity,
    /// Contacts of the opportunity's primary company.
    pub contacts: Vec<PlowContact>,
}

/// Strict UUID shape check — the opportunity id is the ONLY dynamic value ever
/// spliced into SQL in this module; everything else is a static string.
fn is_valid_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// Numeric columns come back from row_to_json as either JSON numbers or strings
/// (`numeric` is rendered as a quoted string in some configurations) — accept both.
fn json_num(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
}

fn json_str(v: &serde_json::Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Runs `inner` wrapped in row_to_json inside the Plow container; one stdout
/// line per row, each parsed as a JSON object.
fn psql_json(inner: &str) -> Result<Vec<serde_json::Value>, String> {
    let sql = format!("SELECT row_to_json(t) FROM ({inner}) t;");
    let output = Command::new("docker")
        .args([
            "exec",
            "agentic-kmu-postgres",
            "psql",
            "-U",
            "agentic_kmu",
            "-d",
            "agentic_kmu",
            "-t",
            "-A",
            "-c",
            &sql,
        ])
        .output()
        .map_err(|e| {
            format!(
                "Plow database not reachable — is the agentic-kmu-postgres container running? ({e})"
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let excerpt: String = stderr.trim().chars().take(200).collect();
        return Err(format!(
            "Plow database not reachable — is the agentic-kmu-postgres container running? ({excerpt})"
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row = serde_json::from_str(line)
            .map_err(|e| format!("failed to parse Plow row as JSON: {e}"))?;
        rows.push(row);
    }
    Ok(rows)
}

fn opportunity_from_row(row: &serde_json::Value) -> PlowOpportunity {
    PlowOpportunity {
        id: row["id"].as_str().unwrap_or_default().to_string(),
        name: row["name"].as_str().unwrap_or_default().to_string(),
        stage: row["stage"].as_str().unwrap_or_default().to_string(),
        value_chf: json_num(&row["value_chf"]),
        probability_pct: json_num(&row["probability_pct"]),
        expected_close_date: json_str(&row["expected_close_date"]),
        description: json_str(&row["description"]),
        source: json_str(&row["source"]),
        company: json_str(&row["company"]),
    }
}

const OPPORTUNITY_SELECT: &str = "SELECT o.id, o.name, o.stage, o.value_chf, \
     o.probability_pct, o.expected_close_date, o.description, o.source, \
     c.name AS company \
     FROM opportunities o \
     LEFT JOIN companies c ON c.id = o.primary_company_id";

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn plow_list_opportunities() -> Result<Vec<PlowOpportunity>, String> {
    let rows = psql_json(&format!(
        "{OPPORTUNITY_SELECT} \
         WHERE o.stage NOT IN ('won','lost') \
         ORDER BY o.created_at DESC LIMIT 50"
    ))?;
    Ok(rows.iter().map(opportunity_from_row).collect())
}

#[tauri::command]
pub fn plow_get_opportunity(id: String) -> Result<PlowOpportunityDetail, String> {
    if !is_valid_uuid(&id) {
        return Err("invalid opportunity id".into());
    }
    let rows = psql_json(&format!("{OPPORTUNITY_SELECT} WHERE o.id = '{id}'"))?;
    let row = rows.first().ok_or("opportunity not found")?;
    let opportunity = opportunity_from_row(row);

    // Contacts of the primary company — still only the validated uuid spliced.
    // Name prefers a non-empty display_name, else "first last".
    let contact_rows = psql_json(&format!(
        "SELECT coalesce(nullif(trim(display_name), ''), \
                         trim(concat(first_name, ' ', last_name))) AS name, \
                email, job_title AS role \
         FROM contacts \
         WHERE company_id = (SELECT primary_company_id FROM opportunities WHERE id = '{id}')"
    ))?;
    let contacts = contact_rows
        .iter()
        .map(|r| PlowContact {
            name: r["name"].as_str().unwrap_or_default().to_string(),
            email: json_str(&r["email"]),
            role: json_str(&r["role"]),
        })
        .collect();

    Ok(PlowOpportunityDetail {
        opportunity,
        contacts,
    })
}

#[tauri::command]
pub fn plow_status(id: String) -> Result<serde_json::Value, String> {
    if !is_valid_uuid(&id) {
        return Err("invalid opportunity id".into());
    }
    let rows = psql_json(&format!(
        "SELECT stage, value_chf, probability_pct FROM opportunities WHERE id = '{id}'"
    ))?;
    let row = rows.first().ok_or("opportunity not found")?;
    Ok(json!({
        "stage": row["stage"].as_str().unwrap_or_default(),
        "value_chf": json_num(&row["value_chf"]),
        "probability_pct": json_num(&row["probability_pct"]),
    }))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_uuid_passes() {
        assert!(is_valid_uuid("a1b2c3d4-e5f6-7a8b-9c0d-e1f2a3b4c5d6"));
    }

    #[test]
    fn injection_attempt_fails() {
        assert!(!is_valid_uuid("x; DROP TABLE opportunities; --"));
        // right length, wrong chars/positions
        assert!(!is_valid_uuid("a1b2c3d4'e5f6-7a8b-9c0d-e1f2a3b4c5d6"));
    }

    #[test]
    fn short_string_fails() {
        assert!(!is_valid_uuid("abc-123"));
        assert!(!is_valid_uuid(""));
    }

    #[test]
    fn numeric_parser_accepts_string_and_number() {
        let as_string = json!({ "value_chf": "5000.00" });
        assert_eq!(json_num(&as_string["value_chf"]), Some(5000.0));

        let as_number = json!({ "value_chf": 5000 });
        assert_eq!(json_num(&as_number["value_chf"]), Some(5000.0));

        let as_null = json!({ "value_chf": null });
        assert_eq!(json_num(&as_null["value_chf"]), None);
    }
}
