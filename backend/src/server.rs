// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use crate::transcriber::{LanguageSelection, Phase2Config, TranslationUpdate};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{ConnectInfo, Multipart, Query, Request, State},
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, watch};
use tower_http::cors::{AllowOrigin, CorsLayer};

// ── Per-IP fixed-window rate limiter ─────────────────────────────────────────

struct IpRateLimiter {
    max_requests: u32,
    window: Duration,
    state: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

impl IpRateLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            max_requests,
            window: Duration::from_secs(window_secs),
            state: Mutex::new(HashMap::new()),
        }
    }

    fn allow(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut map = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let entry = map.entry(ip).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window {
            *entry = (1, now);
            return true;
        }
        if entry.0 >= self.max_requests {
            return false;
        }
        entry.0 += 1;
        true
    }
}

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<TranslationUpdate>,
    settings_tx: watch::Sender<LanguageSelection>,
    config: Arc<Phase2Config>,
}

// ── Server entry point ────────────────────────────────────────────────────────

pub async fn start_server(
    tx: broadcast::Sender<TranslationUpdate>,
    settings_tx: watch::Sender<LanguageSelection>,
    config: Arc<Phase2Config>,
) {
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:3001").await {
        Ok(l) => l,
        Err(e) => {
            log::error!(
                "Cannot bind 127.0.0.1:3001 — {e}. \
                 Kill the previous backend process (e.g. `pkill backend`) and restart."
            );
            return;
        }
    };
    log::info!("WebSocket Sunucusu başlatıldı: ws://127.0.0.1:3001/ws");
    serve_with_listener(listener, tx, settings_tx, config).await;
}

async fn serve_with_listener(
    listener: tokio::net::TcpListener,
    tx: broadcast::Sender<TranslationUpdate>,
    settings_tx: watch::Sender<LanguageSelection>,
    config: Arc<Phase2Config>,
) {
    // ── CORS ─────────────────────────────────────────────────────────────────
    let cors = match std::env::var("ALLOWED_ORIGIN") {
        Ok(origin) => {
            let origin_value: HeaderValue = origin
                .parse()
                .expect("ALLOWED_ORIGIN is not a valid HTTP header value");
            log::info!("CORS: strict — only '{}' is allowed", origin);
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(origin_value))
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([header::CONTENT_TYPE, HeaderName::from_static("x-api-key")])
        }
        Err(_) => {
            log::warn!(
                "ALLOWED_ORIGIN not set — using permissive CORS. \
                 Set ALLOWED_ORIGIN in .env before deploying to production."
            );
            CorsLayer::permissive()
        }
    };

    // ── Rate limiter: 60 requests / IP / 60 s ────────────────────────────────
    let rate_limiter = Arc::new(IpRateLimiter::new(60, 60));

    let state = AppState { tx, settings_tx, config };

    // ── Router ───────────────────────────────────────────────────────────────
    let protected = Router::new()
        .route("/settings", get(get_settings).post(update_settings))
        .route("/upload", post(handle_upload))
        .route("/api/translate-youtube", post(handle_youtube))
        .layer(middleware::from_fn(require_api_key));

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .merge(protected)
        .with_state(state)
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            let limiter = Arc::clone(&rate_limiter);
            async move {
                let ip = req
                    .extensions()
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ci| ci.0.ip())
                    .unwrap_or(IpAddr::from([127, 0, 0, 1]));
                if limiter.allow(ip) {
                    next.run(req).await
                } else {
                    StatusCode::TOO_MANY_REQUESTS.into_response()
                }
            }
        }))
        .layer(axum::extract::DefaultBodyLimit::max(300 * 1024 * 1024))
        .layer(cors);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

// ── Middleware ────────────────────────────────────────────────────────────────

async fn require_api_key(req: Request, next: Next) -> Response {
    let expected = std::env::var("API_KEY").unwrap_or_default();
    if !expected.is_empty() {
        let provided = req
            .headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if provided != expected {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    next.run(req).await
}

// ── Shared STT + translate helper ─────────────────────────────────────────────
//
// Used by both the file-upload and YouTube endpoints.  Sends `audio_bytes` to
// Deepgram's REST API (with optional MIME `content_type`), parses the utterance
// timestamps, translates each utterance via Groq, and returns the subtitle list.

#[derive(serde::Serialize)]
pub struct Subtitle {
    pub start: f64,
    pub end: f64,
    pub original: String, // source-language transcript from Deepgram
    pub text: String,     // translated text (or original when same-language pair)
}

async fn transcribe_and_translate(
    audio_bytes: Vec<u8>,
    content_type: Option<String>,
    config: &Phase2Config,
    settings: &LanguageSelection,
) -> Result<Vec<Subtitle>, (StatusCode, String)> {
    let stt_lang = crate::transcriber::resolve_deepgram_language(&settings.spoken_language, "en");

    let client = reqwest::Client::new();
    let dg_url = format!(
        "https://api.deepgram.com/v1/listen?smart_format=true&utterances=true&punctuate=true&model={}&language={}",
        config.deepgram_model, stt_lang
    );

    let mut req_builder = client
        .post(&dg_url)
        .header("Authorization", format!("Token {}", config.deepgram_api_key));

    if let Some(ct) = content_type {
        if !ct.is_empty() {
            req_builder = req_builder.header("Content-Type", ct);
        }
    }

    let dg_res = req_builder
        .body(audio_bytes)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Deepgram API: {}", e)))?;

    let dg_json: serde_json::Value = dg_res
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Parse Deepgram JSON: {}", e)))?;

    let utterances = dg_json
        .pointer("/results/utterances")
        .and_then(|u| u.as_array())
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No utterances in transcript".to_string()))?;

    let mut phrases = Vec::new();
    for utt in utterances {
        let text = utt.get("transcript").and_then(|t| t.as_str()).unwrap_or("");
        let start = utt.get("start").and_then(|s| s.as_f64()).unwrap_or(0.0);
        let end = utt.get("end").and_then(|e| e.as_f64()).unwrap_or(0.0);
        if !text.is_empty() {
            phrases.push((start, end, text.to_string()));
        }
    }

    let same_lang = crate::transcriber::is_same_language_pair(settings);
    let mut subtitles = Vec::new();

    for (start, end, text) in phrases {
        let mut final_text = text.clone();
        if !same_lang && !text.trim().is_empty() {
            if let Ok(translated) = crate::transcriber::translate_text(
                &client,
                &config.groq_api_key,
                &config.groq_model,
                &text,
                settings,
                &[],
            )
            .await
            {
                final_text = translated;
            }
        }
        subtitles.push(Subtitle { start, end, original: text, text: final_text });
    }

    Ok(subtitles)
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> Response {
    let expected = std::env::var("API_KEY").unwrap_or_default();
    if !expected.is_empty() {
        let provided = params.get("key").map(String::as_str).unwrap_or_default();
        if provided != expected {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state.tx))
}

async fn get_settings(State(state): State<AppState>) -> Json<LanguageSelection> {
    Json(state.settings_tx.borrow().clone())
}

async fn update_settings(
    State(state): State<AppState>,
    Json(payload): Json<LanguageSelection>,
) -> Result<Json<LanguageSelection>, (StatusCode, String)> {
    let spoken_language = payload.spoken_language.trim();
    let target_language = payload.target_language.trim();

    if spoken_language.is_empty() || target_language.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "spoken_language and target_language are required".into()));
    }
    if !matches!(spoken_language, "English" | "Turkish")
        || !matches!(target_language, "English" | "Turkish")
    {
        return Err((StatusCode::BAD_REQUEST, "Only English and Turkish are supported for now".into()));
    }

    let normalized = LanguageSelection {
        spoken_language: spoken_language.to_string(),
        target_language: target_language.to_string(),
    };
    state.settings_tx.send(normalized.clone()).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update language settings".into())
    })?;
    Ok(Json(normalized))
}

async fn handle_socket(mut socket: WebSocket, tx: broadcast::Sender<TranslationUpdate>) {
    let mut rx = tx.subscribe();
    log::info!("Yeni bir frontend bağlantısı kabul edildi.");
    while let Ok(update) = rx.recv().await {
        if let Ok(json) = serde_json::to_string(&update) {
            if socket.send(Message::Text(json)).await.is_err() {
                log::warn!("Frontend bağlantısı koptu.");
                break;
            }
        }
    }
}

async fn handle_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Vec<Subtitle>>, (StatusCode, String)> {
    let mut file_data = Vec::new();
    let mut content_type = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            if let Some(ct) = field.content_type() {
                content_type = ct.to_string();
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Bytes error: {}", e)))?;
            file_data = bytes.to_vec();
            break;
        }
    }

    if file_data.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No file uploaded".into()));
    }

    let settings = state.settings_tx.borrow().clone();
    let subtitles = transcribe_and_translate(
        file_data,
        Some(content_type),
        &state.config,
        &settings,
    )
    .await?;

    Ok(Json(subtitles))
}

// ── YouTube translation endpoint ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct YoutubeRequest {
    url: String,
}

/// Validates a YouTube URL and returns the trimmed value.
/// Accepts https://youtube.com/watch?v=ID and https://youtu.be/ID only.
/// The URL is later passed as a separate argv element to yt-dlp (no shell
/// interpretation), so this validation is defence-in-depth, not the only guard.
fn validate_youtube_url(url: &str) -> Result<String, (StatusCode, String)> {
    let url = url.trim().to_string();

    // Reject any URL that isn't HTTPS — no HTTP, no data:, no javascript:
    if !url.starts_with("https://") {
        return Err((StatusCode::BAD_REQUEST, "URL must use HTTPS".into()));
    }

    let is_watch = url.starts_with("https://www.youtube.com/watch?")
        || url.starts_with("https://youtube.com/watch?");
    let is_short = url.starts_with("https://youtu.be/");

    if !is_watch && !is_short {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only https://youtube.com/watch?v=... and https://youtu.be/... URLs are accepted".into(),
        ));
    }

    // Reject shell metacharacters as a second layer of defence.
    if url.chars().any(|c| matches!(c, '\n' | '\r' | '\x00' | ';' | '|' | '`' | '$' | '\\' | '\'' | '"')) {
        return Err((StatusCode::BAD_REQUEST, "URL contains disallowed characters".into()));
    }

    Ok(url)
}

/// Extracts the video ID from a validated YouTube URL and reconstructs a
/// canonical https://www.youtube.com/watch?v=ID link, stripping tracking
/// parameters (?si=..., &utm_source=..., etc.) that can confuse yt-dlp's
/// parser and trigger bot-detection heuristics.
fn sanitize_youtube_url(url: &str) -> Result<String, (StatusCode, String)> {
    let video_id = if url.starts_with("https://youtu.be/") {
        // Path segment after the domain, up to any '?' or '/'.
        let after = &url["https://youtu.be/".len()..];
        after.split(&['?', '/'][..]).next().unwrap_or("").to_string()
    } else {
        // watch? URL — pull only the 'v' query parameter.
        let query_start = url.find('?').map(|i| i + 1).unwrap_or(url.len());
        url[query_start..]
            .split('&')
            .find_map(|kv| {
                let mut parts = kv.splitn(2, '=');
                let key = parts.next()?;
                let val = parts.next()?;
                (key == "v").then(|| val.to_string())
            })
            .unwrap_or_default()
    };

    // YouTube IDs are exactly 11 chars: alphanumeric, '-', '_'.
    if video_id.len() != 11
        || !video_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Could not extract a valid video ID from URL: {url}"),
        ));
    }

    Ok(format!("https://www.youtube.com/watch?v={video_id}"))
}

async fn handle_youtube(
    State(state): State<AppState>,
    Json(payload): Json<YoutubeRequest>,
) -> Result<Json<Vec<Subtitle>>, (StatusCode, String)> {
    let url = validate_youtube_url(&payload.url)?;
    let clean_url = sanitize_youtube_url(&url)?;

    log::info!("YouTube translation requested: {} (sanitized from: {})", clean_url, url);

    // Download audio-only stream via yt-dlp to stdout.
    // stderr is piped so we can print the raw bytes on failure — the inherited
    // approach silenced errors when the pipe was full.  The URL is passed as a
    // discrete argv element, never interpolated into a shell string.
    let download = tokio::process::Command::new("yt-dlp")
        .args([
            "--no-warnings",
            "--allow-unplayable-formats",
            "--extractor-args", "youtube:player_client=web,android",
            "-f", "ba/ba*",
            "-o", "-",
            &clean_url,
        ])
        .output();

    let output = tokio::time::timeout(Duration::from_secs(300), download)
        .await
        .map_err(|_| {
            log::error!("yt-dlp timed out after 5 minutes for: {}", clean_url);
            (StatusCode::GATEWAY_TIMEOUT, "yt-dlp timed out (5 min limit)".into())
        })?
        .map_err(|e| {
            let hint = if e.kind() == std::io::ErrorKind::NotFound {
                " — install yt-dlp: pip install yt-dlp"
            } else {
                ""
            };
            log::error!("yt-dlp spawn error{hint}: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("yt-dlp failed{hint}: {e}"))
        })?;

    if !output.status.success() {
        eprintln!("yt-dlp raw stderr: {}", String::from_utf8_lossy(&output.stderr));
        log::error!("yt-dlp exited {} for {}", output.status, clean_url);
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("yt-dlp exited {}; see backend terminal for raw stderr", output.status),
        ));
    }

    let audio_bytes = output.stdout;
    if audio_bytes.is_empty() {
        eprintln!("yt-dlp raw stderr: {}", String::from_utf8_lossy(&output.stderr));
        log::error!("yt-dlp produced no audio bytes for: {}", clean_url);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "yt-dlp produced no audio output".into()));
    }

    log::info!("Downloaded {} bytes of audio for {}", audio_bytes.len(), clean_url);

    // No Content-Type: Deepgram auto-detects m4a / opus / webm.
    let settings = state.settings_tx.borrow().clone();
    let subtitles = transcribe_and_translate(audio_bytes, None, &state.config, &settings).await?;

    Ok(Json(subtitles))
}
