use std::time::Duration;

use askama::Template;
use derive_more::Display;
use selectors::matching::CountingStats;

use super::{MatchBenchResult, PreprocessingResult, SelectorSlowRejectSamples, WebsiteResult};

const MAX_SLOW_REJECT_ROWS: usize = 100;

#[derive(Clone, Debug, Display, Hash, PartialEq, Eq)]
struct Href(String);

#[derive(Template)]
#[template(path = "all_websites/report.html")]
struct ReportTemplate<'a> {
    websites: &'a [WebsiteView<'a>],
}

impl ReportTemplate<'_> {
    const MAX_SLOW_REJECT_ROWS: usize = MAX_SLOW_REJECT_ROWS;

    fn max_duration_ns(&self) -> u128 {
        todo!()
    }
}

struct WebsiteView<'result> {
    name: &'result str,
    json_file: Href,
    before_preprocessing: BarView<'result>,
    with_preprocessing: BarView<'result>,
}

impl<'result> WebsiteView<'result> {
    fn total_duration_sort_key(&self) -> u128 {
        todo!()
    }

    fn slow_reject_duration_sort_key(&self) -> u128 {
        todo!()
    }

    fn compact_legend_segments(&self) -> Vec<SegmentKind> {
        todo!()
    }
}

impl<'result> From<&'result WebsiteResult> for WebsiteView<'result> {
    fn from(value: &'result WebsiteResult) -> Self {
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

struct BarView<'result> {
    label: BarLabel,
    page_max_duration_ns: u128,
    segments: Vec<SegmentView>,
    stats: CountingStatsView,
    top_slow_reject_selectors: Vec<SelectorRowView<'result>>,
}

impl<'result> BarView<'result> {
    fn before_preprocessing(result: &'result MatchBenchResult) -> Self {
        todo!()
    }

    fn with_preprocessing(
        result: &'result MatchBenchResult,
        preprocessing: &PreprocessingResult,
    ) -> Self {
        todo!()
    }

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

struct SelectorRowView<'result> {
    source: &'result SelectorSlowRejectSamples,
}

impl<'result> SelectorRowView<'result> {
    fn selector(&self) -> &'result str {
        todo!()
    }

    fn formatted_mean_duration(&self) -> String {
        todo!()
    }

    fn formatted_stddev_duration(&self) -> String {
        todo!()
    }
}

impl<'result> From<&'result SelectorSlowRejectSamples> for SelectorRowView<'result> {
    fn from(value: &'result SelectorSlowRejectSamples) -> Self {
        todo!()
    }
}

struct CountingStatsView {
    source: CountingStats,
}

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

impl From<CountingStats> for CountingStatsView {
    fn from(value: CountingStats) -> Self {
        todo!()
    }
}
