// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

mod audio;
mod transcriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the logger to see system outputs in the terminal
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    dotenvy::dotenv().ok();

    log::info!("Starting Live-Translate Backend...");

    let capture = audio::start_streaming()?;
    let config = transcriber::Phase2Config::from_env()?;

    transcriber::run_realtime_pipeline(capture.rx, config).await?;

    Ok(())
}
