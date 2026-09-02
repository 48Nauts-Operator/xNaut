// The clocks nobody could read.
//
// /api/control/doctor reported eight healthy-looking facts (version, window,
// session counts, verify records, the switches) while every one of them stayed
// healthy with the app's periodic work dead behind it. On 2026-09-02 answering
// "has the sweep ticked recently?" cost an hour: ssh to the machine, read a
// JSONL ledger, compare its timestamps against a `ps` clock, and get the
// comparison wrong, because the ledger is UTC and `ps` prints local. The
// conclusion drawn was confident and false.
//
// The ledger cannot answer it at all, in either direction. The sweep only
// writes a line when it DOES something, so "ran three minutes ago and the board
// was empty" and "has not run since the app started" leave the same trace,
// which is none. That indistinguishability is the bug; the timezone was only
// how it got noticed.
//
// So each periodic loop stamps a heartbeat and doctor reports it. Two rules
// come out of the hour that was lost:
//
//   - never ticked is `None`, never a zero date or an empty string. A clock
//     that has not run must not be able to look like one that has.
//   - the timestamp is UTC RFC3339 with the Z spelled out, and an age in
//     seconds rides along, so nobody has to do the arithmetic that went wrong.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// One periodic loop's clock. Cheap enough to stamp on every tick of a 750ms
/// loop: two relaxed atomic stores, no lock, no allocation.
pub struct Heartbeat {
    /// Epoch milliseconds of the last completed tick. 0 means never.
    at_ms: AtomicI64,
    ticks: AtomicU64,
}

/// One reading of a clock, as doctor reports it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Beat {
    /// UTC, RFC3339, trailing `Z`. A bare local timestamp is what caused the
    /// original mistake, so this type cannot express one.
    pub at: String,
    /// How long ago, in seconds. The number a human actually wanted.
    pub age_secs: i64,
    /// Completed ticks since the app started. Tells "it woke once at startup"
    /// apart from "it has been running steadily".
    pub ticks: u64,
}

impl Heartbeat {
    pub const fn new() -> Self {
        Self {
            at_ms: AtomicI64::new(0),
            ticks: AtomicU64::new(0),
        }
    }

    /// A tick completed. Stamped whether the tick found work, was refused work,
    /// or found an empty board: telling those apart from "did not run" is the
    /// entire point of the thing.
    pub fn beat(&self) {
        self.at_ms
            .store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
        self.ticks.fetch_add(1, Ordering::Relaxed);
    }

    /// `None` when this loop has never completed a tick.
    pub fn read(&self) -> Option<Beat> {
        self.read_at(chrono::Utc::now())
    }

    /// Pure, so the age arithmetic can be tested without waiting on a clock.
    pub fn read_at(&self, now: chrono::DateTime<chrono::Utc>) -> Option<Beat> {
        let at_ms = self.at_ms.load(Ordering::Relaxed);
        if at_ms == 0 {
            return None;
        }
        let at = chrono::DateTime::from_timestamp_millis(at_ms)?;
        Some(Beat {
            at: at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            age_secs: (now - at).num_seconds(),
            ticks: self.ticks.load(Ordering::Relaxed),
        })
    }
}

/// The board clock: `sweep.rs`, every 180s. The one that cost the hour.
pub static SWEEP: Heartbeat = Heartbeat::new();

/// The status clock: `status.rs`, every 750ms. Its death is the same class of
/// invisible: every agent dot freezes at whatever it last said, which on screen
/// is indistinguishable from a calm fleet.
pub static STATUS_DECAY: Heartbeat = Heartbeat::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clock_that_ran_and_found_nothing_does_not_look_like_one_that_never_ran() {
        // THE load-bearing case. The sweep writes to the ledger only when it
        // acts, so a quiet tick and a dead loop left identical evidence. These
        // two readings must not be able to be confused.
        let hb = Heartbeat::new();
        assert_eq!(
            hb.read(),
            None,
            "a loop that has never ticked reports nothing, not a zero date"
        );

        hb.beat(); // a tick that did no work whatsoever

        let beat = hb
            .read()
            .expect("a tick that found nothing to do still has to leave a beat");
        assert!(
            beat.at.ends_with('Z'),
            "the timestamp spells out UTC, so nobody compares it to a local clock: {}",
            beat.at
        );
        assert!(
            (0..5).contains(&beat.age_secs),
            "a tick that just happened reads as seconds old, got {}",
            beat.age_secs
        );
        assert_eq!(beat.ticks, 1);
    }

    #[test]
    fn the_age_is_utc_arithmetic_and_the_tick_count_climbs() {
        let hb = Heartbeat::new();
        hb.beat();
        let two_hours_on = chrono::Utc::now() + chrono::Duration::hours(2);
        let age = hb.read_at(two_hours_on).expect("stamped").age_secs;
        assert!(
            (7199..=7200).contains(&age),
            "two hours of silence must read as two hours, got {age}s"
        );

        hb.beat();
        hb.beat();
        assert_eq!(hb.read().expect("stamped").ticks, 3, "every tick counts");
    }
}
