use std::time::Duration;

use askama::Template;
use derive_more::Display;

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

    fn max_duration_ns(&self) -> u128 {
        todo!()
    }
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
        todo!()
    }

    fn slow_reject_duration_sort_key(&self) -> u128 {
        todo!()
    }

    fn compact_legend_segments(&self) -> Vec<SegmentKind> {
        todo!()
    }

    fn bars(&self) -> [&BarView<'json>; 2] {
        todo!()
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
    WithPreprocessing
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
        todo!()
    }

    fn formatted_total_duration(&self) -> String {
        todo!()
    }

    fn summary_width_pct(&self) -> f64 {
        todo!()
    }

    fn slow_reject_duration(&self) -> Duration {
        todo!()
    }
}

struct SegmentView {
    kind: SegmentKind,
    parent_total_duration: Duration,
    duration: Duration,
}

impl SegmentView {
    fn width_pct(&self) -> f64 {
        todo!()
    }

    fn formatted_duration(&self) -> String {
        todo!()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        todo!()
    }

    fn css_class(self) -> &'static str {
        todo!()
    }
}

struct SelectorRowView<'json> {
    selector: &'json SelectorString,
    mean_aggregate_slow_reject_time: Duration,
    stddev_aggregate_slow_reject_time: Duration,
}

impl<'json> SelectorRowView<'json> {
    fn selector(&self) -> &'json str {
        todo!()
    }

    fn formatted_mean_duration(&self) -> String {
        todo!()
    }

    fn formatted_stddev_duration(&self) -> String {
        todo!()
    }
}

struct CountingStatsView(CountingStatsJson);

impl CountingStatsView {
    fn formatted_sharing_instances(&self) -> String {
        todo!()
    }

    fn formatted_selector_map_hits(&self) -> String {
        todo!()
    }

    fn formatted_fast_rejects(&self) -> String {
        todo!()
    }

    fn formatted_slow_rejects(&self) -> String {
        todo!()
    }

    fn formatted_slow_accepts(&self) -> String {
        todo!()
    }
}

impl From<CountingStatsJson> for CountingStatsView {
    fn from(value: CountingStatsJson) -> Self {
        Self(value)
    }
}
