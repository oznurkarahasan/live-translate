// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

mod audio;
mod server;
mod transcriber;
use tokio::sync::broadcast;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the logger to see system outputs in the terminal
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    dotenvy::dotenv().ok();

    log::info!("Starting Live-Translate Backend...");

    let capture = audio::start_streaming()?;
    let config = transcriber::Phase2Config::from_env()?;
    let (tx, _) = broadcast::channel(128);
    let (settings_tx, settings_rx) = watch::channel(config.initial_language_selection());

    tokio::spawn(server::start_server(tx.clone(), settings_tx));

    transcriber::run_realtime_pipeline(capture.rx, config, tx, settings_rx).await?;

    Ok(())
}
