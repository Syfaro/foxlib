//! Utilities to expose standard metrics.

use std::{convert::Infallible, net::SocketAddr};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use opentelemetry::{sdk::trace, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use prometheus::{Encoder, TextEncoder};
use tracing_subscriber::prelude::*;

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

/// Configure tracing output format and span reporting.
pub fn configure_tracing(
    agent: &str,
    namespace: &'static str,
    name: &'static str,
    version: &'static str,
    json_logs: bool,
) {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(agent),
        )
        .with_trace_config(
            trace::config()
                .with_sampler(trace::Sampler::AlwaysOn)
                .with_resource(opentelemetry::sdk::Resource::new(vec![
                    KeyValue::new("service.namespace", namespace),
                    KeyValue::new("service.name", name),
                    KeyValue::new("service.version", version),
                ])),
        )
        .install_batch(opentelemetry::runtime::Tokio)
        .expect("could not create otlp tracer");

    if json_logs {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json())
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .event_format(tracing_subscriber::fmt::format().pretty()),
            )
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init();
    }
}
