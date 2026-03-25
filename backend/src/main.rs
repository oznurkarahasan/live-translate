// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

mod audio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the logger to see system outputs in the terminal
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    
    log::info!("Starting Live-Translate Backend...");

    // Call the audio capture module
    audio::start_listening().await?;

    Ok(())
}