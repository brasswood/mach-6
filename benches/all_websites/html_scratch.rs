use std::time::Duration;

use derive_more::Display;
use selectors::matching::CountingStats;

use super::{MatchBenchResult, PreprocessingResult, SelectorSlowRejectSamples, WebsiteResult};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Href(String);

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
    segments: Vec<SegmentView>,
    stats: CountingStats,
    top_slow_reject_selectors: &'result [SelectorSlowRejectSamples],
}

impl<'result> BarView<'result> {
    fn total_duration(&self) -> Duration {
        todo!()
    }

    fn before_preprocessing(result: &'result MatchBenchResult) -> Self {
        todo!()
    }

    fn with_preprocessing(
        result: &'result MatchBenchResult,
        preprocessing: &PreprocessingResult,
    ) -> Self {
        todo!()
    }

    fn slow_reject_duration(&self) -> Duration {
        todo!()
    }
}

struct SegmentView {
    kind: SegmentKind,
    duration: Duration,
}

impl SegmentView {
    fn new(kind: SegmentKind, duration: Duration) -> Self {
        Self { kind, duration }
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
