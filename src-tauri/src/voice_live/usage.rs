//! Local voice accounting. Only IDs, durations, word counts and rates are stored;
//! never transcripts, audio or credentials. Provider snapshots replace, never add.
use super::{Config, Handle};
use crate::state::AppState;
use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::{Duration, Instant}};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub model: String,
    pub month: String,
    pub seconds: f64,
    pub words: u64,
    pub rate_per_minute: Option<f64>,
    pub finalized: bool,
}
impl Record {
    fn cost(&self) -> Option<f64> { self.rate_per_minute.map(|r| self.seconds / 60.0 * r) }
}

pub(super) struct Meter {
    record: Record,
    anchor: Option<Instant>,
    in_word: bool,
    started: bool,
}
impl Meter {
    pub fn new(id: &str, model: &str) -> Self {
        Self { record: Record { id: id.into(), model: model.into(), month: Local::now().format("%Y-%m").to_string(),
            seconds: 0.0, words: 0, rate_per_minute: (model == "gpt-live-1").then_some(0.05), finalized: false },
            anchor: None, in_word: false, started: false }
    }
    pub fn observe(&mut self, value: &serde_json::Value) {
        match value["type"].as_str().unwrap_or("") {
            "session.started" if !self.started => { self.started = true; self.anchor = Some(Instant::now()); }
            "session.usage.updated" | "session.closed" => {
                if let Some(seconds) = value["usage"]["seconds"].as_f64().filter(|s| s.is_finite() && *s >= 0.0) {
                    self.started = true;
                    // Cumulative snapshots are authoritative, including corrections.
                    self.record.seconds = seconds;
                    self.record.finalized = value["type"] == "session.closed";
                    self.anchor = if self.record.finalized { None } else { Some(Instant::now()) };
                }
            }
            "session.input_transcript.delta" => {
                // Count streaming words without retaining speech. A word split across
                // two deltas counts once; punctuation alone does not count.
                for c in value["delta"].as_str().unwrap_or("").chars() {
                    if c.is_whitespace() { self.in_word = false; }
                    else if c.is_alphanumeric() && !self.in_word { self.record.words += 1; self.in_word = true; }
                }
            }
            "session.input_transcript.done" | "session.delegation.created" => self.in_word = false,
            _ => {}
        }
    }
    pub fn snapshot(&self) -> Option<Record> {
        if !self.started { return None; }
        let mut r = self.record.clone();
        r.seconds += self.anchor.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
        Some(r)
    }
    pub fn stop(&mut self) {
        if let Some(r) = self.snapshot() { self.record = r; }
        self.anchor = None;
    }
}

fn database(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|_| "Cannot create voice usage directory")?; }
    let db = Connection::open(path).map_err(|_| "Cannot open voice usage history")?;
    db.busy_timeout(Duration::from_secs(2)).map_err(|_| "Cannot configure voice usage history")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS voice_usage (id TEXT PRIMARY KEY, month TEXT NOT NULL, record TEXT NOT NULL, started_at INTEGER NOT NULL DEFAULT (unixepoch()));")
        .map_err(|_| "Cannot prepare voice usage history")?;
    Ok(db)
}
fn path() -> Result<std::path::PathBuf, String> {
    Ok(dirs::data_dir().ok_or("No app data directory")?.join("xnaut/voice-usage.sqlite3"))
}
fn save_to(db: &Connection, r: &Record) -> Result<(), String> {
    let json = serde_json::to_string(r).map_err(|_| "Cannot encode voice usage")?;
    db.execute("INSERT INTO voice_usage(id,month,record) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET record=excluded.record,month=excluded.month", params![r.id,r.month,json])
        .map_err(|_| "Cannot save voice usage")?;
    Ok(())
}
pub(super) async fn checkpoint(handle: &Handle, stop: bool) {
    let mut meter = handle.usage.lock().await;
    if stop { meter.stop(); }
    if let Some(r) = meter.snapshot() {
        if let Err(e) = path().and_then(|p| database(&p)).and_then(|db| save_to(&db, &r)) {
            eprintln!("[voice usage] {e}");
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    configured: bool,
    session: Option<Record>,
    active: bool,
    month: String,
    month_cost_usd: f64,
    month_words: u64,
    unconfirmed_sessions: usize,
    unpriced_sessions: usize,
}
fn summarize(db: &Connection, month: String, current: Option<Record>, active: bool) -> Result<Summary, String> {
    let mut query = db.prepare("SELECT record FROM voice_usage ORDER BY started_at DESC,rowid DESC").map_err(|_| "Cannot read voice usage")?;
    let rows = query.query_map([], |row| row.get::<_, String>(0)).map_err(|_| "Cannot read voice usage")?;
    let mut result = Summary { configured: true, session: current, active, month: month.clone(), month_cost_usd: 0.0, month_words: 0, unconfirmed_sessions: 0, unpriced_sessions: 0 };
    for json in rows {
        let r: Record = serde_json::from_str(&json.map_err(|_| "Cannot read voice usage")?).map_err(|_| "Voice usage history is invalid")?;
        if result.session.is_none() { result.session = Some(r.clone()); }
        if r.month != month { continue; }
        result.month_words += r.words;
        if let Some(cost) = r.cost() { result.month_cost_usd += cost; } else { result.unpriced_sessions += 1; }
        if !r.finalized { result.unconfirmed_sessions += 1; }
    }
    Ok(result)
}

#[tauri::command]
pub async fn voice_live_usage(state: tauri::State<'_, AppState>) -> Result<Summary, String> {
    let month = Local::now().format("%Y-%m").to_string();
    if Config::load().is_err() {
        return Ok(Summary { configured: false, session: None, active: false, month, month_cost_usd: 0.0, month_words: 0, unconfirmed_sessions: 0, unpriced_sessions: 0 });
    }
    let handle = state.live_voice.lock().await.clone();
    let mut current = None;
    let active = handle.as_ref().is_some_and(|h| !h.is_closed());
    if let Some(handle) = handle {
        let meter = handle.usage.lock().await;
        current = meter.snapshot();
        // Serialize this write with finalization; an older UI snapshot must
        // never overwrite a final provider report.
        if let Some(r) = &current { save_to(&database(&path()?)?, r)?; }
    }
    let db = database(&path()?)?;
    summarize(&db, month, current, active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn cumulative_final_snapshots_replace_even_when_duplicate_or_corrected() {
        let mut m = Meter::new("test", "gpt-live-1");
        assert!(m.snapshot().is_none());
        for seconds in [12.0,15.0,15.0] { m.observe(&json!({"type":"session.usage.updated","usage":{"seconds":seconds}})); }
        m.observe(&json!({"type":"session.closed","usage":{"seconds":14.5}}));
        m.stop(); let r=m.snapshot().unwrap(); assert_eq!(r.seconds,14.5); assert!(r.finalized);
        assert!((r.cost().unwrap()-14.5/1200.0).abs()<1e-9);
    }
    #[test]
    fn elapsed_includes_quiet_time_and_failed_finalization_stays_unconfirmed() {
        let mut m=Meter::new("test","gpt-live-1"); m.started=true; m.anchor=Some(Instant::now()-Duration::from_secs(90));
        m.observe(&json!({"type":"session.closed","usage":{"seconds":-1}}));
        m.stop(); let r=m.snapshot().unwrap(); assert!(r.seconds>=90.0 && r.seconds<91.0); assert!(!r.finalized);
        assert_eq!(m.snapshot().unwrap().seconds,r.seconds);
    }
    #[test]
    fn transcript_fragments_count_words_without_counting_replies_or_typed_input() {
        let mut m=Meter::new("test","unknown-model");
        m.observe(&json!({"type":"session.started"}));
        for delta in ["Hel","lo ","world", "!"] { m.observe(&json!({"type":"session.input_transcript.delta","delta":delta})); }
        m.observe(&json!({"type":"session.output_transcript.delta","delta":"Never count these words"}));
        m.observe(&json!({"type":"session.delegation.created"}));
        m.observe(&json!({"type":"session.input_transcript.delta","delta":"Again"}));
        assert_eq!(m.snapshot().unwrap().words,3);assert!(m.snapshot().unwrap().cost().is_none());
    }
    #[test]
    fn ledger_survives_reopen_deduplicates_and_keeps_months_separate() {
        let dir=std::env::temp_dir().join(format!("voice-cost-{}",uuid::Uuid::new_v4()));
        let p=dir.join("usage.sqlite3");
        let mut r=Record{id:"a".into(),model:"gpt-live-1".into(),month:"2026-09".into(),seconds:12.0,words:149,rate_per_minute:Some(0.05),finalized:false};
        {let db=database(&p).unwrap();save_to(&db,&r).unwrap();r.seconds=90.0;r.finalized=true;save_to(&db,&r).unwrap();save_to(&db,&r).unwrap();r.id="old".into();r.month="2026-08".into();save_to(&db,&r).unwrap();}
        {let db=database(&p).unwrap();let s=summarize(&db,"2026-09".into(),None,false).unwrap();assert!((s.month_cost_usd-0.075).abs()<1e-9);assert_eq!(s.month_words,149);assert_eq!(s.unconfirmed_sessions,0);}
        std::fs::remove_dir_all(dir).unwrap();
    }
}
