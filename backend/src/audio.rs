// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use anyhow::Context;
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter to throttle logs and avoid terminal flooding
static CALLBACK_COUNT: AtomicU64 = AtomicU64::new(0);

pub async fn start_listening() -> anyhow::Result<()> {
    // 1. Initialize Audio Host
    let host = cpal::default_host();
    
    // 2. Find Default Input Device
    let device = host.default_input_device()
        .context("No input device found. Is the microphone connected?")?;
    
    let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    
    // 3. Get Hardware Config
    let config_range = device.default_input_config()
        .context("Failed to get default input config")?;
    let sample_rate = config_range.sample_rate().0;
    let channels = config_range.channels();

    log::info!("Hardware detected: {} | {}Hz | {} channels", device_name, sample_rate, channels);

    // 4. Build Input Stream
    let stream = device.build_input_stream(
        &config_range.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // STEP 1: Mono Conversion (Average multiple channels into one)
            let mono_data: Vec<f32> = if channels > 1 {
                data.chunks_exact(channels as usize)
                    .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                    .collect()
            } else {
                data.to_vec()
            };

            // STEP 2: Precise Resampling Calculation
            // We target 16000Hz for AI models
            let target_rate = 16000.0;
            let skip_step = (sample_rate as f32 / target_rate).round() as usize;
            
            let resampled_data: Vec<f32> = mono_data.iter()
                .step_by(skip_step)
                .cloned()
                .collect();

            // STEP 3: Progress Logging (Every 100 callbacks ~ roughly every 1-2 seconds)
            let count = CALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            if count % 100 == 0 {
                log::info!(
                    "Stream Active | Raw: {} | Mono: {} | Resampled (16kHz): {}", 
                    data.len(), 
                    mono_data.len(), 
                    resampled_data.len()
                );
            }
        },
        move |err| {
            log::error!("Audio stream error: {}", err);
        },
        None
    )?;

    // 5. Start Capture
    stream.play()?;
    log::info!("Microphone is live. Resampling to 16kHz in real-time...");

    // Keep the task alive
    tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;

    Ok(())
}