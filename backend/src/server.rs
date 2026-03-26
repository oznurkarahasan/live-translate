// Copyright 2026 live-translate
// Licensed under the Apache License, Version 2.0

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};
use tokio::sync::broadcast;
use crate::transcriber::TranslationUpdate;
use tower_http::cors::CorsLayer;

pub async fn start_server(tx: broadcast::Sender<TranslationUpdate>) {
    let app = Router::new()
        .route("/ws", get(move |ws: WebSocketUpgrade| {
            let tx = tx.clone();
            async move {
                ws.on_upgrade(|socket| handle_socket(socket, tx))
            }
        }))
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    log::info!("WebSocket Sunucusu başlatıldı: ws://127.0.0.1:3001/ws");
    
    ax_server(listener, app).await;
}

async fn ax_server(listener: tokio::net::TcpListener, app: Router) {
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