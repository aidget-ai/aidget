//! Session-scoped runtime diagnostics collected from sandboxed widget documents.
use std::collections::HashSet;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::common::error::{Error, Result};
use crate::widgets::model::{
    DiagnosticSource, DiagnosticSummary, DiagnosticsReport, WidgetDiagnostic,
    WidgetDiagnosticSummary, WidgetMeta,
};

const MAX_ENTRIES: usize = 100;
const MAX_BYTES: usize = 128 * 1024;
const MAX_FIELD_BYTES: usize = 8 * 1024;
const RATE_PER_SECOND: usize = 20;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticInput {
    pub source: DiagnosticSource,
    pub revision: u64,
    pub message: String,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub column: Option<u32>,
}

#[derive(Default)]
struct Bucket {
    entries: VecDeque<WidgetDiagnostic>,
    bytes: usize,
    dropped: u64,
    ready_revision: Option<u64>,
    recent: VecDeque<Instant>,
}

pub struct Diagnostics {
    session_id: String,
    sequence: u64,
    buckets: BTreeMap<String, Bucket>,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            sequence: 0,
            buckets: BTreeMap::new(),
        }
    }
}

impl Diagnostics {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Add a report while enforcing per-widget rate and byte limits.
    pub fn push(&mut self, widget: &str, input: DiagnosticInput) -> WidgetDiagnostic {
        self.sequence += 1;
        let message = truncate(input.message, MAX_FIELD_BYTES);
        let stack = input
            .stack
            .map(|value| truncate(value, MAX_FIELD_BYTES))
            .filter(|value| !value.is_empty());
        let filename = input
            .filename
            .map(|value| sanitize_location(&value))
            .filter(|value| !value.is_empty());
        let event = WidgetDiagnostic {
            sequence: self.sequence,
            received_at: chrono::Local::now().to_rfc3339(),
            widget: widget.to_string(),
            revision: input.revision,
            source: input.source,
            message,
            stack,
            filename,
            line: input.line,
            column: input.column,
        };
        let size = record_size(&event);
        let bucket = self.buckets.entry(widget.to_string()).or_default();
        let now = Instant::now();
        while bucket
            .recent
            .front()
            .is_some_and(|time| now.duration_since(*time) >= Duration::from_secs(1))
        {
            bucket.recent.pop_front();
        }
        if bucket.recent.len() >= RATE_PER_SECOND {
            bucket.dropped += 1;
            return event;
        }
        bucket.recent.push_back(now);
        bucket.entries.push_back(event.clone());
        bucket.bytes += size;
        while bucket.entries.len() > MAX_ENTRIES || bucket.bytes > MAX_BYTES {
            if let Some(removed) = bucket.entries.pop_front() {
                bucket.bytes = bucket.bytes.saturating_sub(record_size(&removed));
                bucket.dropped += 1;
            } else {
                break;
            }
        }
        event
    }

    pub fn mark_ready(&mut self, widget: &str, revision: u64) {
        self.buckets
            .entry(widget.to_string())
            .or_default()
            .ready_revision = Some(revision);
    }

    pub fn add_dropped(&mut self, widget: &str, count: u64) {
        self.buckets.entry(widget.to_string()).or_default().dropped += count;
    }

    pub fn summary(&self, widget: &str, revision: u64) -> DiagnosticSummary {
        self.buckets
            .get(widget)
            .map_or_else(DiagnosticSummary::default, |bucket| DiagnosticSummary {
                count: bucket.entries.len(),
                dropped: bucket.dropped,
                collector_ready: bucket.ready_revision == Some(revision),
            })
    }

    pub fn summaries(&self) -> Vec<WidgetDiagnosticSummary> {
        self.buckets
            .iter()
            .map(|(widget, bucket)| WidgetDiagnosticSummary {
                widget: widget.clone(),
                count: bucket.entries.len(),
                dropped: bucket.dropped,
                collector_ready: bucket.ready_revision.is_some(),
                latest_at: bucket.entries.back().map(|event| event.received_at.clone()),
            })
            .collect()
    }

    pub fn report(
        &self,
        meta: &WidgetMeta,
        requested_revision: Option<u64>,
        after: u64,
        limit: usize,
        running: bool,
        compatibility: crate::widgets::package::HostCompatibility,
    ) -> Result<DiagnosticsReport> {
        if limit == 0 || limit > MAX_ENTRIES {
            return Err(Error::InvalidParams(format!(
                "diagnostic limit must be between 1 and {MAX_ENTRIES}"
            )));
        }
        let revision = requested_revision.unwrap_or(meta.revision);
        let bucket = self.buckets.get(&meta.id);
        let entries: Vec<_> = bucket
            .into_iter()
            .flat_map(|bucket| bucket.entries.iter())
            .filter(|event| event.revision == revision && event.sequence > after)
            .collect();
        let start = if after == 0 {
            entries.len().saturating_sub(limit)
        } else {
            0
        };
        let end = (start + limit).min(entries.len());
        let selected: Vec<_> = entries[start..end]
            .iter()
            .map(|event| (*event).clone())
            .collect();
        let next_after = selected.last().map_or(after, |event| event.sequence);
        Ok(DiagnosticsReport {
            session_id: self.session_id.clone(),
            widget: meta.id.clone(),
            revision: Some(revision),
            after,
            next_after,
            dropped: bucket.map_or(0, |bucket| bucket.dropped),
            collector_ready: bucket.is_some_and(|bucket| bucket.ready_revision == Some(revision)),
            enabled: meta.enabled,
            running,
            compatibility,
            diagnostics: selected,
        })
    }

    pub fn remove(&mut self, widget: &str) {
        self.buckets.remove(widget);
    }

    pub fn retain(&mut self, widgets: &HashSet<String>) {
        self.buckets.retain(|widget, _| widgets.contains(widget));
    }
}

fn sanitize_location(value: &str) -> String {
    let without_fragment = value.split('#').next().unwrap_or(value);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    truncate(without_query.to_string(), MAX_FIELD_BYTES)
}

fn truncate(value: String, max: usize) -> String {
    if value.len() <= max {
        return value;
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn record_size(event: &WidgetDiagnostic) -> usize {
    event.message.len()
        + event.stack.as_ref().map_or(0, String::len)
        + event.filename.as_ref().map_or(0, String::len)
        + event.received_at.len()
        + event.widget.len()
        + 128
}
