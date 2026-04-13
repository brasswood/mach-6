use std::time::Duration;

use askama::Template;
use derive_more::Display;
use indexmap::IndexSet;
use num_format::{Locale, ToFormattedString};

use crate::{SelectorString, json::{SelectorsSummaryJson, SummaryJson}};

use super::json::{CountingStatsJson, SelectorStatsJson, WebsiteJson};

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
        let page_max_duration_ns = value
            .iter()
            .flat_map(|website| {
                [
                    website.summary.before_preprocessing.mean_duration_ns,
                    website.summary.after_preprocessing.mean_duration_ns
                        + website.summary.preprocessing.mean_overall_duration_ns,
                ]
            })
            .max()
            .unwrap_or(0);

        Self {
            websites: value
                .iter()
                .map(|website| WebsiteView::new(website, page_max_duration_ns))
                .collect(),
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
    fn new(value: &'json WebsiteJson, page_max_duration_ns: u128) -> Self {
        let [before_preprocessing, with_preprocessing] = 
            [BarLabel::BeforePreprocessing, BarLabel::WithPreprocessing].map(|label| 
                BarView::new(
                    label,
                    &value.summary,
                    &value.selector_slow_rejects_summary,
                    page_max_duration_ns,
                )
            );

        Self {
            name: &value.website,
            json_file: Href(format!(
                "json/{}.json",
                crate::make_filename_safe(&value.website)
            )),
            before_preprocessing,
            with_preprocessing,
        }
    }

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
    fn new(
        label: BarLabel,
        summary: &'json SummaryJson,
        selectors_summary: &'json SelectorsSummaryJson,
        page_max_duration_ns: u128,
    ) -> Self {
        let match_summary;
        let preprocessing_summary;
        let selector_stats;
        match label {
            BarLabel::BeforePreprocessing => {
                match_summary = &summary.before_preprocessing;
                preprocessing_summary = None;
                selector_stats = &selectors_summary.before_preprocessing;
            },
            BarLabel::WithPreprocessing => {
                match_summary = &summary.after_preprocessing;
                preprocessing_summary = Some(&summary.preprocessing);
                selector_stats = &selectors_summary.after_preprocessing;
            },
        };

        let times = &match_summary.times.means;
        let total_duration = Duration::from_nanos_u128(
            match_summary.mean_duration_ns
                + if let Some(preprocessing) = preprocessing_summary {
                    preprocessing.mean_overall_duration_ns
                } else {
                    0
                },
        );

        let mut measured_durations = Vec::with_capacity(10);
        if let Some(preprocessing) = preprocessing_summary {
            let indexing_duration = Duration::from_nanos_u128(preprocessing.mean_indexing_duration_ns);
            let preprocessing_other_duration = Duration::from_nanos_u128(
                preprocessing
                    .mean_overall_duration_ns
                    .checked_sub(preprocessing.mean_indexing_duration_ns)
                    .expect("preprocessing overall duration should be >= indexing duration"),
            );
            measured_durations.append(&mut vec![
                (
                    SegmentKind::Indexing,
                    indexing_duration
                ),
                (
                    SegmentKind::OtherPreprocessing,
                    preprocessing_other_duration,
                ),
            ]);
        }

        measured_durations.append(&mut vec![
            (
                SegmentKind::UpdatingBloomFilter,
                Duration::from_nanos_u128(times.updating_bloom_filter_ns),
            ),
            (
                SegmentKind::CheckingStyleSharing,
                Duration::from_nanos_u128(times.checking_style_sharing_ns),
            ),
            (
                SegmentKind::QueryingSelectorMap,
                Duration::from_nanos_u128(times.querying_selector_map_ns),
            ),
            (
                SegmentKind::FastRejecting,
                Duration::from_nanos_u128(times.fast_rejecting_ns),
            ),
            (
                SegmentKind::SlowRejecting,
                Duration::from_nanos_u128(times.slow_rejecting_ns),
            ),
            (
                SegmentKind::SlowAccepting,
                Duration::from_nanos_u128(times.slow_accepting_ns),
            ),
            (
                SegmentKind::InsertingIntoSharingCache,
                Duration::from_nanos_u128(times.inserting_into_sharing_cache_ns),
            ),
        ]);

        let measured_sum = measured_durations
                .iter()
                .map(|(_, duration)| *duration)
                .sum::<Duration>();
        let other_duration = total_duration.checked_sub(measured_sum).unwrap_or_else(|| {
            panic!(
                "Measured timing sum exceeded total duration: measured_sum={}, total_duration={}",
                format_duration(measured_sum),
                format_duration(total_duration),
            )
        });
        measured_durations.push((
            SegmentKind::Other,
            other_duration,
        ));

        let segments: Vec<SegmentView> = measured_durations.into_iter().filter_map(|(kind, duration)| {
            (!duration.is_zero()).then(|| SegmentView {
                kind,
                parent_total_duration: total_duration,
                duration
            })
        })
        .collect();

        BarView {
            label,
            page_max_duration_ns,
            segments,
            stats: CountingStatsView::from(match_summary.counts),
            top_slow_reject_selectors: build_selector_rows(selector_stats),
        }

    }

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

fn build_selector_rows<'json>(stats: &'json SelectorStatsJson) -> Vec<SelectorRowView<'json>> {
    let mut rows: Vec<_> = stats
        .means_ns
        .iter()
        .map(|(selector, mean_ns)| {
            let Some(&stddev_ns) = stats.stddevs_ns.get(selector) else {
                panic!(
                    "mean slow reject time found, but not stddev, for selector {:?}",
                    selector
                )
            };
            SelectorRowView {
                selector,
                mean_aggregate_slow_reject_time: Duration::from_nanos_u128(*mean_ns),
                stddev_aggregate_slow_reject_time: Duration::from_nanos_u128(stddev_ns),
            }
        })
        .collect();

    // Sort by mean slow reject time (descending), then if equal, by name (ascending).
    rows.sort_by(|left, right| {
        right
            .mean_aggregate_slow_reject_time
            .cmp(&left.mean_aggregate_slow_reject_time)
            .then_with(|| left.selector.0.cmp(&right.selector.0))
    });
    rows.truncate(ReportTemplate::MAX_SLOW_REJECT_ROWS);
    rows
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
