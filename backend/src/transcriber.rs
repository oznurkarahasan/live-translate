// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use anyhow::anyhow;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};

use crate::whisper_stt::WhisperStt;

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

/// Runtime configuration read from environment variables.
pub struct Phase2Config {
    pub groq_api_key: String,
    pub groq_model: String,
    pub spoken_language: String,
    pub target_language: String,
    /// Kept for upload/file-based transcription (Deepgram REST, optional)
    pub deepgram_api_key: Option<String>,
    pub deepgram_model: String,
}

impl Phase2Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            // Groq is still used for translation (network required)
            groq_api_key: std::env::var("GROQ_API_KEY")
                .unwrap_or_default(),
            groq_model: std::env::var("GROQ_MODEL")
                .unwrap_or_else(|_| "llama-3.1-8b-instant".to_string()),
            spoken_language: std::env::var("SPOKEN_LANGUAGE")
                .unwrap_or_else(|_| "English".to_string()),
            target_language: std::env::var("TARGET_LANGUAGE")
                .unwrap_or_else(|_| "Turkish".to_string()),
            // Deepgram is now OPTIONAL — only needed for /upload file transcription
            deepgram_api_key: std::env::var("DEEPGRAM_API_KEY").ok(),
            deepgram_model: std::env::var("DEEPGRAM_MODEL")
                .unwrap_or_else(|_| "nova-2".to_string()),
        })
    }

    pub fn initial_language_selection(&self) -> LanguageSelection {
        LanguageSelection {
            spoken_language: self.spoken_language.clone(),
            target_language: self.target_language.clone(),
        }
    }
}

// ── Real-time pipeline (local Whisper STT) ────────────────────────────────────

/// Minimum number of PCM bytes to accumulate before running Whisper inference.
/// 16 kHz × 16-bit × 1 ch × 2 seconds = 64 000 bytes → good balance of
/// latency vs. accuracy.  Shorter windows increase CPU load and risk partial
/// words; longer windows add latency.
const MIN_BUFFER_BYTES: usize = 16_000 * 2 * 2; // 2 seconds

/// Maximum buffer size before we force a transcription even if audio keeps
/// coming.  Caps worst-case latency at ~6 s.
const MAX_BUFFER_BYTES: usize = 16_000 * 2 * 6; // 6 seconds

pub async fn run_realtime_pipeline(
    mut audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cfg: Phase2Config,
    tx: broadcast::Sender<TranslationUpdate>,
    mut settings_rx: watch::Receiver<LanguageSelection>,
) -> anyhow::Result<()> {
    let initial_lang = {
        let sel = settings_rx.borrow();
        WhisperStt::resolve_whisper_language(&sel.spoken_language).to_string()
    };

    // Load Whisper model (blocking I/O — run on a dedicated thread)
    let mut whisper = tokio::task::spawn_blocking(move || WhisperStt::from_env(&initial_lang))
        .await
        .map_err(|e| anyhow!("Whisper load task panicked: {}", e))??;

    let http_client = Client::new();
    // PCM byte accumulation buffer
    let mut pcm_buf: Vec<u8> = Vec::with_capacity(MAX_BUFFER_BYTES);
    let mut last_final = String::new();

    log::info!("Real-time pipeline active (local Whisper STT, Groq translation)");

    loop {
        tokio::select! {
            // ── Receive audio chunks from VAD ──────────────────────────────
            maybe_chunk = audio_rx.recv() => {
                match maybe_chunk {
                    Some(chunk) => {
                        pcm_buf.extend_from_slice(&chunk);

                        // Only run inference when we have enough audio
                        if pcm_buf.len() < MIN_BUFFER_BYTES {
                            continue;
                        }

                        // Don't block the async runtime — offload to threadpool
                        let buf_clone = pcm_buf.split_off(0); // drain buffer
                        let samples = pcm16le_to_f32(&buf_clone);

                        let language_selection = settings_rx.borrow().clone();

                        // Run Whisper on a blocking thread
                        let transcript_result = {
                            let whisper_ref = &whisper;
                            tokio::task::block_in_place(|| whisper_ref.transcribe(&samples))
                        };

                        match transcript_result {
                            Ok(Some(transcript)) => {
                                if transcript == last_final {
                                    continue;
                                }
                                last_final = transcript.clone();
                                println!("[STT] {}", transcript);

                                if is_same_language_pair(&language_selection) {
                                    println!("[{}] {}", language_selection.target_language, transcript);
                                    let _ = tx.send(TranslationUpdate {
                                        original: transcript.clone(),
                                        translated: transcript,
                                    });
                                    continue;
                                }

                                // Translate via Groq (still online) or skip if no key
                                if cfg.groq_api_key.is_empty() {
                                    log::warn!("GROQ_API_KEY not set — skipping translation, forwarding transcript as-is");
                                    let _ = tx.send(TranslationUpdate {
                                        original: transcript.clone(),
                                        translated: transcript,
                                    });
                                    continue;
                                }

                                match translate_text(
                                    &http_client,
                                    &cfg.groq_api_key,
                                    &cfg.groq_model,
                                    &transcript,
                                    &language_selection,
                                )
                                .await
                                {
                                    Ok(translated) => {
                                        println!("[{}] {}", language_selection.target_language, translated);
                                        let _ = tx.send(TranslationUpdate {
                                            original: transcript,
                                            translated,
                                        });
                                    }
                                    Err(err) => log::error!("Translation failed: {}", err),
                                }
                            }
                            Ok(None) => {
                                log::debug!("Whisper: no speech detected in this buffer");
                            }
                            Err(err) => {
                                log::error!("Whisper transcription error: {}", err);
                            }
                        }
                    }
                    None => {
                        log::info!("Audio channel closed — shutting down pipeline");
                        break;
                    }
                }
            }

            // ── Language change ────────────────────────────────────────────
            changed = settings_rx.changed() => {
                if changed.is_err() {
                    log::warn!("Settings channel closed");
                    continue;
                }

                let new_lang_label = settings_rx.borrow().spoken_language.clone();
                let new_whisper_lang = WhisperStt::resolve_whisper_language(&new_lang_label).to_string();

                log::info!("Spoken language changed to '{}' ({}); reloading Whisper state", new_lang_label, new_whisper_lang);

                // Reload Whisper with the new language on the blocking pool
                match tokio::task::spawn_blocking(move || WhisperStt::from_env(&new_whisper_lang))
                    .await
                    .map_err(|e| anyhow!("Whisper reload panicked: {}", e))
                {
                    Ok(Ok(new_whisper)) => {
                        whisper = new_whisper;
                        pcm_buf.clear();
                        last_final.clear();
                    }
                    Ok(Err(e)) | Err(e) => {
                        log::error!("Failed to reload Whisper: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert raw 16-bit little-endian PCM bytes to normalized f32 samples.
/// Whisper expects f32 in [-1.0, 1.0] at 16 kHz mono.
fn pcm16le_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|b| {
            let sample = i16::from_le_bytes([b[0], b[1]]);
            sample as f32 / i16::MAX as f32
        })
        .collect()
}

pub fn is_same_language_pair(language_selection: &LanguageSelection) -> bool {
    language_selection
        .spoken_language
        .trim()
        .eq_ignore_ascii_case(language_selection.target_language.trim())
}

/// Kept for backward-compat with `server.rs` (file upload path still uses Deepgram).
pub fn resolve_deepgram_language(spoken_language: &str, fallback_language: &str) -> String {
    match spoken_language {
        "English" => "en".to_string(),
        "Turkish" => "tr".to_string(),
        _ => fallback_language.to_string(),
    }
}

pub async fn translate_text(
    client: &Client,
    groq_api_key: &str,
    groq_model: &str,
    text: &str,
    language_selection: &LanguageSelection,
) -> anyhow::Result<String> {
    let prompt = format!(
        "You are an expert, highly accurate translator. Translate the following text from {} to {}. \
         Make it sound natural and contextual in the target language. \
         CRITICAL: Output ONLY the direct translation. No explanations, no notes, no quotes.",
        language_selection.spoken_language, language_selection.target_language
    );

    let body = serde_json::json!({
        "model": groq_model,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user",   "content": text}
        ],
        "temperature": 0.0
    });

    let response = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .bearer_auth(groq_api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to call Groq API: {}", e))?;

    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Groq response: {}", e))?;

    if !status.is_success() {
        return Err(anyhow!("Groq API error ({}): {}", status, payload));
    }

    let translated = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Groq response did not include translated text"))?;

    Ok(translated)
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn translation_update_serializes_to_expected_json_shape() {
        let update = TranslationUpdate {
            original: "merhaba".to_string(),
            translated: "hello".to_string(),
        };
        let payload = serde_json::to_value(update).unwrap();
        assert_eq!(payload, json!({"original": "merhaba", "translated": "hello"}));
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

    #[test]
    fn pcm16le_to_f32_converts_correctly() {
        // i16::MAX (32767) → ~1.0
        let bytes = 32767i16.to_le_bytes();
        let samples = pcm16le_to_f32(&bytes);
        assert_eq!(samples.len(), 1);
        assert!((samples[0] - 1.0).abs() < 0.001, "got {}", samples[0]);

        // i16 zero → 0.0
        let bytes = 0i16.to_le_bytes();
        let samples = pcm16le_to_f32(&bytes);
        assert_eq!(samples[0], 0.0);
    }

    #[test]
    fn resolve_deepgram_language_maps_known_languages() {
        assert_eq!(resolve_deepgram_language("English", "en"), "en");
        assert_eq!(resolve_deepgram_language("Turkish", "en"), "tr");
        assert_eq!(resolve_deepgram_language("Unknown", "en"), "en");
    }
}
