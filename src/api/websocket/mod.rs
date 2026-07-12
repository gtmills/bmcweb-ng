//! WebSocket API handlers
//!
//! Provides WebSocket endpoints matching those in upstream bmcweb:
//!
//! - `/console0`   — Host serial console (connects to host UART via DBus)
//! - `/kvm/0`      — KVM / Remote Frame Buffer (TCP proxy to kvmd on port 5900)
//! - `/vm/0/0`     — Virtual Media (planned)
//! - `/nbd/0`      — NBD virtual media (planned)
//! - `/redfish/events` — Server-Sent Events for Redfish EventService (see event_service.rs)
//!
//! ## Serial Console
//!
//! The serial console endpoint proxies bidirectional byte streams between
//! the WebSocket client and the host OBMC Console service.  The upstream
//! implementation (see `http/websocket.hpp` and `obmc-console`) connects
//! via a UNIX domain socket at `/run/obmc-console/`.  We implement the
//! same socket path convention.
//!
//! ## KVM
//!
//! Upstream bmcweb (`features/kvm/kvm_websocket.hpp`) connects to TCP port
//! 5900 on localhost where `obmc-ikvm` (or another kvmd) listens for VNC/RFB
//! connections.  We implement the same proxy: accept the WebSocket upgrade,
//! connect to `127.0.0.1:5900`, and forward binary frames in both directions.
//! The VNC/RFB protocol is handled entirely by the kvmd daemon and the
//! browser-side noVNC client — we are a transparent TCP proxy.
//!
//! ## Security
//!
//! All WebSocket endpoints require authentication.  The axum `ws` extractor
//! is only reachable after the authentication middleware has run.
//!
//! ## Status
//!
//! | Endpoint         | Status                          |
//! |------------------|---------------------------------|
//! | /console0        | Implemented                     |
//! | /kvm/0           | Implemented (TCP proxy to :5900)|
//! | /vm/0/0          | Not yet started                 |
//! | /nbd/0           | Not yet started                 |
//! | /redfish/events  | Implemented (SSE, event_service)|

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};
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
                Ok(Message::Binary(data)) => {
                    if stream_writer.write_all(&data).await.is_err() {
                        debug!("Failed to write to console socket");
                        break;
                    }
                }
                Ok(Message::Text(text)) => {
                    if stream_writer.write_all(text.as_bytes()).await.is_err() {
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
// KVM
// ---------------------------------------------------------------------------

/// WebSocket upgrade handler for KVM at `/kvm/0`.
///
/// Upstream bmcweb (`features/kvm/kvm_websocket.hpp`) connects to TCP port
/// 5900 on localhost where `obmc-ikvm` runs the VNC/RFB server.  We implement
/// the same transparent proxy: binary WebSocket frames are forwarded as raw
/// TCP bytes in both directions.  The VNC/RFB protocol is handled entirely by
/// the kvmd daemon and the client-side noVNC library.
///
/// # OpenBMC service
///
/// The `obmc-ikvm` daemon listens on `127.0.0.1:5900`.  Ensure the
/// `obmc-ikvm` service is enabled on the BMC for KVM to function.
pub async fn kvm_handler(
    ws: WebSocketUpgrade,
    State(_state): State<Arc<AppState>>,
) -> impl IntoResponse {
    debug!("WebSocket upgrade request for /kvm/0");

    ws.on_upgrade(|socket| handle_kvm(socket))
}

/// Handle an established KVM WebSocket connection.
///
/// Connects to `obmc-ikvm` on `127.0.0.1:5900` and bidirectionally proxies
/// binary frames between the WebSocket and the TCP connection.
async fn handle_kvm(mut ws: WebSocket) {
    info!("KVM WebSocket connection established");

    const KVM_HOST: &str = "127.0.0.1:5900";

    let stream = match TcpStream::connect(KVM_HOST).await {
        Ok(s) => {
            info!("Connected to KVM service at {}", KVM_HOST);
            s
        }
        Err(e) => {
            warn!("Failed to connect to KVM service {}: {}", KVM_HOST, e);
            let _ = ws
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1011,
                    reason: "KVM service unavailable".into(),
                })))
                .await;
            return;
        }
    };

    let (mut tcp_reader, mut tcp_writer) = tokio::io::split(stream);
    let (mut ws_sender, mut ws_receiver) = ws.split();

    // Task: TCP → WebSocket
    let tcp_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => {
                    debug!("KVM TCP connection closed (EOF)");
                    break;
                }
                Ok(n) => {
                    if ws_sender
                        .send(Message::Binary(buf[..n].to_vec()))
                        .await
                        .is_err()
                    {
                        debug!("KVM WebSocket send failed (client disconnected)");
                        break;
                    }
                }
                Err(e) => {
                    error!("Error reading from KVM TCP socket: {}", e);
                    break;
                }
            }
        }
    });

    // Task: WebSocket → TCP
    let ws_to_tcp = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if tcp_writer.write_all(&data).await.is_err() {
                        debug!("Failed to write to KVM TCP socket");
                        break;
                    }
                }
                Ok(Message::Text(text)) => {
                    if tcp_writer.write_all(text.as_bytes()).await.is_err() {
                        debug!("Failed to write to KVM TCP socket");
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    debug!("KVM WebSocket closed by client");
                    break;
                }
                Ok(_) => {} // Ping/Pong handled by axum
                Err(e) => {
                    error!("KVM WebSocket receive error: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for either direction to finish
    tokio::select! {
        _ = tcp_to_ws => {}
        _ = ws_to_tcp => {}
    }

    info!("KVM WebSocket connection closed");
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
