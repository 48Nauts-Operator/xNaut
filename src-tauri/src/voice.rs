// Voice entry for the chat boxes (XNAUT-187).
//
// The old dictate button called `window.SpeechRecognition`, which does not
// exist in WKWebView: a silent no-op that survived every build and test since
// it shipped. See CLAUDE.md, "Grep before you call a window.* global".
//
// Capture is cpal (pure Rust, no C++ in our build); transcription shells out to
// whisper.cpp's `whisper-cli`. Both choices are deliberate:
//
// ponytail: whisper-rs would put a C++ toolchain in the release build, and the
// Windows leg has already died twice at link time on exactly that class of
// dependency (sqlite3.lib in 1.16.0). A binary on PATH cannot break our build.
// The ceiling: whisper-cli must be installed (brew install whisper-cpp). If
// that friction ever matters more than the build risk, swap this one function
// for whisper-rs and keep everything else.
//
// The model is downloaded on first use, never bundled — a 148 MB model would
// end the "20 MB download" claim on the spot.

use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_CAPTURE_SECONDS: u64 = 120;
const MAX_CAPTURE_SAMPLES: usize = 32 * 1024 * 1024;

pub struct CaptureOwner {
    window: String,
    id: String,
}

impl CaptureOwner {
    fn new(window: &str, id: &str) -> Result<Self, String> {
        uuid::Uuid::parse_str(id).map_err(|_| "invalid voice capture ID".to_string())?;
        Ok(Self {
            window: window.into(),
            id: id.into(),
        })
    }

    fn check(&self, window: &str, id: &str) -> Result<(), String> {
        if self.window == window && self.id == id {
            Ok(())
        } else {
            Err("voice capture belongs to another session".into())
        }
    }
}

fn capture_limit(rate: u32, channels: u16) -> Result<usize, String> {
    let samples = u64::from(rate) * u64::from(channels) * MAX_CAPTURE_SECONDS;
    if samples == 0 || samples > MAX_CAPTURE_SAMPLES as u64 {
        return Err("unsupported microphone rate or channel count".into());
    }
    Ok(samples as usize)
}

/// A per-utterance file, removed on every exit (including model/tool errors).
struct TemporaryAudio(PathBuf);
impl Drop for TemporaryAudio {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Join before reading samples or permitting a new capture. Otherwise the old
/// callback can still run after stop, overlapping the next microphone owner.
fn finish_capture(capture: crate::state::VoiceCapture) -> Result<(Vec<i16>, u32, u16), String> {
    drop(capture.stop);
    capture
        .thread
        .join()
        .map_err(|_| "microphone thread failed".to_string())?;
    let samples = std::mem::take(&mut *capture.samples.lock().map_err(|e| e.to_string())?);
    Ok((samples, capture.sample_rate, capture.channels))
}

#[derive(Serialize)]
pub struct Transcript {
    pub text: String,
    pub seconds: f32,
}

const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";
const MODEL_FILE: &str = "ggml-base.bin";

/// App-support path for the model. Same directory family as settings.json, so
/// a user who clears app data clears the model too.
pub fn model_path() -> Result<PathBuf, String> {
    let dir = dirs::data_dir()
        .ok_or_else(|| "no data directory on this platform".to_string())?
        .join("xnaut");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir.join(MODEL_FILE))
}

/// Download the model once. Returns the path either way.
pub fn ensure_model() -> Result<PathBuf, String> {
    // Two completed captures can transcribe concurrently; only one may write
    // the shared model's .part file.
    static MODEL_DOWNLOAD: Mutex<()> = Mutex::new(());
    let _download = MODEL_DOWNLOAD.lock().map_err(|e| e.to_string())?;
    let path = model_path()?;
    if path.is_file() {
        return Ok(path);
    }
    let bytes = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .build()
        .map_err(|e| e.to_string())?
        .get(MODEL_URL)
        .send()
        .map_err(|e| format!("model download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("model download failed: {e}"))?
        .bytes()
        .map_err(|e| format!("model download failed: {e}"))?;
    // Write to a temp name first: a half-written model on disk looks present
    // and then fails every transcription with a confusing whisper error.
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("cannot write model: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("cannot install model: {e}"))?;
    Ok(path)
}

/// Minimal 16-bit mono PCM WAV. whisper-cli wants 16 kHz mono; anything else
/// transcribes as noise, so the resample happens before this is called.
pub fn wav_bytes(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM header size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Nearest-neighbour downsample to 16 kHz mono. Good enough for speech, and it
/// avoids a resampler dependency for a fixed, well-conditioned ratio.
pub fn to_16k_mono(samples: &[i16], from_rate: u32, channels: u16) -> Vec<i16> {
    let mono: Vec<i16> = if channels <= 1 {
        samples.to_vec()
    } else {
        samples
            .chunks(channels as usize)
            .map(|frame| (frame.iter().map(|s| *s as i32).sum::<i32>() / frame.len() as i32) as i16)
            .collect()
    };
    if from_rate == 16_000 || mono.is_empty() {
        return mono;
    }
    let ratio = from_rate as f32 / 16_000.0;
    let out_len = (mono.len() as f32 / ratio) as usize;
    (0..out_len)
        .map(|i| mono[((i as f32) * ratio) as usize])
        .collect()
}

/// Strip whisper-cli's timestamped output down to the words.
/// Lines look like `[00:00:00.000 --> 00:00:02.000]   hello there`.
pub fn clean_transcript(raw: &str) -> String {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("whisper_") || line.starts_with("ggml_") {
                return None;
            }
            let text = match line.rfind(']') {
                Some(i) if line.starts_with('[') => &line[i + 1..],
                _ => line,
            };
            let text = text.trim();
            // whisper marks non-speech like this; it is not something a user said.
            if text.is_empty() || (text.starts_with('(') && text.ends_with(')')) {
                None
            } else {
                Some(text.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run whisper-cli over a WAV file. Separated from capture so the whole
/// transcription path is testable without a microphone.
pub fn transcribe_wav(wav: &std::path::Path, model: &std::path::Path) -> Result<String, String> {
    let out = Command::new("whisper-cli")
        .args([
            "-m",
            &model.to_string_lossy(),
            "-f",
            &wav.to_string_lossy(),
            "--no-timestamps",
            "--no-prints",
            "-l",
            "auto",
        ])
        .output()
        .map_err(|e| match e.kind() {
            // The commonest failure by far, and the one with a real answer.
            std::io::ErrorKind::NotFound => {
                "whisper-cli not found — install it with `brew install whisper-cpp`".to_string()
            }
            _ => format!("whisper-cli failed to start: {e}"),
        })?;
    if !out.status.success() {
        return Err(format!(
            "whisper-cli failed: {}",
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("unknown error")
        ));
    }
    Ok(clean_transcript(&String::from_utf8_lossy(&out.stdout)))
}

// ---- Tauri commands -------------------------------------------------------

/// Begin capturing from the default input device.
///
/// The stream lives on its own thread because cpal's Stream is not Send and
/// therefore cannot be held in AppState. The thread parks until stopped.
#[tauri::command]
pub async fn voice_start(
    window: tauri::Window,
    state: tauri::State<'_, crate::state::AppState>,
    capture_id: String,
) -> Result<(), String> {
    let owner = CaptureOwner::new(window.label(), &capture_id)?;
    // A persistent conversation reserves the input destination between
    // utterances too. Use the same lock order as voice_local_open.
    let local = state.local_voice.lock().await;
    if local
        .as_ref()
        .is_some_and(|session| !session.window_is(window.label()))
    {
        return Err("microphone belongs to another window's voice conversation".into());
    }
    let mut rec = state.voice.lock().await;
    if rec.is_some() {
        return Err("already recording".into());
    }

    let samples: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    // The thread reports its device config back, so a failure to open the
    // microphone surfaces here as an error instead of a silent no-op.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(u32, u16), String>>();
    let sink = samples.clone();

    let thread = std::thread::spawn(move || {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let opened = (|| -> Result<(cpal::Stream, u32, u16), String> {
            let device = cpal::default_host()
                .default_input_device()
                .ok_or_else(|| "no microphone found".to_string())?;
            let config = device
                .default_input_config()
                .map_err(|e| format!("microphone unavailable: {e}"))?;
            let rate = config.sample_rate().0;
            let channels = config.channels();
            let limit = capture_limit(rate, channels)?;
            let err_fn = |e| eprintln!("[voice] stream error: {e}");
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        if let Ok(mut buf) = sink.lock() {
                            let remaining = limit.saturating_sub(buf.len());
                            buf.extend(
                                data.iter()
                                    .take(remaining)
                                    .map(|s| (s * i16::MAX as f32) as i16),
                            );
                        }
                    },
                    err_fn,
                    None,
                ),
                // Some Windows drivers only offer i16.
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        if let Ok(mut buf) = sink.lock() {
                            let remaining = limit.saturating_sub(buf.len());
                            buf.extend_from_slice(&data[..data.len().min(remaining)]);
                        }
                    },
                    err_fn,
                    None,
                ),
                other => return Err(format!("unsupported microphone sample format: {other}")),
            }
            .map_err(|e| format!("cannot open microphone: {e}"))?;
            stream
                .play()
                .map_err(|e| format!("cannot start microphone: {e}"))?;
            Ok((stream, rate, channels))
        })();

        match opened {
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
            Ok((stream, rate, channels)) => {
                let _ = ready_tx.send(Ok((rate, channels)));
                // Park until voice_stop drops the sender. Dropping `stream`
                // after this is what releases the microphone.
                let _ = stop_rx.recv_timeout(Duration::from_secs(MAX_CAPTURE_SECONDS));
                drop(stream);
            }
        }
    });

    // Device initialization may wait for OS permission. Never block an async
    // runtime worker on the synchronous channel.
    let ready = tauri::async_runtime::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| "microphone did not become ready within 15 seconds".to_string())?
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let (sample_rate, channels) = match ready {
        Ok(config) => config,
        Err(error) => {
            drop(stop_tx);
            // Keep the lease until a delayed device open has actually retired.
            let _ = tauri::async_runtime::spawn_blocking(move || thread.join()).await;
            return Err(error);
        }
    };

    *rec = Some(crate::state::VoiceCapture {
        owner,
        samples,
        sample_rate,
        channels,
        stop: stop_tx,
        thread,
    });
    Ok(())
}

/// Stop capturing and transcribe what was said.
#[tauri::command]
pub async fn voice_stop(
    window: tauri::Window,
    state: tauri::State<'_, crate::state::AppState>,
    capture_id: String,
) -> Result<Transcript, String> {
    let (pcm, seconds) = take_pcm(state.inner(), window.label(), &capture_id).await?;
    let text = tauri::async_runtime::spawn_blocking(move || {
        let model = ensure_model()?;
        let path = std::env::temp_dir().join(format!("xnaut-voice-{}.wav", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options
            .open(&path)
            .map_err(|e| format!("cannot write audio: {e}"))?;
        let wav = TemporaryAudio(path);
        let written = f.write_all(&wav_bytes(&pcm, 16_000));
        drop(f);
        written.map_err(|e| format!("cannot write audio: {e}"))?;
        transcribe_wav(&wav.0, &model)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(Transcript { text, seconds })
}

/// Shared capture extraction for Whisper and the optional local speech service.
/// The local path never writes a WAV or invokes/downloads Whisper.
pub(crate) async fn take_pcm(
    state: &crate::state::AppState,
    window: &str,
    capture_id: &str,
) -> Result<(Vec<i16>, f32), String> {
    let (samples, rate, channels) = {
        let mut rec = state.voice.lock().await;
        rec.as_ref()
            .ok_or_else(|| "not recording".to_string())?
            .owner
            .check(window, capture_id)?;
        let capture = rec.take().ok_or_else(|| "not recording".to_string())?;
        tauri::async_runtime::spawn_blocking(move || finish_capture(capture))
            .await
            .map_err(|e| e.to_string())??
    };

    let seconds = samples.len() as f32 / (rate as f32 * channels.max(1) as f32);
    if seconds < 0.3 {
        return Err("nothing recorded — hold the button while you speak".into());
    }

    // Silent audio is not "no speech" — it is a mic that captured nothing, and
    // the model turns that into a phantom word ("you"). Catch it here so the
    // failure names its cause instead of writing a word the user never said.
    // On macOS a bare `cargo tauri dev` binary has no Info.plist, so TCC denies
    // the mic with no prompt; the bundled app (which carries the key) works.
    let peak = peak_amplitude(&samples);
    if peak < SILENCE_PEAK {
        return Err(
            "no audio captured — the mic is muted, the wrong input is selected, or xNAUT was not granted Microphone access (System Settings > Privacy & Security > Microphone). In `cargo tauri dev` the bare binary cannot get the mic; use the bundled app."
                .into(),
        );
    }

    Ok((to_16k_mono(&samples, rate, channels), seconds))
}

/// Discard a capture without transcription or a model download. An old cancel
/// is harmless when another session already owns the microphone.
#[tauri::command]
pub async fn voice_cancel(
    window: tauri::Window,
    state: tauri::State<'_, crate::state::AppState>,
    capture_id: String,
) -> Result<(), String> {
    let mut rec = state.voice.lock().await;
    if rec
        .as_ref()
        .is_some_and(|c| c.owner.check(window.label(), &capture_id).is_ok())
    {
        if let Some(capture) = rec.take() {
            tauri::async_runtime::spawn_blocking(move || finish_capture(capture))
                .await
                .map_err(|e| e.to_string())??;
        }
    }
    Ok(())
}

pub async fn release_window(state: &crate::state::AppState, label: &str) {
    let mut rec = state.voice.lock().await;
    if rec.as_ref().is_some_and(|c| c.owner.window == label) {
        if let Some(capture) = rec.take() {
            let _ = tauri::async_runtime::spawn_blocking(move || finish_capture(capture)).await;
        }
    }
}

/// Below this peak the capture is effectively silence — a denied/muted mic, not
/// quiet speech (real speech peaks in the thousands out of i16's 32767).
const SILENCE_PEAK: u32 = 120;

/// Loudest sample, as a positive magnitude. Used to tell "no audio" from speech.
fn peak_amplitude(samples: &[i16]) -> u32 {
    samples
        .iter()
        .map(|s| (*s as i32).unsigned_abs())
        .max()
        .unwrap_or(0)
}

/// Is the model already on disk? The UI warns before a 148 MB first download.
#[tauri::command]
pub fn voice_model_ready() -> bool {
    model_path().map(|p| p.is_file()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_tokens_are_bound_to_the_window_and_utterance() {
        let id = uuid::Uuid::new_v4().to_string();
        let owner = CaptureOwner::new("first", &id).unwrap();
        assert!(owner.check("first", &id).is_ok());
        assert!(owner.check("second", &id).is_err());
        assert!(owner
            .check("first", &uuid::Uuid::new_v4().to_string())
            .is_err());
        assert!(CaptureOwner::new("first", "").is_err());
    }

    #[test]
    fn capture_has_a_finite_memory_budget_even_for_bad_device_configs() {
        assert_eq!(capture_limit(48_000, 2).unwrap(), 11_520_000);
        assert!(capture_limit(0, 2).is_err());
        assert!(capture_limit(48_000, 0).is_err());
        assert!(capture_limit(u32::MAX, u16::MAX).is_err());
    }

    #[test]
    fn audio_file_is_removed_when_a_transcription_step_fails() {
        let path = std::env::temp_dir().join(format!("xnaut-voice-test-{}", uuid::Uuid::new_v4()));
        let failed = (|| -> Result<(), String> {
            std::fs::write(&path, b"synthetic audio").unwrap();
            let _audio = TemporaryAudio(path.clone());
            Err("model unavailable".into())
        })();
        assert!(failed.is_err());
        assert!(!path.exists());
    }

    #[test]
    fn stop_joins_the_device_thread_before_returning_samples() {
        let samples = Arc::new(Mutex::new(vec![1]));
        let sink = samples.clone();
        let (stop, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _ = receiver.recv();
            sink.lock().unwrap().push(2);
        });
        let capture = crate::state::VoiceCapture {
            owner: CaptureOwner::new("main", &uuid::Uuid::new_v4().to_string()).unwrap(),
            samples,
            sample_rate: 16_000,
            channels: 1,
            stop,
            thread,
        };
        assert_eq!(finish_capture(capture).unwrap().0, vec![1, 2]);
    }

    #[test]
    fn a_wav_header_declares_the_pcm_it_actually_carries() {
        let wav = wav_bytes(&[1, -1, 100], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        // data chunk length must match the samples, or whisper reads garbage
        // past the end and transcribes noise.
        let declared = u32::from_le_bytes(wav[40..44].try_into().unwrap());
        assert_eq!(declared, 6, "3 samples x 2 bytes");
        assert_eq!(wav.len(), 44 + 6);
        let rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
        assert_eq!(rate, 16_000);
    }

    #[test]
    fn stereo_is_mixed_and_the_rate_lands_on_16k() {
        // 48 kHz stereo, 8 frames: whisper only accepts 16 kHz mono.
        let stereo: Vec<i16> = (0..16).map(|i| i as i16 * 100).collect();
        let out = to_16k_mono(&stereo, 48_000, 2);
        assert_eq!(out.len(), 8 / 3, "8 frames at 48k -> 16k is a third");
        // first output frame is the mean of the first stereo pair
        assert_eq!(out[0], 50);
    }

    #[test]
    fn silence_is_told_apart_from_speech() {
        // A denied/muted mic delivers near-zero samples; real speech does not.
        assert!(
            peak_amplitude(&[0, 1, -2, 3]) < SILENCE_PEAK,
            "near-silence must read as silent"
        );
        assert!(
            peak_amplitude(&[0, 4000, -6000, 12]) >= SILENCE_PEAK,
            "speech-level audio must not"
        );
        assert_eq!(peak_amplitude(&[]), 0);
    }

    #[test]
    fn already_16k_mono_is_left_alone() {
        let pcm: Vec<i16> = vec![5, 6, 7];
        assert_eq!(to_16k_mono(&pcm, 16_000, 1), pcm);
    }

    #[test]
    fn transcript_keeps_the_words_and_drops_everything_else() {
        let raw = "\
whisper_init_from_file: loading model
[00:00:00.000 --> 00:00:02.000]   Add a login button
[00:00:02.000 --> 00:00:03.000]   (silence)
[00:00:03.000 --> 00:00:05.000]   to the header.
";
        assert_eq!(clean_transcript(raw), "Add a login button to the header.");
    }

    #[test]
    fn a_missing_whisper_binary_says_how_to_get_it() {
        // The path is what a user without whisper.cpp hits, and a bare
        // "No such file or directory" would tell them nothing.
        let err = transcribe_wav(
            std::path::Path::new("/nonexistent.wav"),
            std::path::Path::new("/nonexistent.bin"),
        );
        if let Err(msg) = err {
            assert!(
                msg.contains("brew install whisper-cpp") || msg.contains("whisper-cli failed"),
                "unhelpful error: {msg}"
            );
        }
    }
}
