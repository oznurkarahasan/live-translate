// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use anyhow::Context;

pub async fn start_listening() -> anyhow::Result<()> {
    // 1. Initialize the audio host (ALSA on Linux, CoreAudio on macOS, etc.)
    let host = cpal::default_host();
    
    // 2. Access the default input device (Microphone)
    let device = host.default_input_device()
        .context("Failed to find a default input device. Please check your microphone connection.")?;
    
    log::info!("Selected Input Device: {}", device.name()?);

    // 3. Get the default input configuration from the hardware
    let config: cpal::StreamConfig = device.default_input_config()?.into();
    log::info!("Hardware default config: {:?}", config);

    // 4. Build the input stream with a high-performance callback
    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // High-performance callback: this runs every time the audio buffer is full
            // For now, we only log if the buffer contains actual sound (non-zero samples)
            if let Some(first_sample) = data.first() {
                if *first_sample != 0.0 {
                    log::debug!("Captured audio buffer. Size: {}", data.len());
                }
            }
        },
        move |err| {
            log::error!("Audio stream error: {}", err);
        },
        None
    )?;

    // 5. Activate the stream
    stream.play()?;
    log::info!("Audio capture stream is now active.");

    // Keep the async task alive indefinitely to continue capturing
    tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;

    Ok(())
}