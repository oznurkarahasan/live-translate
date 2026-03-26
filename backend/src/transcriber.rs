// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Debug, Serialize)]
pub struct TranslationUpdate {
    pub original: String,
    pub translated: String,
}

pub struct Phase2Config {
    deepgram_api_key: String,
    groq_api_key: String,
    deepgram_model: String,
    deepgram_language: String,
    groq_model: String,
    target_language: String,
}

impl Phase2Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            deepgram_api_key: std::env::var("DEEPGRAM_API_KEY")
                .context("Missing env var: DEEPGRAM_API_KEY")?,
            groq_api_key: std::env::var("GROQ_API_KEY").context("Missing env var: GROQ_API_KEY")?,
            deepgram_model: std::env::var("DEEPGRAM_MODEL")
                .unwrap_or_else(|_| "nova-2".to_string()),
            deepgram_language: std::env::var("DEEPGRAM_LANGUAGE")
                .unwrap_or_else(|_| "tr".to_string()),
            groq_model: std::env::var("GROQ_MODEL")
                .unwrap_or_else(|_| "llama-3.1-8b-instant".to_string()),
            target_language: std::env::var("TARGET_LANGUAGE")
                .unwrap_or_else(|_| "English".to_string()),
        })
    }
}

pub async fn run_realtime_pipeline(
    mut audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cfg: Phase2Config,
    tx: broadcast::Sender<TranslationUpdate>,
) -> anyhow::Result<()> {
    let stt_url = format!(
        "wss://api.deepgram.com/v1/listen?encoding=linear16&sample_rate=16000&channels=1&interim_results=true&punctuate=true&model={}&language={}",
        cfg.deepgram_model, cfg.deepgram_language
    );

    let mut request = stt_url
        .into_client_request()
        .context("Failed to build Deepgram websocket request")?;

    let auth_header = HeaderValue::from_str(&format!("Token {}", cfg.deepgram_api_key))
        .context("Invalid DEEPGRAM_API_KEY for Authorization header")?;
    request.headers_mut().insert("Authorization", auth_header);

    let (websocket, _) = connect_async(request)
        .await
        .context("Failed to connect to Deepgram realtime websocket")?;

    let (mut ws_write, mut ws_read) = websocket.split();
    let http_client = Client::new();
    let mut last_final = String::new();

    log::info!("Phase 2 pipeline active: streaming to Deepgram + Groq translation");

    loop {
        tokio::select! {
            maybe_chunk = audio_rx.recv() => {
                match maybe_chunk {
                    Some(chunk) => {
                        ws_write
                            .send(Message::Binary(chunk))
                            .await
                            .context("Failed sending audio chunk to Deepgram")?;
                    }
                    None => {
                        ws_write.send(Message::Close(None)).await.ok();
                        break;
                    }
                }
            }
            maybe_message = ws_read.next() => {
                match maybe_message {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(partial) = extract_partial_transcript(&text) {
                            log::debug!("Partial: {}", partial);
                        }

                        if let Some(final_transcript) = extract_final_transcript(&text) {
                            if final_transcript == last_final {
                                continue;
                            }

                            last_final = final_transcript.clone();
                            println!("[STT] {}", final_transcript);

                            let translation = translate_text(
                                &http_client,
                                &cfg,
                                &final_transcript,
                            ).await;

                            match translation {
                                Ok(translated_text) => {
                                    println!("[{}] {}", cfg.target_language, translated_text);

                                    let _ = tx.send(TranslationUpdate {
                                        original: final_transcript,
                                        translated: translated_text,
                                    });
                                }
                                Err(err) => log::error!("Translation failed: {}", err),
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        ws_write.send(Message::Pong(payload)).await.ok();
                    }
                    Some(Ok(Message::Close(frame))) => {
                        log::info!("Deepgram websocket closed: {:?}", frame);
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        return Err(anyhow!("Deepgram websocket error: {}", err));
                    }
                    None => {
                        log::warn!("Deepgram websocket stream ended");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn extract_partial_transcript(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    let transcript = value
        .get("channel")?
        .get("alternatives")?
        .get(0)?
        .get("transcript")?
        .as_str()?
        .trim()
        .to_string();

    if transcript.is_empty() {
        return None;
    }

    let is_final = value
        .get("is_final")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if is_final {
        return None;
    }

    Some(transcript)
}

fn extract_final_transcript(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    let transcript = value
        .get("channel")?
        .get("alternatives")?
        .get(0)?
        .get("transcript")?
        .as_str()?
        .trim()
        .to_string();

    let is_final = value
        .get("is_final")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if is_final && !transcript.is_empty() {
        Some(transcript)
    } else {
        None
    }
}

async fn translate_text(
    client: &Client,
    config: &Phase2Config,
    text: &str,
) -> anyhow::Result<String> {
    let prompt = format!(
        "You are a professional real-time translator. Translate the following text from Turkish to {}. CRITICAL: Output ONLY the direct translation. Do not include any explanations, preambles, or notes. Do not say 'This means' or 'Translated as'.",
        config.target_language
    );

    let body = serde_json::json!({
        "model": config.groq_model,
        "messages": [
            {
                "role": "system",
                "content": prompt
            },
            {
                "role": "user",
                "content": text
            }
        ],
        "temperature": 0.0
    });

    let response = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .bearer_auth(&config.groq_api_key)
        .json(&body)
        .send()
        .await
        .context("Failed to call Groq API")?;

    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .context("Failed to parse Groq API response")?;

    if !status.is_success() {
        return Err(anyhow!("Groq API error ({}): {}", status, payload));
    }

    let translated = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Groq response did not include translated text"))?;

    Ok(translated)
}

#[cfg(test)]
mod tests {
    use super::{extract_final_transcript, extract_partial_transcript, TranslationUpdate};
    use serde_json::json;

    #[test]
    fn parses_partial_and_final_transcripts() {
        let partial_payload = r#"{
            "channel": {"alternatives": [{"transcript": "merhaba dunya"}]},
            "is_final": false
        }"#;
        let final_payload = r#"{
            "channel": {"alternatives": [{"transcript": "merhaba dunya"}]},
            "is_final": true
        }"#;

        assert_eq!(
            extract_partial_transcript(partial_payload),
            Some("merhaba dunya".to_string())
        );
        assert_eq!(extract_final_transcript(partial_payload), None);
        assert_eq!(
            extract_final_transcript(final_payload),
            Some("merhaba dunya".to_string())
        );
    }

    #[test]
    fn translation_update_serializes_to_expected_json_shape() {
        let update = TranslationUpdate {
            original: "merhaba".to_string(),
            translated: "hello".to_string(),
        };

        let payload = serde_json::to_value(update).unwrap();
        assert_eq!(
            payload,
            json!({"original": "merhaba", "translated": "hello"})
        );
    }
}
