//! Utilities to expose standard metrics.

use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use prometheus::{Encoder, TextEncoder};

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

        Self::start_server(metrics_server.clone(), host).await;

        metrics_server
    }

    /// Set the ready state.
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Relaxed);
    }

    async fn start_server(metrics_server: Arc<Self>, host: SocketAddr) {
        let make_svc = make_service_fn(move |_conn| {
            let metrics_server = metrics_server.clone();

            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let metrics_server = metrics_server.clone();
                    async move { metrics_server.handler(req).await }
                }))
            }
        });

        let server = Server::bind(&host).serve(make_svc);

        tokio::spawn(async move {
            server.await.expect("metrics server failure");
        });
    }

    async fn handler(&self, req: Request<Body>) -> Result<Response<Body>, Infallible> {
        match req.uri().path() {
            "/health/live" => Ok(Response::new(Body::from("OK"))),
            "/health/ready" if self.ready.load(Ordering::Relaxed) => {
                Ok(Response::new(Body::from("READY")))
            }
            "/health/ready" => {
                let mut resp = Response::new(Body::from("NOT READY"));
                *resp.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                Ok(resp)
            }
            "/metrics" => {
                let mut buffer = Vec::new();
                TextEncoder::default()
                    .encode(&prometheus::gather(), &mut buffer)
                    .expect("could not encode metrics");

                Ok(Response::new(Body::from(buffer)))
            }
            _ => {
                let mut not_found = Response::new(Body::default());
                *not_found.status_mut() = StatusCode::NOT_FOUND;
                Ok(not_found)
            }
        }
    }
}
