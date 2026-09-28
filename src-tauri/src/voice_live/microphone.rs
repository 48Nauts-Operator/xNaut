//! A mute fence shared by capture and the network writer. Epochs prevent
//! queued audio from being replayed when the microphone is unmuted.
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct MicrophoneGate(AtomicU64);

impl MicrophoneGate {
    pub(super) fn set_muted(&self, muted: bool) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |epoch| {
            if (epoch & 1 == 1) == muted { None } else { Some(epoch.wrapping_add(1)) }
        });
    }

    pub(super) fn capture_epoch(&self) -> Option<u64> {
        let epoch = self.0.load(Ordering::Acquire);
        (epoch & 1 == 0).then_some(epoch)
    }

    pub(super) fn accepts(&self, epoch: u64) -> bool {
        self.capture_epoch() == Some(epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mute_drops_capture_and_pending_audio_even_after_unmute() {
        let gate = MicrophoneGate::default();
        let before = gate.capture_epoch().unwrap();
        assert!(gate.accepts(before));
        gate.set_muted(true);
        assert_eq!(gate.capture_epoch(), None);
        assert!(!gate.accepts(before));
        gate.set_muted(true);
        gate.set_muted(false);
        let after = gate.capture_epoch().unwrap();
        assert_ne!(before, after);
        assert!(!gate.accepts(before));
        assert!(gate.accepts(after));
        gate.set_muted(false);
        assert!(gate.accepts(after), "repeated unmute must not discard new audio");
    }
}
