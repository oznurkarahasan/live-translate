// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use anyhow::Context;
use std::sync::atomic::{AtomicU64, Ordering};

static CALLBACK_COUNT: AtomicU64 = AtomicU64::new(0);

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

    // 2. Resampling to 16kHz
    let target_rate = 16000.0;
    let skip_step = (sample_rate as f32 / target_rate).round() as usize;
    
    mono_data.into_iter()
        .step_by(skip_step)
        .collect()
}

pub async fn start_listening() -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host.default_input_device().context("No input device")?;
    let config_range = device.default_input_config()?;
    let sample_rate = config_range.sample_rate().0;
    let channels = config_range.channels();

    log::info!("Hardware: {}Hz, {} channels", sample_rate, channels);

    let stream = device.build_input_stream(
        &config_range.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Use the testable function
            let resampled = process_audio_frame(data, channels, sample_rate);

            let count = CALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            if count % 100 == 0 {
                log::info!("Stream Active. Processed buffer size: {}", resampled.len());
            }
        },
        move |err| log::error!("Stream error: {}", err),
        None
    )?;

    stream.play()?;
    tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;
    Ok(())
}

// --- UNIT TESTS ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mono_conversion_and_resampling() {
        // Create a fake stereo signal (2 channels) at 48000Hz
        // A buffer of 6 samples (3 frames of stereo)
        let fake_audio = vec![0.5, 0.5, 1.0, 1.0, -0.5, -0.5]; 
        let channels = 2;
        let sample_rate = 48000;

        let result = process_audio_frame(&fake_audio, channels, sample_rate);

        // 48000 / 16000 = 3 (skip_step). 
        // Original has 3 mono frames. skip_step 3 should result in 1 sample.
        assert!(!result.is_empty());
        assert_eq!(result.len(), 1);
        // Average of 0.5 and 0.5 is 0.5
        assert_eq!(result[0], 0.5);
    }
}