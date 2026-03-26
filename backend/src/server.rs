// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use crate::transcriber::TranslationUpdate;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

pub async fn start_server(tx: broadcast::Sender<TranslationUpdate>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();
    log::info!("WebSocket Sunucusu başlatıldı: ws://127.0.0.1:3001/ws");

    serve_with_listener(listener, tx).await;
}

async fn serve_with_listener(
    listener: tokio::net::TcpListener,
    tx: broadcast::Sender<TranslationUpdate>,
) {
    let app = Router::new()
        .route(
            "/ws",
            get(move |ws: WebSocketUpgrade| {
                let tx = tx.clone();
                async move { ws.on_upgrade(|socket| handle_socket(socket, tx)) }
            }),
        )
        .layer(CorsLayer::permissive());

    axum::serve(listener, app).await.unwrap();
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
    use crate::transcriber::TranslationUpdate;
    use futures_util::StreamExt;
    use serde_json::Value;
    use std::time::Duration;
    use tokio::sync::broadcast;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    #[tokio::test]
    async fn broadcasts_translation_update_over_websocket() {
        let (tx, _) = broadcast::channel(16);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(serve_with_listener(listener, tx.clone()));

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
