// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use crate::transcriber::{LanguageSelection, Phase2Config, TranslationUpdate};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{ConnectInfo, Multipart, Query, Request, State},
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
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
//
// No external crates — uses only std primitives.
// State is a HashMap guarded by a Mutex; entries are (request_count, window_start).
// On each request: if the window has expired, reset and allow; if the counter is
// below the cap, increment and allow; otherwise reject with 429.

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
            // Returning here drops `settings_tx`, which signals the transcriber
            // pipeline to stop cleanly via the watch channel error path.
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
    // Production: set ALLOWED_ORIGIN in .env to enforce a single exact origin.
    // Development (ALLOWED_ORIGIN unset): fall back to permissive so that
    // both http://localhost:3000 and http://127.0.0.1:3000 work without config.
    let cors = match std::env::var("ALLOWED_ORIGIN") {
        Ok(origin) => {
            let origin_value: HeaderValue = origin
                .parse()
                .expect("ALLOWED_ORIGIN is not a valid HTTP header value");
            log::info!("CORS: strict — only '{}' is allowed", origin);
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(origin_value))
                .allow_methods([Method::GET, Method::POST])
                // x-api-key must be listed so the preflight OPTIONS allows it.
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

    let state = AppState {
        tx,
        settings_tx,
        config,
    };

    // ── Router ───────────────────────────────────────────────────────────────
    //
    // /ws        — WebSocket; browsers cannot send custom headers during the
    //              upgrade handshake, so we accept an optional ?key= query param
    //              and rely on CORS for browser-side origin enforcement.
    //
    // /settings  — REST; requires x-api-key header.
    // /upload    — REST; requires x-api-key header.
    //
    // Both REST routes and /ws are covered by the global rate limiter and the
    // strict CORS policy defined above.

    let protected = Router::new()
        .route("/settings", get(get_settings).post(update_settings))
        .route("/upload", axum::routing::post(handle_upload))
        .layer(middleware::from_fn(require_api_key));

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .merge(protected)
        .with_state(state)
        // Per-IP rate limiter applied globally (captures rate_limiter by Arc clone).
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

    // into_make_service_with_connect_info populates ConnectInfo<SocketAddr> in
    // request extensions so the rate-limiter middleware can read the client IP.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Validates the `x-api-key` request header against the `API_KEY` env var.
/// When `API_KEY` is not set the check is skipped (safe for local development).
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

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> Response {
    // Browsers cannot set custom headers on WebSocket upgrades.
    // Accept an optional ?key=<secret> query param as a lightweight guard;
    // CORS prevents cross-origin browser connections regardless.
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
        return Err((
            StatusCode::BAD_REQUEST,
            "spoken_language and target_language are required".to_string(),
        ));
    }

    if !matches!(spoken_language, "English" | "Turkish")
        || !matches!(target_language, "English" | "Turkish")
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only English and Turkish are supported for now".to_string(),
        ));
    }

    let normalized = LanguageSelection {
        spoken_language: spoken_language.to_string(),
        target_language: target_language.to_string(),
    };

    state.settings_tx.send(normalized.clone()).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update language settings".to_string(),
        )
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

#[derive(serde::Serialize)]
pub struct Subtitle {
    pub start: f64,
    pub end: f64,
    pub original: String, // source-language transcript from Deepgram
    pub text: String,     // translated text (equals original when same-language pair)
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
        log::info!("Received multipart field: {:?}", field.name());
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

    let config = &state.config;
    let settings = state.settings_tx.borrow().clone();
    let stt_lang = crate::transcriber::resolve_deepgram_language(&settings.spoken_language, "en");

    let client = reqwest::Client::new();
    let dg_url = format!(
        "https://api.deepgram.com/v1/listen?smart_format=true&utterances=true&punctuate=true&model={}&language={}",
        config.deepgram_model, stt_lang
    );

    let mut req_builder = client.post(&dg_url).header(
        "Authorization",
        format!("Token {}", config.deepgram_api_key),
    );

    if !content_type.is_empty() {
        req_builder = req_builder.header("Content-Type", content_type);
    }

    let dg_res = req_builder
        .body(reqwest::Body::from(file_data))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Deepgram API: {}", e),
            )
        })?;

    let dg_json: serde_json::Value = dg_res.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Parse Deepgram JSON: {}", e),
        )
    })?;

    let utterances = dg_json
        .pointer("/results/utterances")
        .and_then(|u| u.as_array())
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "No utterances in transcript".to_string(),
        ))?;

    let mut phrases = Vec::new();
    for utt in utterances {
        let text = utt.get("transcript").and_then(|t| t.as_str()).unwrap_or("");
        let start = utt.get("start").and_then(|s| s.as_f64()).unwrap_or(0.0);
        let end = utt.get("end").and_then(|e| e.as_f64()).unwrap_or(0.0);
        if !text.is_empty() {
            phrases.push((start, end, text.to_string()));
        }
    }

    let same_lang = crate::transcriber::is_same_language_pair(&settings);
    let mut subtitles = Vec::new();

    for (start, end, text) in phrases {
        let mut final_text = text.clone();
        if !same_lang && !text.trim().is_empty() {
            if let Ok(translated) = crate::transcriber::translate_text(
                &client,
                &config.groq_api_key,
                &config.groq_model,
                &text,
                &settings,
                &[],
            )
            .await
            {
                final_text = translated;
            }
        }
        subtitles.push(Subtitle {
            start,
            end,
            original: text,
            text: final_text,
        });
    }

    Ok(Json(subtitles))
}
