// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0
//
// Local STT engine backed by whisper.cpp via whisper-rs.
// No internet connection required at runtime.
// The GGML model file must exist at the path given by the `WHISPER_MODEL`
// environment variable (default: `models/ggml-base.bin`).

use anyhow::Context;
use std::path::PathBuf;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Holds the loaded Whisper model context.
/// Constructing this is expensive (~200 ms for base); keep one instance alive
/// for the lifetime of the process and call `transcribe` repeatedly.
pub struct WhisperStt {
    ctx: WhisperContext,
    language: String,
}

impl WhisperStt {
    /// Load the model. Reads `WHISPER_MODEL` env var or falls back to
    /// `models/ggml-base.bin` relative to the working directory.
    pub fn from_env(language: &str) -> anyhow::Result<Self> {
        let model_path: PathBuf = std::env::var("WHISPER_MODEL")
            .unwrap_or_else(|_| "models/ggml-base.bin".to_string())
            .into();

        log::info!("Loading Whisper model from {:?}", model_path);

        if !model_path.exists() {
            anyhow::bail!(
                "Whisper model not found at {:?}. \
                 Download it with:\n  \
                 mkdir -p models && \
                 curl -L -o {:?} \
                 https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
                model_path,
                model_path
            );
        }

        let ctx = WhisperContext::new_with_params(
            model_path.to_str().context("Non-UTF8 model path")?,
            WhisperContextParameters::default(),
        )
        .context("Failed to load Whisper model")?;

        log::info!("Whisper model loaded (language: {})", language);

        Ok(Self {
            ctx,
            language: language.to_string(),
        })
    }

    /// Transcribe a buffer of **16 kHz mono f32** samples.
    ///
    /// Returns `None` when the audio is too short or contains no speech.
    pub fn transcribe(&self, samples: &[f32]) -> anyhow::Result<Option<String>> {
        // whisper-rs requires at least a few hundred milliseconds of audio.
        // 16000 samples = 1 second; bail early on tiny buffers.
        if samples.len() < 1600 {
            return Ok(None);
        }

        let mut state = self.ctx.create_state().context("Failed to create Whisper state")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(&self.language));
        // Suppress blank audio and reduce hallucination on silent segments
        params.set_no_speech_thold(0.6);
        params.set_logprob_thold(-1.0);
        // Single-threaded for determinism; bump if you have CPU headroom
        params.set_n_threads(
            std::thread::available_parallelism()
                .map(|n| (n.get() / 2).max(1) as i32)
                .unwrap_or(2),
        );
        // Disable token timestamps (we don't need word-level timing here)
        params.set_token_timestamps(false);
        // Translate to English internally — we handle translation ourselves
        params.set_translate(false);

        state
            .full(params, samples)
            .context("Whisper inference failed")?;

        let n_segments = state.full_n_segments().context("Failed to get segment count")?;

        let mut transcript = String::new();
        for i in 0..n_segments {
            let seg = state
                .full_get_segment_text(i)
                .context("Failed to get segment text")?;
            transcript.push_str(seg.trim());
            transcript.push(' ');
        }

        let transcript = transcript.trim().to_string();

        if transcript.is_empty() || transcript == "[BLANK_AUDIO]" {
            return Ok(None);
        }

        Ok(Some(transcript))
    }


    /// Convert a spoken-language label (as used in the UI) to a Whisper
    /// language code. Falls back to `"en"` for unknown labels.
    pub fn resolve_whisper_language(spoken_language: &str) -> &'static str {
        match spoken_language {
            "Turkish" => "tr",
            "English" => "en",
            "German" => "de",
            "French" => "fr",
            "Spanish" => "es",
            "Italian" => "it",
            "Portuguese" => "pt",
            "Russian" => "ru",
            "Japanese" => "ja",
            "Chinese" => "zh",
            "Arabic" => "ar",
            _ => "en",
        }
    }
}
