use std::time::Duration;

use askama::Template;
use derive_more::Display;
use indexmap::IndexSet;
use num_format::{Locale, ToFormattedString};

use crate::SelectorString;

use super::json::{WebsiteJson, CountingStatsJson};

#[derive(Clone, Debug, Display, Hash, PartialEq, Eq)]
struct Href(String);

#[derive(Template)]
#[template(path = "all_websites/report.html")]
pub struct ReportTemplate<'json> {
    websites: Vec<WebsiteView<'json>>,
}

impl ReportTemplate<'_> {
    const MAX_SLOW_REJECT_ROWS: usize = 100;
}

impl<'json> From<&'json [WebsiteJson]> for ReportTemplate<'json> {
    fn from(value: &'json [WebsiteJson]) -> Self {
        Self {
            websites: value.iter().map(WebsiteView::from).collect(),
        }
    }
}

struct WebsiteView<'json> {
    name: &'json str,
    json_file: Href,
    before_preprocessing: BarView<'json>,
    with_preprocessing: BarView<'json>,
}

impl<'json> WebsiteView<'json> {
    fn total_duration_sort_key(&self) -> u128 {
        self.bars()
            .into_iter()
            .map(|bar| bar.total_duration().as_nanos())
            .max()
            .unwrap_or(0)
    }

    fn slow_reject_duration_sort_key(&self) -> u128 {
        self.bars()
            .into_iter()
            .map(|bar| bar.slow_reject_duration().as_nanos())
            .max()
            .unwrap_or(0)
    }

    fn compact_legend_segments(&self) -> IndexSet<SegmentKind> {
        let mut out = IndexSet::new();
        for bar in self.bars() {
            for segment in &bar.segments {
                out.insert(segment.kind);
            }
        }
        out
    }

    fn bars(&self) -> [&BarView<'json>; 2] {
        [&self.before_preprocessing, &self.with_preprocessing]
    }
}

impl<'json> From<&'json WebsiteJson> for WebsiteView<'json> {
    fn from(value: &'json WebsiteJson) -> Self {
        todo!()
    }
}

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq)]
enum BarLabel {
    #[display("Before Preprocessing")]
    BeforePreprocessing,
    #[display("With Preprocessing")]
    WithPreprocessing,
}

struct BarView<'json> {
    label: BarLabel,
    page_max_duration_ns: u128,
    segments: Vec<SegmentView>,
    stats: CountingStatsView,
    top_slow_reject_selectors: Vec<SelectorRowView<'json>>,
}

impl<'json> BarView<'json> {
    fn total_duration(&self) -> Duration {
        self.segments.iter().map(|segment| segment.duration).sum()
    }

    fn formatted_total_duration(&self) -> String {
        format_duration(self.total_duration())
    }

    fn summary_width_pct(&self) -> f64 {
        if self.page_max_duration_ns == 0 {
            0.0
        } else {
            (self.total_duration().as_nanos() as f64 / self.page_max_duration_ns as f64) * 100.0
        }
    }

    fn slow_reject_duration(&self) -> Duration {
        self.segments
            .iter()
            .find(|segment| segment.kind == SegmentKind::SlowRejecting)
            .map(|segment| segment.duration)
            .unwrap_or(Duration::ZERO)
    }
}

struct SegmentView {
    kind: SegmentKind,
    parent_total_duration: Duration,
    duration: Duration,
}

impl SegmentView {
    fn width_pct(&self) -> f64 {
        if self.parent_total_duration.is_zero() {
            0.0
        } else {
            (self.duration.as_nanos() as f64 / self.parent_total_duration.as_nanos() as f64) * 100.0
        }
    }

    fn formatted_duration(&self) -> String {
        format_duration(self.duration)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SegmentKind {
    Indexing,
    OtherPreprocessing,
    UpdatingBloomFilter,
    CheckingStyleSharing,
    QueryingSelectorMap,
    FastRejecting,
    SlowRejecting,
    SlowAccepting,
    InsertingIntoSharingCache,
    Other,
}

impl SegmentKind {
    fn label(self) -> &'static str {
        match self {
            SegmentKind::Indexing => "Indexing",
            SegmentKind::OtherPreprocessing => "Other Preprocessing",
            SegmentKind::UpdatingBloomFilter => "Updating Bloom Filter",
            SegmentKind::CheckingStyleSharing => "Checking Style Sharing",
            SegmentKind::QueryingSelectorMap => "Querying Selector Map",
            SegmentKind::FastRejecting => "Fast Rejecting",
            SegmentKind::SlowRejecting => "Slow Rejecting",
            SegmentKind::SlowAccepting => "Slow Accepting",
            SegmentKind::InsertingIntoSharingCache => "Inserting Into Sharing Cache",
            SegmentKind::Other => "Other",
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            SegmentKind::Indexing => "seg-index",
            SegmentKind::OtherPreprocessing => "seg-preprocess-other",
            SegmentKind::UpdatingBloomFilter => "seg-bloom",
            SegmentKind::CheckingStyleSharing => "seg-share-check",
            SegmentKind::QueryingSelectorMap => "seg-query",
            SegmentKind::FastRejecting => "seg-fast",
            SegmentKind::SlowRejecting => "seg-slow",
            SegmentKind::SlowAccepting => "seg-slow-accept",
            SegmentKind::InsertingIntoSharingCache => "seg-share-insert",
            SegmentKind::Other => "seg-other",
        }
    }
}

struct SelectorRowView<'json> {
    selector: &'json SelectorString,
    mean_aggregate_slow_reject_time: Duration,
    stddev_aggregate_slow_reject_time: Duration,
}

impl<'json> SelectorRowView<'json> {
    fn selector(&self) -> &'json str {
        &self.selector.0
    }

    fn formatted_mean_duration(&self) -> String {
        format_duration(self.mean_aggregate_slow_reject_time)
    }

    fn formatted_stddev_duration(&self) -> String {
        format_duration(self.stddev_aggregate_slow_reject_time)
    }
}

struct CountingStatsView(CountingStatsJson);

impl CountingStatsView {
    fn formatted_sharing_instances(&self) -> String {
        self.0.sharing_instances.to_formatted_string(&Locale::en)
    }

    fn formatted_selector_map_hits(&self) -> String {
        self.0.selector_map_hits.to_formatted_string(&Locale::en)
    }

    fn formatted_fast_rejects(&self) -> String {
        self.0.fast_rejects.to_formatted_string(&Locale::en)
    }

    fn formatted_slow_rejects(&self) -> String {
        self.0.slow_rejects.to_formatted_string(&Locale::en)
    }

    fn formatted_slow_accepts(&self) -> String {
        self.0.slow_accepts.to_formatted_string(&Locale::en)
    }
}

impl From<CountingStatsJson> for CountingStatsView {
    fn from(value: CountingStatsJson) -> Self {
        Self(value)
    }
}

fn duration_from_ns(ns: u128) -> Duration {
    Duration::from_nanos(u64::try_from(ns).expect("nanoseconds value should fit in u64"))
}

fn format_duration(duration: Duration) -> String {
    if duration >= Duration::from_millis(1) {
        format!("{:.3} ms", duration.as_secs_f64() * 1_000.0)
    } else if duration >= Duration::from_micros(1) {
        format!("{:.3} us", duration.as_secs_f64() * 1_000_000.0)
    } else {
        format!("{} ns", duration.as_nanos())
    }
}
