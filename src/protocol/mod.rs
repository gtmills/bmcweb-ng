//! Protocol layer — HTTP/HTTPS server and TLS configuration.
//!
//! Low-level protocol handling.  WebSocket endpoints are registered in
//! `crate::api::websocket` and merged into the main router by `HttpServer`.

pub mod http;
pub use http::HttpServer;
