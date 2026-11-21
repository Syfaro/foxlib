//! Initialize app tracing and otel reporting.

use std::{collections::HashMap, sync::OnceLock};

use opentelemetry::{
    propagation::TextMapCompositePropagator,
    trace::{TraceContextExt, TracerProvider as _},
    KeyValue,
};
use opentelemetry_sdk::{
    propagation::{BaggagePropagator, TraceContextPropagator},
    resource::{EnvResourceDetector, ResourceDetector, TelemetryResourceDetector},
    trace::SdkTracerProvider,
    Resource,
};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Configuration for tracing.
pub struct TracingConfig {
    /// Resource namespace (`service.namespace`).
    pub namespace: &'static str,
    /// Resource name (`service.name`).
    pub name: &'static str,
    /// Resource version (`service.version`).
    pub version: &'static str,

    /// If logs should be output as JSON instead of human-readable text.
    pub json_logs: bool,
}

const UNTRACED: &[&str] = &[
    "h2",
    "hyper",
    "hyper_util",
    "tonic",
    "tower",
    "opentelemetry",
    "opentelemetry_sdk",
    "opentelemetry_otlp",
];

static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// Initialize tracing.
pub fn init(config: TracingConfig) {
    opentelemetry::global::set_text_map_propagator(TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ]));

    let log_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let logs = if config.json_logs {
        let mut json = json_subscriber::fmt::layer()
            .flatten_event(true)
            .with_current_span(false);
        json.inner_layer_mut()
            .add_dynamic_field("trace_id", |_, _| {
                current_span_context().map(|cx| cx.trace_id().to_string())
            });
        json.inner_layer_mut().add_dynamic_field("span_id", |_, _| {
            current_span_context().map(|cx| cx.span_id().to_string())
        });
        json.with_filter(log_filter).boxed()
    } else {
        tracing_subscriber::fmt::layer()
            .with_filter(log_filter)
            .boxed()
    };

    let traces = otlp_enabled().then(|| {
        let provider = tracer_provider(&config);
        let tracer = provider.tracer(config.name);
        opentelemetry::global::set_tracer_provider(provider.clone());
        let _ = TRACER_PROVIDER.set(provider);

        tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(trace_filter())
    });

    tracing_subscriber::registry()
        .with(logs)
        .with(traces)
        .init();
}

/// Send any spans still queued and stop exporting.
pub fn shutdown() {
    if let Some(provider) = TRACER_PROVIDER.get() {
        if let Err(err) = provider.shutdown() {
            eprintln!("could not shut down tracer provider: {err}");
        }
    }
}

/// Get tracing headers to associate with the job.
pub fn tracing_headers() -> HashMap<String, String> {
    let mut headers = HashMap::with_capacity(2);
    let cx = tracing::Span::current().context();

    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut headers)
    });

    headers
}

fn otlp_enabled() -> bool {
    let set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    let is = |name, expected: &str| {
        std::env::var(name).is_ok_and(|value| value.trim().eq_ignore_ascii_case(expected))
    };

    !is("OTEL_SDK_DISABLED", "true")
        && !is("OTEL_TRACES_EXPORTER", "none")
        && (set("OTEL_EXPORTER_OTLP_ENDPOINT") || set("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"))
}

fn trace_filter() -> EnvFilter {
    UNTRACED.iter().fold(
        EnvFilter::try_from_env("RUST_TRACE").unwrap_or_else(|_| EnvFilter::new("info")),
        |filter, target| {
            filter.add_directive(format!("{target}=off").parse().expect("valid directive"))
        },
    )
}

fn tracer_provider(config: &TracingConfig) -> SdkTracerProvider {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("could not create otlp exporter");

    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource(config))
        .build()
}

fn resource(config: &TracingConfig) -> Resource {
    let mut builder = Resource::builder_empty()
        .with_service_name(config.name)
        .with_attributes([
            KeyValue::new("service.namespace", config.namespace),
            KeyValue::new("service.version", config.version),
        ]);

    let detectors: [Box<dyn ResourceDetector>; 2] = [
        Box::new(TelemetryResourceDetector),
        Box::new(EnvResourceDetector::new()),
    ];
    builder = builder.with_detectors(&detectors);

    if let Some(name) = non_empty_env("OTEL_SERVICE_NAME") {
        builder = builder.with_service_name(name);
    }

    builder.build()
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn current_span_context() -> Option<opentelemetry::trace::SpanContext> {
    let cx = opentelemetry::Context::current();
    let span_context = cx.span().span_context().clone();
    span_context.is_valid().then_some(span_context)
}
