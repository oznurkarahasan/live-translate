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
        .with_state(state)
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

#[cfg(test)]
mod tests {
    use super::serve_with_listener;
    use crate::transcriber::{LanguageSelection, TranslationUpdate};
    use futures_util::StreamExt;
    use serde_json::Value;
    use std::time::Duration;
    use tokio::sync::{broadcast, watch};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    #[tokio::test]
    async fn broadcasts_translation_update_over_websocket() {
        let (tx, _) = broadcast::channel(16);
        let (settings_tx, _) = watch::channel(LanguageSelection {
            spoken_language: "English".to_string(),
            target_language: "Turkish".to_string(),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(serve_with_listener(listener, tx.clone(), settings_tx));

        let url = format!("ws://{}/ws", addr);
        let (mut socket, _) = connect_async(url).await.unwrap();

        tx.send(TranslationUpdate {
            original: "merhaba".to_string(),
            translated: "hello".to_string(),
        })
        .unwrap();

        let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("timed out waiting for websocket message")
            .expect("websocket stream ended")
            .expect("websocket received an error");

        match message {
            Message::Text(text) => {
                let payload: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(payload["original"], "merhaba");
                assert_eq!(payload["translated"], "hello");
            }
            other => panic!("expected text websocket message, got: {:?}", other),
        }

        handle.abort();
    }
}
