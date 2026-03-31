// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Debug, Serialize)]
pub struct TranslationUpdate {
    pub original: String,
    pub translated: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LanguageSelection {
    pub spoken_language: String,
    pub target_language: String,
}

pub struct Phase2Config {
    pub deepgram_api_key: String,
    pub groq_api_key: String,
    pub deepgram_model: String,
    pub deepgram_language: String,
    pub groq_model: String,
    pub spoken_language: String,
    pub target_language: String,
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
                .unwrap_or_else(|_| "en".to_string()),
            groq_model: std::env::var("GROQ_MODEL")
                .unwrap_or_else(|_| "llama-3.1-8b-instant".to_string()),
            spoken_language: std::env::var("SPOKEN_LANGUAGE")
                .unwrap_or_else(|_| "English".to_string()),
            target_language: std::env::var("TARGET_LANGUAGE")
                .unwrap_or_else(|_| "Turkish".to_string()),
        })
    }

    pub fn initial_language_selection(&self) -> LanguageSelection {
        LanguageSelection {
            spoken_language: self.spoken_language.clone(),
            target_language: self.target_language.clone(),
        }
    }
}

pub async fn run_realtime_pipeline(
    mut audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cfg: Phase2Config,
    tx: broadcast::Sender<TranslationUpdate>,
    mut settings_rx: watch::Receiver<LanguageSelection>,
) -> anyhow::Result<()> {
    let initial_spoken_language = settings_rx.borrow().spoken_language.clone();
    let mut active_stt_language =
        resolve_deepgram_language(&initial_spoken_language, &cfg.deepgram_language);
    let (mut ws_write, mut ws_read) = connect_to_deepgram(&cfg, &active_stt_language).await?;
    let http_client = Client::new();
    let mut last_final = String::new();

    log::info!(
        "Phase 2 pipeline active: streaming to Deepgram + Groq translation (STT language: {})",
        active_stt_language
    );

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

                            let language_selection = settings_rx.borrow().clone();

                            if is_same_language_pair(&language_selection) {
                                println!(
                                    "[{}] {}",
                                    language_selection.target_language, final_transcript
                                );

                                let _ = tx.send(TranslationUpdate {
                                    original: final_transcript.clone(),
                                    translated: final_transcript,
                                });
                                continue;
                            }

                            let translation = translate_text(
                                &http_client,
                                &cfg.groq_api_key,
                                &cfg.groq_model,
                                &final_transcript,
                                &language_selection,
                            ).await;

                            match translation {
                                Ok(translated_text) => {
                                    println!("[{}] {}", language_selection.target_language, translated_text);

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
            changed = settings_rx.changed() => {
                if changed.is_err() {
                    log::warn!("Language settings channel closed");
                    continue;
                }

                let new_spoken_language = settings_rx.borrow().spoken_language.clone();
                let new_stt_language = resolve_deepgram_language(
                    &new_spoken_language,
                    &cfg.deepgram_language,
                );

                if new_stt_language == active_stt_language {
                    continue;
                }

                log::info!(
                    "Spoken language changed to '{}'; reconnecting Deepgram with language '{}'",
                    new_spoken_language,
                    new_stt_language
                );

                ws_write.send(Message::Close(None)).await.ok();
                let (new_ws_write, new_ws_read) = connect_to_deepgram(&cfg, &new_stt_language).await?;
                ws_write = new_ws_write;
                ws_read = new_ws_read;
                active_stt_language = new_stt_language;
                last_final.clear();
            }
        }
    }

    Ok(())
}

pub fn resolve_deepgram_language(spoken_language: &str, fallback_language: &str) -> String {
    match spoken_language {
        "English" => "en".to_string(),
        "Turkish" => "tr".to_string(),
        _ => fallback_language.to_string(),
    }
}

async fn connect_to_deepgram(
    cfg: &Phase2Config,
    deepgram_language: &str,
) -> anyhow::Result<(
    futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
)> {
    let stt_url = format!(
        "wss://api.deepgram.com/v1/listen?encoding=linear16&sample_rate=16000&channels=1&interim_results=true&punctuate=true&model={}&language={}",
        cfg.deepgram_model, deepgram_language
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

    Ok(websocket.split())
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

pub fn is_same_language_pair(language_selection: &LanguageSelection) -> bool {
    language_selection
        .spoken_language
        .trim()
        .eq_ignore_ascii_case(language_selection.target_language.trim())
}

pub async fn translate_text(
    client: &Client,
    groq_api_key: &str,
    groq_model: &str,
    text: &str,
    language_selection: &LanguageSelection,
) -> anyhow::Result<String> {
    let prompt = format!(
        "You are an expert, highly accurate translator. Translate the following text from {} to {}. Make it sound natural and contextual in the target language. CRITICAL: Output ONLY the direct translation. No explanations, no notes, no quotes.",
        language_selection.spoken_language,
        language_selection.target_language
    );

    let body = serde_json::json!({
        "model": groq_model,
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
        .bearer_auth(groq_api_key)
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
    use super::{
        extract_final_transcript, extract_partial_transcript, is_same_language_pair,
        LanguageSelection, TranslationUpdate,
    };
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

    #[test]
    fn detects_same_language_selection() {
        let same = LanguageSelection {
            spoken_language: "Turkish".to_string(),
            target_language: "turkish".to_string(),
        };
        let different = LanguageSelection {
            spoken_language: "Turkish".to_string(),
            target_language: "English".to_string(),
        };

        assert!(is_same_language_pair(&same));
        assert!(!is_same_language_pair(&different));
    }
}
