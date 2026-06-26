// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
compile_error!(
    "System audio loopback is only supported on Windows (WASAPI) and Linux \
     (PulseAudio/PipeWire). macOS is not supported."
);

use anyhow::Context;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use webrtc_vad::{Vad, VadMode};

static CALLBACK_COUNT: AtomicU64 = AtomicU64::new(0);

// Thread-local webrtc-vad instance.
// cpal's audio callback runs on a dedicated OS thread. By using thread_local!,
// `Vad` (which contains a raw pointer and is !Send) never needs to cross
// thread boundaries — it is created on first use on the callback thread.
thread_local! {
    static WVAD: std::cell::RefCell<Vad> = std::cell::RefCell::new({
        let mut v = Vad::new();
        v.set_mode(VadMode::Quality);
        v
    });
}

/// ZCR band that characterises speech (crossings-per-sample).
pub const ZCR_MIN: f32 = 0.01; // below this → DC / silence / very low-freq rumble
pub const ZCR_MAX: f32 = 0.35; // above this → impulse noise / clicks

pub struct AudioCapture {
    pub rx: mpsc::UnboundedReceiver<Vec<u8>>,
    _stream: cpal::Stream,
}

/// Core logic for audio processing, separated for unit testing.
pub fn process_audio_frame(data: &[f32], channels: u16, sample_rate: u32) -> Vec<f32> {
    // 1. Mono Conversion
    let mono_data: Vec<f32> = if channels > 1 {
        data.chunks_exact(channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        data.to_vec()
    };

    // 2. Resample to exactly 16kHz using linear interpolation.
    // The old step_by approach produced 14700Hz (44100/3) instead of 16000Hz,
    // causing Deepgram to receive pitch-shifted audio it couldn't recognise.
    let target_rate = 16000u32;
    if sample_rate == target_rate {
        return mono_data;
    }

    let input_len = mono_data.len();
    if input_len == 0 {
        return Vec::new();
    }

    let output_len = (input_len as u64 * target_rate as u64 / sample_rate as u64) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        // Map output sample index back to a fractional position in the input
        let src_pos = i as f64 * sample_rate as f64 / target_rate as f64;
        let src_idx = src_pos as usize;
        let frac = (src_pos - src_idx as f64) as f32;

        let s0 = mono_data[src_idx.min(input_len - 1)];
        let s1 = mono_data[(src_idx + 1).min(input_len - 1)];
        output.push(s0 + frac * (s1 - s0));
    }

    output
}

fn float_to_pcm16le(samples: &[f32]) -> Vec<u8> {
    let mut pcm_bytes = Vec::with_capacity(samples.len() * 2);

    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let value = (clamped * i16::MAX as f32) as i16;
        pcm_bytes.extend_from_slice(&value.to_le_bytes());
    }

    pcm_bytes
}

/// Compute the Zero-Crossing Rate of a frame.
///
/// Returns crossings-per-sample in the range 0.0–0.5.
/// Speech typically falls in the 0.01–0.35 band:
///   - Very low ZCR (<0.01): near-DC signal, not speech.
///   - Very high ZCR (>0.35): click / impulse noise, not speech.
pub fn zero_crossing_rate(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let crossings = samples
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count();
    crossings as f32 / samples.len() as f32
}

pub fn start_streaming() -> anyhow::Result<AudioCapture> {
    let host = cpal::default_host();

    // Windows: WASAPI loopback — build_input_stream on the default render device.
    // cpal's WASAPI backend detects eRender data flow and sets AUDCLNT_STREAMFLAGS_LOOPBACK.
    #[cfg(target_os = "windows")]
    let (device, config_range) = {
        let device = host
            .default_output_device()
            .context("No default output device for WASAPI loopback")?;
        let config = device
            .default_output_config()
            .context("Failed to query WASAPI output device config")?;
        log::info!(
            "Windows WASAPI loopback: capturing from '{}'",
            device.name().unwrap_or_else(|_| "unknown".into())
        );
        (device, config)
    };

    // Linux device selection priority:
    //   1. LIVE_TRANSLATE_CHOSEN_DEVICE env var — exact substring match against device name.
    //   2. Auto-detect: first device whose name contains "monitor", then "pulse", then "default".
    //   3. host.default_input_device() as a last resort (logs a warning).
    //
    // Set LIVE_TRANSLATE_CHOSEN_DEVICE=pulse in .env to pin a specific device.
    #[cfg(target_os = "linux")]
    let (device, config_range) = {
        let all_inputs: Vec<cpal::Device> = host
            .input_devices()
            .context("Failed to enumerate input devices")?
            .collect();

        log::info!("Discovered {} input device(s):", all_inputs.len());
        for d in &all_inputs {
            log::info!("  - {}", d.name().unwrap_or_else(|_| "<unnamed>".into()));
        }

        // Build a helper that matches a device name against a needle (case-insensitive contains).
        let matches = |d: &cpal::Device, needle: &str| {
            d.name()
                .map(|n| n.to_lowercase().contains(&needle.to_lowercase()))
                .unwrap_or(false)
        };

        let chosen_device_name = std::env::var("LIVE_TRANSLATE_CHOSEN_DEVICE").ok();

        let selected: cpal::Device = if let Some(ref name) = chosen_device_name {
            // Priority 1: explicit env var override.
            match all_inputs.into_iter().find(|d| matches(d, name)) {
                Some(d) => {
                    log::info!(
                        "Linux: using LIVE_TRANSLATE_CHOSEN_DEVICE-selected device '{}'",
                        d.name().unwrap_or_else(|_| "unknown".into())
                    );
                    d
                }
                None => {
                    anyhow::bail!(
                        "LIVE_TRANSLATE_CHOSEN_DEVICE=\"{}\" set but no matching input device found. \
                         Check the device list above.",
                        name
                    );
                }
            }
        } else {
            // Priority 2: auto-detect — monitor → pulse → default name → system default.
            let candidates = ["monitor", "pulse", "default"];
            let auto = candidates
                .iter()
                .find_map(|&needle| all_inputs.iter().find(|d| matches(d, needle)));

            match auto {
                Some(d) => {
                    log::info!(
                        "Linux loopback: auto-selected device '{}'",
                        d.name().unwrap_or_else(|_| "unknown".into())
                    );
                    // `d` is a &cpal::Device borrowed from all_inputs; re-find by name to take ownership.
                    let name = d.name().unwrap_or_default();
                    all_inputs.into_iter().find(|d| matches(d, &name)).unwrap()
                }
                None => {
                    log::warn!(
                        "No monitor/pulse/default device found — falling back to system default input. \
                         System audio loopback may NOT be captured. \
                         Set LIVE_TRANSLATE_CHOSEN_DEVICE=<name> in .env to pin a device."
                    );
                    host.default_input_device()
                        .context("No default input device available")?
                }
            }
        };

        let config = selected
            .default_input_config()
            .context("Failed to query input device config")?;
        (selected, config)
    };

    let sample_rate = config_range.sample_rate().0;
    let channels = config_range.channels();
    let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();

    log::info!("Hardware: {}Hz, {} channels", sample_rate, channels);

    // ── VAD state ────────────────────────────────────────────────────────────

    /// Exponential smoothing factor for the noise floor update.
    /// Close to 1.0 = slow adaptation (doesn't track speech as noise).
    const NOISE_FLOOR_ALPHA: f32 = 0.995;

    /// Speech threshold = noise_floor × SNR_RATIO.
    /// 3.0 means speech must be 3× louder than the measured ambient noise.
    const SNR_RATIO: f32 = 3.0;

    /// How long to keep streaming after speech stops (ms).
    /// Prevents clipping the tail of words and sentences.
    const HANGOVER_MS: f32 = 300.0;

    /// Number of pre-speech frames to buffer.
    /// At ~20ms/frame this covers ~160 ms before speech onset,
    /// so the attack of the first syllable is never cut off.
    const PRE_SPEECH_FRAMES: usize = 8;

    // Mutable VAD variables captured by the callback closure.
    let mut noise_floor: f32 = 0.005; // conservative starting value
    let mut hangover_remaining_ms: f32 = 0.0;
    let mut pre_buffer: VecDeque<Vec<u8>> = VecDeque::with_capacity(PRE_SPEECH_FRAMES + 1);

    // Read the device's native format before consuming config_range.
    // This drives the format-dispatch below so ALSA is never asked to use a
    // sample format it doesn't support (e.g. F32 on hw: devices).
    let sample_format = config_range.sample_format();
    let stream_config: cpal::StreamConfig = config_range.into();

    log::info!("Device native sample format: {:?}", sample_format);

    // The entire VAD pipeline lives inside one Box<dyn FnMut(&[f32])> so that
    // mutable state (noise_floor, hangover, pre_buffer) is created exactly once.
    // Option::take() transfers ownership into whichever format arm actually runs;
    // the other arms compile but are never reached at runtime.
    let mut vad_cb: Option<Box<dyn FnMut(&[f32]) + Send + 'static>> =
        Some(Box::new(move |data: &[f32]| {
            let resampled = process_audio_frame(data, channels, sample_rate);

            if resampled.is_empty() {
                return;
            }

            // ── Feature extraction ───────────────────────────────────────────
            let frame_ms = resampled.len() as f32 / 16_000.0 * 1000.0;

            let rms = (resampled.iter().map(|&x| x * x).sum::<f32>() / resampled.len() as f32)
                .sqrt();

            let zcr = zero_crossing_rate(&resampled);

            // ── VAD decision ─────────────────────────────────────────────────
            //
            // Two-stage pipeline:
            //   Stage 1 (cheap):  RMS energy + ZCR band filter.
            //                     Rejects obvious silence / impulse noise
            //                     without touching the webrtc-vad API.
            //   Stage 2 (accurate): webrtc-vad GMM algorithm on 10 ms frames.
            //                     Only runs when Stage 1 passes.

            let dynamic_threshold = noise_floor * SNR_RATIO;
            let energy_pass = rms > dynamic_threshold && zcr > ZCR_MIN && zcr < ZCR_MAX;

            // Stage 2: confirm with webrtc-vad on 10 ms windows.
            // We convert the resampled f32 frame to i16 and split into
            // 160-sample chunks; speech is confirmed if ANY chunk is voiced.
            let wvad_speech = if energy_pass {
                let i16_frame: Vec<i16> = resampled
                    .iter()
                    .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                    .collect();

                WVAD.with(|cell| {
                    let mut vad = cell.borrow_mut();
                    i16_frame
                        .chunks(160)
                        .filter(|chunk| chunk.len() == 160)
                        .any(|chunk| vad.is_voice_segment(chunk).unwrap_or(false))
                })
            } else {
                false
            };

            let is_speech = wvad_speech;

            // ── Update adaptive noise floor (only during confirmed silence) ──
            // We skip the update while the hangover is active to prevent
            // voiced audio from dragging the floor upward.
            if !is_speech && hangover_remaining_ms <= 0.0 {
                noise_floor =
                    NOISE_FLOOR_ALPHA * noise_floor + (1.0 - NOISE_FLOOR_ALPHA) * rms;
            }

            // ── Hangover logic ───────────────────────────────────────────────
            let was_silent = hangover_remaining_ms <= 0.0;

            if is_speech {
                if was_silent {
                    // ── Speech onset: flush the pre-speech ring buffer first ─
                    // This recovers the ~160 ms audio that preceded detection,
                    // so the start of the utterance is not clipped.
                    for buffered in pre_buffer.drain(..) {
                        if let Err(err) = tx.send(buffered) {
                            log::error!("Failed to flush pre-buffer: {}", err);
                            return;
                        }
                    }
                }
                hangover_remaining_ms = HANGOVER_MS;
            } else {
                hangover_remaining_ms = (hangover_remaining_ms - frame_ms).max(0.0);
            }

            // ── Route audio ──────────────────────────────────────────────────
            let pcm = float_to_pcm16le(&resampled);

            if hangover_remaining_ms > 0.0 {
                // Active or hanging-over: forward to transcriber
                if let Err(err) = tx.send(pcm) {
                    log::error!("Failed to queue audio chunk: {}", err);
                }
            } else {
                // Silence: maintain the pre-speech ring buffer
                if pre_buffer.len() >= PRE_SPEECH_FRAMES {
                    pre_buffer.pop_front();
                }
                pre_buffer.push_back(pcm);
            }

            // ── Periodic diagnostic log ──────────────────────────────────────
            let count = CALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            if count.is_multiple_of(100) {
                log::info!(
                    "VAD | RMS: {:.4}  ZCR: {:.3}  floor: {:.4}  thr: {:.4}  energy_pass: {}  wvad: {}  hangover: {:.0}ms",
                    rms,
                    zcr,
                    noise_floor,
                    dynamic_threshold,
                    energy_pass,
                    wvad_speech,
                    hangover_remaining_ms,
                );
            }
        }));

    // Build a stream using the device's native sample format.
    // Non-f32 formats are converted with simple arithmetic before entering the
    // pipeline — no ALSA format coercion, no Invalid argument (22) crash.
    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let mut cb = vad_cb.take().unwrap();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| cb(data),
                move |err| log::error!("Stream error: {}", err),
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let mut cb = vad_cb.take().unwrap();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    // Map signed 16-bit integers to [-1.0, 1.0].
                    let floats: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    cb(&floats)
                },
                move |err| log::error!("Stream error: {}", err),
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let mut cb = vad_cb.take().unwrap();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    // Shift unsigned 16-bit integers to signed range, then normalize to [-1.0, 1.0].
                    let floats: Vec<f32> =
                        data.iter().map(|&s| (s as f32 / 32_768.0) - 1.0).collect();
                    cb(&floats)
                },
                move |err| log::error!("Stream error: {}", err),
                None,
            )?
        }
        fmt => anyhow::bail!(
            "Unsupported device sample format {:?}. Expected F32, I16, or U16.",
            fmt
        ),
    };

    stream.play()?;

    Ok(AudioCapture {
        rx,
        _stream: stream,
    })
}

// ── Unit tests ────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mono_conversion_and_resampling() {
        // 6 input samples = 3 stereo frames at 48000Hz
        // After mono mix: [0.5, 1.0, -0.5]  (3 mono frames)
        // After resample to 16kHz: output_len = 3 * 16000 / 48000 = 1 sample
        // At i=0: src_pos=0.0, s0=0.5, frac=0.0 → output = 0.5
        let fake_audio = vec![0.5f32, 0.5, 1.0, 1.0, -0.5, -0.5];
        let channels = 2;
        let sample_rate = 48000;

        let result = process_audio_frame(&fake_audio, channels, sample_rate);

        assert!(!result.is_empty(), "Resampled output must not be empty");
        assert_eq!(result.len(), 1, "3 mono frames at 48kHz → 1 frame at 16kHz");
        assert!(
            (result[0] - 0.5).abs() < 1e-5,
            "Expected ~0.5, got {}",
            result[0]
        );
    }

    #[test]
    fn test_zcr_silence() {
        // A flat zero signal has zero crossings.
        let silence = vec![0.0f32; 64];
        assert_eq!(zero_crossing_rate(&silence), 0.0);
    }

    #[test]
    fn test_zcr_alternating() {
        // Fully alternating signal (+1, -1, +1, …) produces near-maximum ZCR.
        let alternating: Vec<f32> = (0..64)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let zcr = zero_crossing_rate(&alternating);
        // Every adjacent pair crosses → (n-1)/(n) crossings per sample
        assert!(zcr > 0.4, "Expected high ZCR, got {}", zcr);
    }

    #[test]
    fn test_zcr_short_frame() {
        // Single sample: no pairs to compare, should not panic.
        assert_eq!(zero_crossing_rate(&[0.5]), 0.0);
        assert_eq!(zero_crossing_rate(&[]), 0.0);
    }

    #[test]
    fn test_zcr_speech_band() {
        // A 200 Hz sine wave at 16 kHz should have reasonable ZCR.
        // 200 Hz → 200 zero-crossings/sec → 200/16000 = 0.0125 per sample.
        let sr = 16000.0f32;
        let freq = 200.0f32;
        let samples: Vec<f32> = (0..160)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect();
        let zcr = zero_crossing_rate(&samples);
        // Expect roughly 2 crossings per cycle → 200 * 2 / 16000 = 0.025
        assert!(
            zcr > ZCR_MIN && zcr < ZCR_MAX,
            "Speech ZCR out of band: {}",
            zcr
        );
    }

    #[test]
    fn test_adaptive_noise_floor_converges() {
        // Simulate 500 frames of background noise at RMS ~0.01
        // and verify the floor converges toward that value.
        let alpha = 0.995f32;
        let true_noise_rms = 0.01f32;
        let mut floor = 0.005f32;

        for _ in 0..500 {
            floor = alpha * floor + (1.0 - alpha) * true_noise_rms;
        }

        // After 500 updates the floor should be within 30% of the true noise
        assert!(
            (floor - true_noise_rms).abs() < true_noise_rms * 0.3,
            "Noise floor did not converge: floor={:.5}, target={:.5}",
            floor,
            true_noise_rms
        );
    }
}
