//! Utilities to expose standard metrics.

use std::{convert::Infallible, net::SocketAddr};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use prometheus::{Encoder, TextEncoder};

/// Start a HTTP server on host to expose metrics endpoints.
/// * `/health` always returns a 200 OK
/// * `/metrics` returns all discovered Prometheus metrics
pub async fn serve(host: SocketAddr) {
    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(metrics)) });

    let server = Server::bind(&host).serve(make_svc);

    tokio::spawn(async move {
        server.await.expect("metrics server failure");
    });
}

async fn metrics(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match req.uri().path() {
        "/health" => Ok(Response::new(Body::from("OK"))),
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
