// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use anyhow::Context;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

static CALLBACK_COUNT: AtomicU64 = AtomicU64::new(0);

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

pub fn start_streaming() -> anyhow::Result<AudioCapture> {
    let host = cpal::default_host();
    let device = host.default_input_device().context("No input device")?;
    let config_range = device.default_input_config()?;
    let sample_rate = config_range.sample_rate().0;
    let channels = config_range.channels();
    let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();

    log::info!("Hardware: {}Hz, {} channels", sample_rate, channels);

    let stream = device.build_input_stream(
        &config_range.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let resampled = process_audio_frame(data, channels, sample_rate);
            let pcm = float_to_pcm16le(&resampled);

            if let Err(err) = tx.send(pcm) {
                log::error!("Failed to queue audio chunk: {}", err);
            }

            let count = CALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            if count % 100 == 0 {
                log::info!("Stream Active. Processed buffer size: {}", resampled.len());
            }
        },
        move |err| log::error!("Stream error: {}", err),
        None
    )?;

    stream.play()?;

    Ok(AudioCapture {
        rx,
        _stream: stream,
    })
}

// --- UNIT TESTS ---
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
        assert!((result[0] - 0.5).abs() < 1e-5, "Expected ~0.5, got {}", result[0]);
    }
}