//! Bounded native PCM renderer for the optional local voice service.
//! Cancellation is checked in the audio callback as well as the device thread.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const QUEUE_SAMPLES: usize = 32_000;

#[derive(Default)]
struct Pcm {
    samples: VecDeque<i16>,
    current: Option<i16>,
    phase: f64,
    finished: bool,
    failed: bool,
}

impl Pcm {
    fn next(&mut self, output_rate: u32) -> f32 {
        if self.current.is_none() {
            self.current = self.samples.pop_front();
        }
        let Some(current) = self.current else {
            self.phase = 0.0;
            return 0.0;
        };
        let next = self.samples.front().copied().unwrap_or(current);
        let value = (current as f64 + (next as f64 - current as f64) * self.phase) / 32768.0;
        self.phase += 16_000.0 / output_rate as f64;
        while self.phase >= 1.0 {
            self.current = self.samples.pop_front();
            self.phase -= 1.0;
        }
        value as f32
    }
    fn drained(&self) -> bool {
        self.samples.is_empty() && self.current.is_none()
    }
}

pub struct Playback {
    pcm: Arc<Mutex<Pcm>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Playback {
    pub async fn open(cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Result<Self, String> {
        let pcm = Arc::new(Mutex::new(Pcm::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let source = pcm.clone();
        let stopped = stop.clone();
        let cancel = cancelled.clone();
        let thread = std::thread::spawn(move || {
            let opened = (|| -> Result<cpal::Stream, String> {
                let device = cpal::default_host()
                    .default_output_device()
                    .ok_or("no speaker output available")?;
                let config = device.default_output_config().map_err(|e| e.to_string())?;
                let rate = config.sample_rate().0;
                let channels = config.channels() as usize;
                if rate == 0 || channels == 0 {
                    return Err("invalid speaker format".into());
                }
                let format = config.sample_format();
                let stream_config: cpal::StreamConfig = config.into();
                let failed = source.clone();
                let error = move |_| {
                    if let Ok(mut p) = failed.lock() {
                        p.failed = true;
                    }
                };
                let audio = source.clone();
                let flag = stopped.clone();
                let cancellation = cancel.clone();
                macro_rules! output {
                    ($type:ty, $convert:expr) => {
                        device.build_output_stream(
                            &stream_config,
                            move |out: &mut [$type], _: &_| {
                                let silent = flag.load(Ordering::Acquire) || cancellation();
                                let mut buffer = audio.lock().ok();
                                for frame in out.chunks_mut(channels) {
                                    let sample = if silent {
                                        0.0
                                    } else {
                                        buffer.as_mut().map(|p| p.next(rate)).unwrap_or(0.0)
                                    };
                                    frame.fill(($convert)(sample));
                                }
                            },
                            error,
                            None,
                        )
                    };
                }
                let stream = match format {
                    cpal::SampleFormat::F32 => output!(f32, |x: f32| x),
                    cpal::SampleFormat::I16 => output!(i16, |x: f32| (x * 32767.0) as i16),
                    cpal::SampleFormat::U16 => output!(u16, |x: f32| ((x + 1.0) * 32767.5) as u16),
                    _ => return Err("unsupported speaker sample format".into()),
                }
                .map_err(|e| e.to_string())?;
                stream.play().map_err(|e| e.to_string())?;
                Ok(stream)
            })();
            match opened {
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    let mut drained_at = None;
                    while !stopped.load(Ordering::Acquire) && !cancel() {
                        let done = match source.lock() {
                            Ok(p) if p.failed => break,
                            Ok(p) => p.finished && p.drained(),
                            Err(_) => break,
                        };
                        if done {
                            let when = drained_at.get_or_insert_with(std::time::Instant::now);
                            // Allow the final callback's device buffer to drain.
                            if when.elapsed() >= Duration::from_millis(100) {
                                break;
                            }
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    drop(stream);
                }
            }
        });
        let mut player = Self {
            pcm,
            stop,
            thread: Some(thread),
            cancelled,
        };
        let ready = tauri::async_runtime::spawn_blocking(move || {
            ready_rx.recv_timeout(Duration::from_secs(15))
        })
        .await
        .map_err(|e| e.to_string())?;
        match ready {
            Ok(Ok(())) => Ok(player),
            other => {
                player.stop.store(true, Ordering::Release);
                player.join().await;
                Err(format!("cannot open speaker: {other:?}"))
            }
        }
    }

    pub async fn push(&self, samples: &[i16]) -> Result<(), String> {
        if samples.len() > QUEUE_SAMPLES {
            return Err("speech frame exceeds playback budget".into());
        }
        loop {
            if (self.cancelled)() {
                return Err("speech cancelled".into());
            }
            {
                let mut pcm = self.pcm.lock().map_err(|e| e.to_string())?;
                if pcm.failed {
                    return Err("speaker device disconnected".into());
                }
                if pcm.samples.len() + samples.len() <= QUEUE_SAMPLES {
                    pcm.samples.extend(samples);
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    async fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = tauri::async_runtime::spawn_blocking(move || thread.join()).await;
        }
    }
    pub async fn finish(mut self) -> Result<(), String> {
        self.pcm.lock().map_err(|e| e.to_string())?.finished = true;
        self.join().await;
        if self.pcm.lock().map_err(|e| e.to_string())?.failed {
            return Err("speaker device disconnected".into());
        }
        Ok(())
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_resampler_preserves_duration_and_drains_at_device_rate() {
        let mut pcm = Pcm::default();
        pcm.samples.extend([1000; 160]);
        let mut frames = 0;
        while !pcm.drained() {
            assert!((pcm.next(48_000) - 1000.0 / 32768.0).abs() < 0.00001);
            frames += 1;
        }
        assert_eq!(frames, 480);
        assert_eq!(pcm.next(48_000), 0.0);
    }
    #[test]
    fn streaming_resampler_survives_underrun_without_replaying_audio() {
        let mut pcm = Pcm::default();
        pcm.samples.push_back(1000);
        assert!(pcm.next(16_000) > 0.0);
        assert_eq!(pcm.next(16_000), 0.0);
        pcm.samples.push_back(-1000);
        assert!(pcm.next(16_000) < 0.0);
        assert!(pcm.drained());
    }
}
