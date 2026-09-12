use std::time::Instant;
use std::{fs, io};

use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, Registry, filter, fmt};

#[derive(Clone, Copy)]
pub struct LoggingFilters {
    pub query: LevelFilter,
    pub checking: LevelFilter,
    pub lsp: LevelFilter,
}

struct SpanTimingLayer;

impl<S> tracing_subscriber::Layer<S> for SpanTimingLayer
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_enter(&self, id: &tracing::span::Id, context: tracing_subscriber::layer::Context<'_, S>) {
        if let Some(span) = context.span(id)
            && span.extensions().get::<Instant>().is_none()
        {
            span.extensions_mut().insert(Instant::now());
        }
    }

    fn on_close(&self, id: tracing::span::Id, context: tracing_subscriber::layer::Context<'_, S>) {
        if let Some(span) = context.span(&id)
            && let Some(start) = span.extensions().get::<Instant>()
            && context.enabled(span.metadata())
        {
            tracing::info!(target: "meta", span = span.name(), span.duration = ?start.elapsed());
        }
    }
}

pub fn start(filters: LoggingFilters) -> io::Result<()> {
    let path = std::env::temp_dir().join("iris-v2.log");
    let file = fs::OpenOptions::new().create(true).append(true).open(path)?;

    let output_filter = filter::Targets::new()
        .with_target("building::engine", filters.query)
        .with_target("checking", filters.checking)
        .with_target("iris_lsp", filters.lsp)
        .with_target("meta", filters.lsp)
        .with_default(LevelFilter::OFF);
    let output = fmt::layer().with_writer(file).with_filter(output_filter);

    let timing_filter =
        filter::Targets::new().with_target("iris_lsp", filters.lsp).with_default(LevelFilter::OFF);
    let timing = SpanTimingLayer.with_filter(timing_filter);

    let subscriber = Registry::default().with(output).with(timing);
    tracing::subscriber::set_global_default(subscriber).map_err(io::Error::other)
}
