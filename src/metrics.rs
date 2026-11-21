//! Utilities to expose standard metrics.

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use axum::{
    extract::State, http::header, http::StatusCode, response::IntoResponse, routing::get, Router,
};
use prometheus::{Encoder, TextEncoder};
use tokio::net::TcpListener;

/// Manage a HTTP server on host to expose metrics endpoints.
/// * `/health/live` always returns a 200 OK
/// * `/health/ready` returns 200 or 503 depending on ready state
/// * `/metrics` returns all discovered Prometheus metrics
pub struct MetricsServer {
    ready: AtomicBool,
}

impl MetricsServer {
    /// Start a server in the given ready state.
    pub async fn serve(host: SocketAddr, ready: bool) -> Arc<Self> {
        let metrics_server = Arc::new(MetricsServer {
            ready: AtomicBool::new(ready),
        });

        let router = Router::new()
            .route("/health/live", get(|| async { "OK" }))
            .route("/health/ready", get(Self::ready))
            .route("/metrics", get(Self::metrics))
            .with_state(metrics_server.clone());

        let listener = TcpListener::bind(host)
            .await
            .expect("could not bind metrics server");
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("metrics server failure");
        });

        metrics_server
    }

    /// Set the ready state.
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Relaxed);
    }

    async fn ready(State(server): State<Arc<Self>>) -> impl IntoResponse {
        if server.ready.load(Ordering::Relaxed) {
            (StatusCode::OK, "READY")
        } else {
            (StatusCode::SERVICE_UNAVAILABLE, "NOT READY")
        }
    }

    async fn metrics() -> impl IntoResponse {
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        encoder
            .encode(&prometheus::gather(), &mut buffer)
            .expect("could not encode metrics");

        (
            [(header::CONTENT_TYPE, encoder.format_type().to_owned())],
            buffer,
        )
    }
}
