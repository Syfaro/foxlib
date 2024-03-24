//! Initialize app tracing and otel reporting.

use std::{borrow::Cow, marker::PhantomData};

use chrono::Utc;
use opentelemetry::trace::{SpanId, TraceContextExt};
use opentelemetry_sdk::{trace, Resource};
use opentelemetry_semantic_conventions as semcov;
use serde::{ser::SerializeMap, Serialize, Serializer};
use tracing::Subscriber;
use tracing_log::NormalizeEvent;
use tracing_opentelemetry::OtelData;
use tracing_serde::{fields::AsMap, AsSerde};
use tracing_subscriber::{
    fmt::{FormatEvent, FormatFields, FormattedFields},
    prelude::*,
    registry::{LookupSpan, SpanRef},
};

/// Configuration for tracing.
pub struct TracingConfig {
    /// Resource namespace.
    pub namespace: &'static str,
    /// Resource name.
    pub name: &'static str,
    /// Resource version.
    pub version: &'static str,

    /// If logs should be output in JSON format and traces sent to otlp pipeline.
    pub otlp: bool,
}

/// Initialize tracing.
pub fn init(config: TracingConfig) {
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );

    let environment = if let Ok(environment) = std::env::var("DEPLOYMENT_ENVIRONMENT") {
        Cow::from(environment)
    } else if cfg!(debug_assertions) {
        Cow::from("dev")
    } else {
        Cow::from("unknown")
    };

    if config.otlp {
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(opentelemetry_otlp::new_exporter().tonic())
            .with_trace_config(trace::config().with_resource(Resource::new([
                semcov::resource::SERVICE_NAMESPACE.string(config.namespace),
                semcov::resource::SERVICE_NAME.string(config.name),
                semcov::resource::SERVICE_VERSION.string(config.version),
                semcov::resource::DEPLOYMENT_ENVIRONMENT.string(environment),
            ])))
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .expect("could not create otlp tracer");

        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .event_format(TraceIdFormat),
            )
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
            .init();
    }
}

struct TraceInfo {
    trace_id: String,
    span_id: String,
}

struct TraceIdFormat;

impl TraceIdFormat {
    fn lookup_trace_info<S>(span_ref: &SpanRef<S>) -> Option<TraceInfo>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        span_ref.extensions().get::<OtelData>().map(|o| TraceInfo {
            trace_id: o.parent_cx.span().span_context().trace_id().to_string(),
            span_id: o.builder.span_id.unwrap_or(SpanId::INVALID).to_string(),
        })
    }
}

impl<S, N> FormatEvent<S, N> for TraceIdFormat
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: tracing_subscriber::fmt::format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let normalized_meta = event.normalized_metadata();
        let meta = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());

        let mut visit = || {
            let mut serializer = serde_json::Serializer::new(WriteAdaptor::new(&mut writer));
            let mut serializer = serializer.serialize_map(None)?;
            serializer.serialize_entry("timestamp", &Utc::now().to_rfc3339())?;
            serializer.serialize_entry("level", &meta.level().as_serde())?;
            serializer.serialize_entry("fields", &event.field_map())?;
            serializer.serialize_entry("target", meta.target())?;

            if let Some(ref span_ref) = ctx.lookup_current() {
                let format_field_marker = PhantomData::<N>;

                serializer
                    .serialize_entry("span", &SerializableSpan(span_ref, format_field_marker))?;

                serializer.serialize_entry(
                    "spans",
                    &SerializableContext(span_ref, format_field_marker),
                )?;

                if let Some(trace_info) = Self::lookup_trace_info(span_ref) {
                    serializer.serialize_entry("span_id", &trace_info.span_id)?;
                    serializer.serialize_entry("trace_id", &trace_info.trace_id)?;
                }
            }

            serializer.end()
        };

        visit().map_err(|_err| std::fmt::Error)?;
        writeln!(writer)
    }
}

struct SerializableContext<'a, 'b, Span, N>(&'b SpanRef<'a, Span>, PhantomData<N>)
where
    Span: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static;

impl<'a, 'b, Span, N> Serialize for SerializableContext<'a, 'b, Span, N>
where
    Span: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serializer = serializer.serialize_seq(None)?;

        for span in self.0.scope().from_root() {
            serializer.serialize_element(&SerializableSpan(&span, self.1))?;
        }

        serializer.end()
    }
}

struct SerializableSpan<'a, 'b, Span, N>(&'b SpanRef<'a, Span>, PhantomData<N>)
where
    Span: for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static;

impl<'a, 'b, Span, N> Serialize for SerializableSpan<'a, 'b, Span, N>
where
    Span: for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut serializer = serializer.serialize_map(None)?;

        let ext = self.0.extensions();
        let data = ext.get::<FormattedFields<N>>().unwrap();

        match serde_json::from_str(data) {
            Ok(serde_json::Value::Object(fields)) => {
                for (key, value) in fields {
                    serializer.serialize_entry(&key, &value)?;
                }
            }
            Ok(value) => {
                serializer.serialize_entry("field", &value)?;
                serializer.serialize_entry("field_error", "field was not valid object")?;
            }
            Err(err) => {
                serializer.serialize_entry("field_error", &err.to_string())?;
            }
        }

        serializer.serialize_entry("name", self.0.metadata().name())?;
        serializer.end()
    }
}

/// A wrapper type to allow
struct WriteAdaptor<'a, W: std::fmt::Write> {
    fmt_write: &'a mut W,
}

impl<'a, W: std::fmt::Write> WriteAdaptor<'a, W> {
    pub fn new(fmt_write: &'a mut W) -> Self {
        Self { fmt_write }
    }
}

impl<'a, W: std::fmt::Write> std::io::Write for WriteAdaptor<'a, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = std::str::from_utf8(buf)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

        self.fmt_write
            .write_str(s)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;

        Ok(s.as_bytes().len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
