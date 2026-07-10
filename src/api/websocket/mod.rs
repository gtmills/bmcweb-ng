//! WebSocket API handlers
//!
//! Provides WebSocket endpoints matching those in upstream bmcweb:
//!
//! - `/console0`   — Host serial console (connects to host UART via DBus)
//! - `/kvm/0`      — KVM / Remote Frame Buffer (planned)
//! - `/vm/0/0`     — Virtual Media (planned)
//! - `/nbd/0`      — NBD virtual media (planned)
//! - `/redfish/events` — Server-Sent Events for Redfish EventService (planned)
//!
//! ## Serial Console
//!
//! The serial console endpoint proxies bidirectional byte streams between
//! the WebSocket client and the host OBMC Console service.  The upstream
//! implementation (see `http/websocket.hpp` and `obmc-console`) connects
//! via a UNIX domain socket at `/run/obmc-console/`.  We implement the
//! same socket path convention.
//!
//! ## Security
//!
//! All WebSocket endpoints require authentication.  The axum `ws` extractor
//! is only reachable after the authentication middleware has run.
//!
//! ## Status
//!
//! | Endpoint         | Status                |
//! |------------------|-----------------------|
//! | /console0        | ✅ Implemented        |
//! | /kvm/0           | ⚠️  Stub (TODO)       |
//! | /vm/0/0          | ❌ Not yet started    |
//! | /nbd/0           | ❌ Not yet started    |
//! | /redfish/events  | ⚠️  SSE Stub (TODO)   |

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    http::StatusCode,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, error, info, warn};

use crate::AppState;

// ---------------------------------------------------------------------------
// Router helper
// ---------------------------------------------------------------------------

/// Register WebSocket routes onto an existing axum [`Router`].
///
/// Call this from [`crate::protocol::http::HttpServer::build_router`].
pub fn websocket_routes() -> axum::Router<Arc<AppState>> {
    use axum::routing::get;

    axum::Router::new()
        .route("/console0", get(serial_console_handler))
        .route("/kvm/0", get(kvm_handler))
}

// ---------------------------------------------------------------------------
// Serial Console
// ---------------------------------------------------------------------------

/// WebSocket upgrade handler for the host serial console at `/console0`.
///
/// Upstream bmcweb connects to the obmc-console UNIX socket at
/// `/run/obmc-console/default`.  We follow the same convention.
pub async fn serial_console_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    debug!("WebSocket upgrade request for /console0");

    ws.on_upgrade(|socket| handle_serial_console(socket, state))
}

/// Handle an established serial console WebSocket connection.
///
/// Connects to the obmc-console UNIX socket and bidirectionally proxies
/// data between the WebSocket and the socket.
async fn handle_serial_console(mut ws: WebSocket, _state: Arc<AppState>) {
    info!("Serial console WebSocket connection established");

    // Connect to the obmc-console UNIX socket
    const CONSOLE_SOCKET: &str = "/run/obmc-console/default";

    let stream = match UnixStream::connect(CONSOLE_SOCKET).await {
        Ok(s) => {
            info!("Connected to obmc-console socket at {}", CONSOLE_SOCKET);
            s
        }
        Err(e) => {
            warn!(
                "Failed to connect to obmc-console socket {}: {}",
                CONSOLE_SOCKET, e
            );
            // Send an error close frame to the client
            let _ = ws
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1011,
                    reason: "Console service unavailable".into(),
                })))
                .await;
            return;
        }
    };

    let (mut stream_reader, mut stream_writer) = tokio::io::split(stream);

    // Spawn a task to read from the UNIX socket and write to the WebSocket
    let (mut ws_sender, mut ws_receiver) = ws.split();

    // Task: UNIX socket → WebSocket
    let socket_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            match stream_reader.read(&mut buf).await {
                Ok(0) => {
                    debug!("Console socket closed (EOF)");
                    break;
                }
                Ok(n) => {
                    if ws_sender
                        .send(Message::Binary(buf[..n].to_vec()))
                        .await
                        .is_err()
                    {
                        debug!("WebSocket send failed (client disconnected)");
                        break;
                    }
                }
                Err(e) => {
                    error!("Error reading from console socket: {}", e);
                    break;
                }
            }
        }
    });

    // Task: WebSocket → UNIX socket
    let ws_to_socket = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) | Ok(Message::Text(data)) => {
                    let bytes: &[u8] = if let Message::Binary(ref b) = msg.unwrap_or(Message::Binary(data.clone())) {
                        b
                    } else {
                        data.as_bytes()
                    };
                    if stream_writer.write_all(bytes).await.is_err() {
                        debug!("Failed to write to console socket");
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    debug!("WebSocket closed by client");
                    break;
                }
                Ok(_) => {} // Ping/Pong handled by axum
                Err(e) => {
                    error!("WebSocket receive error: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for either direction to finish
    tokio::select! {
        _ = socket_to_ws => {}
        _ = ws_to_socket => {}
    }

    info!("Serial console WebSocket connection closed");
}

// ---------------------------------------------------------------------------
// KVM (stub)
// ---------------------------------------------------------------------------

/// WebSocket upgrade handler for KVM at `/kvm/0`.
///
/// The upstream bmcweb KVM implementation proxies the RFB (VNC) frame
/// buffer protocol over a WebSocket, connecting to a kvmd UNIX socket.
///
/// TODO: Implement full KVM proxying.  The frame buffer protocol requires:
///   1. Connecting to the KVM UNIX socket or /dev/fb device
///   2. Negotiating RFB version and security type
///   3. Forwarding FramebufferUpdate and PointerEvent/KeyEvent messages
pub async fn kvm_handler(
    ws: WebSocketUpgrade,
    State(_state): State<Arc<AppState>>,
) -> impl IntoResponse {
    debug!("WebSocket upgrade request for /kvm/0");

    ws.on_upgrade(|mut socket| async move {
        info!("KVM WebSocket connection established (not yet implemented)");
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1011,
                reason: "KVM not yet implemented".into(),
            })))
            .await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_routes_registration() {
        // Verify that the router can be constructed without panic
        let _router = websocket_routes();
    }
}
