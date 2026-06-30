//! WebSocket Bridge for Unity Avatar
//! 
//! This module provides a WebSocket server that Unity WebGL can connect to.
//! When animation events are received from the backend SSE stream,
//! they are broadcast to all connected Unity clients.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use log::{info, warn, error, debug};

/// Animation command to send to Unity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationCommand {
    /// The animation name to play (e.g., "idle", "smile", "think")
    pub animation: String,
    /// Optional parameters for the animation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<AnimationParameters>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blend_duration: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
}

/// WebSocket bridge server
pub struct WebSocketBridge {
    /// Broadcast channel for sending commands to all connected clients
    sender: broadcast::Sender<String>,
    /// Connected clients
    clients: Arc<Mutex<HashMap<SocketAddr, tokio::task::JoinHandle<()>>>>,
    /// Server address
    addr: SocketAddr,
}

impl WebSocketBridge {
    /// Create a new WebSocket bridge
    pub fn new(addr: SocketAddr) -> Self {
        let (sender, _) = broadcast::channel(100);
        Self {
            sender,
            clients: Arc::new(Mutex::new(HashMap::new())),
            addr,
        }
    }

    /// Start the WebSocket server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        info!("WebSocket bridge listening on {}", self.addr);

        let sender = self.sender.clone();
        let clients = self.clients.clone();

        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let sender = sender.clone();
                let clients_clone = clients.clone();
                
                info!("Unity client connected: {}", addr);

                let handle = tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, addr, sender, clients_clone.clone()).await {
                        warn!("Connection error for {}: {}", addr, e);
                    }
                    // Remove client when disconnected
                    clients_clone.lock().await.remove(&addr);
                    info!("Unity client disconnected: {}", addr);
                });

                clients.lock().await.insert(addr, handle);
            }
        });

        Ok(())
    }

    /// Send an animation command to all connected Unity clients
    pub fn send_animation(&self, animation: &str, parameters: Option<AnimationParameters>) {
        let command = AnimationCommand {
            animation: animation.to_string(),
            parameters,
        };

        match serde_json::to_string(&command) {
            Ok(json) => {
                debug!("Broadcasting animation command: {}", json);
                if let Err(e) = self.sender.send(json) {
                    warn!("Failed to broadcast animation command: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to serialize animation command: {}", e);
            }
        }
    }

    /// Get the number of connected clients
    pub async fn client_count(&self) -> usize {
        self.clients.lock().await.len()
    }
}

/// Handle a single WebSocket connection
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    sender: broadcast::Sender<String>,
    _clients: Arc<Mutex<HashMap<SocketAddr, tokio::task::JoinHandle<()>>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();

    // Subscribe to broadcast channel
    let mut receiver = sender.subscribe();

    // Spawn task to forward messages to client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = receiver.recv().await {
            if write.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Read messages from client (for now, just log them)
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received from {}: {}", addr, text);
                    // Could handle commands from Unity here if needed
                }
                Ok(Message::Close(_)) => {
                    info!("Close message from {}", addr);
                    break;
                }
                Err(e) => {
                    warn!("Error reading from {}: {}", addr, e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
        }
        _ = (&mut recv_task) => {
            send_task.abort();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_animation_command_serialization() {
        let cmd = AnimationCommand {
            animation: "smile".to_string(),
            parameters: Some(AnimationParameters {
                loop_mode: Some(false),
                blend_duration: Some(0.3),
                speed: None,
            }),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"animation\":\"smile\""));
        assert!(json.contains("\"loop_mode\":false"));
    }

    #[tokio::test]
    async fn test_websocket_bridge_creation() {
        let addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
        let bridge = WebSocketBridge::new(addr);
        assert_eq!(bridge.client_count().await, 0);
    }
}