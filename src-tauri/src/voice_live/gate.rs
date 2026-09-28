// Ported from Bucki (48Nauts/Bucky, development @ 629ff06),
// `Bucki/Services/LivePlaybackInterruptionGate.swift::LivePlaybackInterruptionGate`.
// Licence: the Bucky repository carries no distributed licence file; this is a
// reimplementation of the observed algorithm for xNAUT, not copied source.
//
// Why a gate at all: the Live transport streams assistant audio continuously and
// has no "response.cancel" the way Realtime does. Barge-in therefore has to be
// decided locally, from two independent signals, and it has to hold its decision
// for a moment so the tail of an already-buffered reply cannot leak through.
//
// Departure from the source: Bucki keeps the window in a float `TimeInterval`
// seeded with `-.infinity`. We take an explicit `Option<f64>` instead, so a
// session that has never heard speech is a distinct state rather than a
// sentinel that arithmetic can accidentally resurrect.

/// Seconds of continued output suppression after speech is confirmed. Long
/// enough to swallow audio already in flight from the server, short enough that
/// a one-word interjection does not mute the rest of the answer.
const SUPPRESSION_WINDOW: f64 = 0.25;

/// Microphone energy must exceed both a floor and this multiple of the current
/// output level before it counts as a candidate. The multiple is what keeps the
/// speaker's own output from opening the gate on a machine without hardware
/// echo cancellation.
const OUTPUT_ENERGY_MULTIPLE: f64 = 4.0;
const ENERGY_FLOOR: f64 = 0.22;

/// A candidate must persist this long before it is confirmed, so a door slam or
/// a keyboard clack does not stop playback.
const CANDIDATE_HOLD: f64 = 0.08;

#[derive(Debug, Default, Clone)]
pub struct InterruptionGate {
    candidate_since: Option<f64>,
    last_speech_at: Option<f64>,
}

impl InterruptionGate {
    /// True while assistant audio must be dropped rather than played. Callers
    /// check this on every output frame, not only at turn boundaries.
    pub fn suppresses_output(&self, now: f64) -> bool {
        self.last_speech_at
            .is_some_and(|last| now - last < SUPPRESSION_WINDOW)
    }

    /// Records confirmed speech — a transcript delta from the server, which is
    /// authoritative in a way energy never is. Returns true only on the edge
    /// that *begins* an interruption, so one barge-in raises one stop.
    pub fn confirm_speech(&mut self, now: f64) -> bool {
        let began = !self.suppresses_output(now);
        self.last_speech_at = Some(now);
        self.candidate_since = None;
        began
    }

    /// Local energy heuristic, used to stop playback before the server's
    /// transcript arrives. `has_output` gates it: with nothing playing there is
    /// nothing to interrupt, and treating silence as barge-in would retrigger
    /// on every breath.
    pub fn update(&mut self, level: f64, output_level: f64, has_output: bool, now: f64) -> bool {
        if level <= ENERGY_FLOOR.max(output_level * OUTPUT_ENERGY_MULTIPLE) {
            self.candidate_since = None;
            return false;
        }
        // Already inside a confirmed interruption: extend it, do not raise a
        // second stop for the same stretch of speech.
        if self.suppresses_output(now) {
            self.last_speech_at = Some(now);
            return false;
        }
        if !has_output {
            self.candidate_since = None;
            return false;
        }
        let since = *self.candidate_since.get_or_insert(now);
        if now - since < CANDIDATE_HOLD {
            return false;
        }
        self.confirm_speech(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_gate_suppresses_nothing() {
        let gate = InterruptionGate::default();
        assert!(!gate.suppresses_output(0.0));
        assert!(!gate.suppresses_output(1_000.0));
    }

    #[test]
    fn confirmed_speech_reports_only_the_opening_edge() {
        let mut gate = InterruptionGate::default();
        assert!(gate.confirm_speech(10.0), "first confirmation interrupts");
        assert!(
            !gate.confirm_speech(10.1),
            "a second delta inside the window is the same interruption"
        );
        assert!(
            gate.confirm_speech(20.0),
            "speech after the window is a new interruption"
        );
    }

    #[test]
    fn output_is_suppressed_only_inside_the_window() {
        let mut gate = InterruptionGate::default();
        gate.confirm_speech(5.0);
        assert!(gate.suppresses_output(5.1));
        assert!(gate.suppresses_output(5.0 + SUPPRESSION_WINDOW - 0.01));
        assert!(!gate.suppresses_output(5.0 + SUPPRESSION_WINDOW));
    }

    #[test]
    fn the_speakers_own_output_does_not_open_the_gate() {
        let mut gate = InterruptionGate::default();
        // Loud, but only because the assistant itself is loud.
        assert!(!gate.update(0.8, 0.5, true, 1.0));
        assert!(!gate.update(0.8, 0.5, true, 1.5));
        assert!(!gate.suppresses_output(1.5));
    }

    #[test]
    fn energy_must_persist_before_it_interrupts() {
        let mut gate = InterruptionGate::default();
        assert!(!gate.update(0.9, 0.01, true, 1.0), "first sample only arms");
        assert!(
            !gate.update(0.9, 0.01, true, 1.0 + CANDIDATE_HOLD / 2.0),
            "still inside the hold"
        );
        assert!(gate.update(0.9, 0.01, true, 1.0 + CANDIDATE_HOLD));
    }

    #[test]
    fn a_transient_spike_disarms_the_candidate() {
        let mut gate = InterruptionGate::default();
        assert!(!gate.update(0.9, 0.01, true, 1.0));
        // Energy collapses: the clack is over.
        assert!(!gate.update(0.0, 0.01, true, 1.02));
        // The hold restarts rather than carrying the old candidate forward.
        assert!(!gate.update(0.9, 0.01, true, 1.04));
        assert!(gate.update(0.9, 0.01, true, 1.04 + CANDIDATE_HOLD));
    }

    #[test]
    fn energy_alone_never_interrupts_silence() {
        let mut gate = InterruptionGate::default();
        for step in 0..20 {
            assert!(!gate.update(1.0, 0.0, false, step as f64 * 0.05));
        }
        assert!(!gate.suppresses_output(1.0));
    }
}
