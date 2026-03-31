// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use crate::transcriber::{LanguageSelection, TranslationUpdate};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use tokio::sync::{broadcast, watch};
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<TranslationUpdate>,
    settings_tx: watch::Sender<LanguageSelection>,
}

pub async fn start_server(
    tx: broadcast::Sender<TranslationUpdate>,
    settings_tx: watch::Sender<LanguageSelection>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();
    log::info!("WebSocket Sunucusu başlatıldı: ws://127.0.0.1:3001/ws");

    serve_with_listener(listener, tx, settings_tx).await;
}

async fn serve_with_listener(
    listener: tokio::net::TcpListener,
    tx: broadcast::Sender<TranslationUpdate>,
    settings_tx: watch::Sender<LanguageSelection>,
) {
    let state = AppState { tx, settings_tx };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/settings", get(get_settings).post(update_settings))
        .route("/upload", axum::routing::post(handle_upload))
        .with_state(state)
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB limit
        .layer(CorsLayer::permissive());

    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
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
    pub text: String,
}

use axum::extract::Multipart;
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

    let config = crate::transcriber::Phase2Config::from_env().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Config error: {}", e),
        )
    })?;
    let settings = state.settings_tx.borrow().clone();
    let stt_lang = crate::transcriber::resolve_deepgram_language(&settings.spoken_language, "en");

    let client = reqwest::Client::new();
    let dg_url = format!("https://api.deepgram.com/v1/listen?smart_format=true&utterances=true&punctuate=true&model={}&language={}", config.deepgram_model, stt_lang);

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

    let mut subtitles = Vec::new();
    let mut phrases = Vec::new();

    for utt in utterances {
        let text = utt.get("transcript").and_then(|t| t.as_str()).unwrap_or("");
        let start = utt.get("start").and_then(|s| s.as_f64()).unwrap_or(0.0);
        let end = utt.get("end").and_then(|e| e.as_f64()).unwrap_or(0.0);

        if !text.is_empty() {
            phrases.push((start, end, text.to_string()));
        }
    }

    // Now translate them sequentially or run into rate limits, let's just do sequential for reliability but maybe batched if fast.
    let same_lang = crate::transcriber::is_same_language_pair(&settings);

    for (start, end, text) in phrases {
        let mut final_text = text.clone();
        if !same_lang && !text.trim().is_empty() {
            if let Ok(translated) = crate::transcriber::translate_text(
                &client,
                &config.groq_api_key,
                &config.groq_model,
                &text,
                &settings,
            )
            .await
            {
                final_text = translated;
            }
        }
        subtitles.push(Subtitle {
            start,
            end,
            text: final_text,
        });
    }

    Ok(Json(subtitles))
}
