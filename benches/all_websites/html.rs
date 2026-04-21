use std::time::Duration;

use askama::Template;
use derive_more::Display;
use indexmap::IndexSet;
use num_format::{Locale, ToFormattedString};

use crate::{SelectorString, json::{ReportMetadataJson, SelectorsSummaryJson, SummaryJson}};

use super::json::{CountingStatsJson, SelectorStatsJson, WebsiteJson};

#[derive(Clone, Debug, Display, Hash, PartialEq, Eq)]
struct Href(String);

#[derive(Template)]
#[template(path = "all_websites/report.html")]
pub struct ReportTemplate<'json> {
    metadata: &'json ReportMetadataJson,
    websites: Vec<WebsiteView<'json>>,
}

impl ReportTemplate<'_> {
    const MAX_SLOW_REJECT_ROWS: usize = 100;
}

impl<'json> ReportTemplate<'json> {
    pub(crate) fn new(metadata: &'json ReportMetadataJson, value: &'json [WebsiteJson]) -> Self {
        Self {
            metadata,
            websites: value
                .iter()
                .map(|website| WebsiteView::new(website))
                .collect(),
        }
    }
}

impl ReportTemplate<'_> {
    fn page_max_bar_length(&self) -> Duration {
        self.websites.iter()
            .flat_map(|wv|
                wv.bars().into_iter().map(BarView::total_length)
            )
            .max()
            .expect("There were no websites.")
    }
}

struct WebsiteView<'json> {
    name: &'json str,
    json_file: Href,
    before_preprocessing: BarView<'json>,
    with_preprocessing: BarView<'json>,
}

impl<'json> WebsiteView<'json> {
    fn new(value: &'json WebsiteJson) -> Self {
        let [before_preprocessing, with_preprocessing] = 
            [BarLabel::BeforePreprocessing, BarLabel::WithPreprocessing].map(|label| 
                BarView::new(
                    &value.website,
                    label,
                    &value.summary,
                    &value.selector_slow_rejects_summary,
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
        const ORDER: [SegmentKind; 10] = [
            SegmentKind::Indexing,
            SegmentKind::OtherPreprocessing,
            SegmentKind::UpdatingBloomFilter,
            SegmentKind::CheckingStyleSharing,
            SegmentKind::QueryingSelectorMap,
            SegmentKind::FastRejecting,
            SegmentKind::SlowRejecting,
            SegmentKind::SlowAccepting,
            SegmentKind::InsertingIntoSharingCache,
            SegmentKind::Other,
        ];

        ORDER
            .into_iter()
            .filter(|kind| {
                self.bars()
                    .into_iter()
                    .any(|bar| bar.segments.iter().any(|segment| segment.kind == *kind))
            })
            .collect()
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
    segments: Vec<SegmentView>,
    stats: CountingStatsView,
    top_slow_reject_selectors: Vec<SelectorRowView<'json>>,
    website_for_diagnostics: &'json str,
}

impl<'json> BarView<'json> {
    fn new(
        website_for_diagnostics: &'json str, // for error diagnostics
        label: BarLabel,
        summary: &'json SummaryJson,
        selectors_summary: &'json SelectorsSummaryJson,
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
            }
        };

        let times_means = &match_summary.times.means;
        let times_stddevs = &match_summary.times.stddevs;

        // First the match durations
        let mut measured_match_durations = vec![
            (
                SegmentKind::UpdatingBloomFilter,
                times_means.updating_bloom_filter_ns as i128,
                Some(times_stddevs.updating_bloom_filter_ns),
            ),
            (
                SegmentKind::CheckingStyleSharing,
                times_means.checking_style_sharing_ns as i128,
                Some(times_stddevs.checking_style_sharing_ns),
            ),
            (
                SegmentKind::QueryingSelectorMap,
                times_means.querying_selector_map_ns as i128,
                Some(times_stddevs.querying_selector_map_ns),
            ),
            (
                SegmentKind::FastRejecting,
                times_means.fast_rejecting_ns as i128,
                Some(times_stddevs.fast_rejecting_ns),
            ),
            (
                SegmentKind::SlowRejecting,
                times_means.slow_rejecting_ns as i128,
                Some(times_stddevs.slow_rejecting_ns),
            ),
            (
                SegmentKind::SlowAccepting,
                times_means.slow_accepting_ns as i128,
                Some(times_stddevs.slow_accepting_ns),
            ),
            (
                SegmentKind::InsertingIntoSharingCache,
                times_means.inserting_into_sharing_cache_ns as i128,
                Some(times_stddevs.inserting_into_sharing_cache_ns),
            ),
        ];
        // Compute "Other" from match durations only
        let measured_match_sum = measured_match_durations
                .iter()
                .map(|(_, nanos, _)| *nanos)
                .sum::<i128>();
        let other_duration_ns = match_summary.mean_duration_ns as i128 - measured_match_sum;
        measured_match_durations.push((
            SegmentKind::Other,
            other_duration_ns,
            None,
        ));

        // Now create a vector with preprocessing at the beginning and
        // matching afterward
        let mut measured_durations = Vec::with_capacity(10);
        if let Some(preprocessing) = preprocessing_summary {
            let mean_preprocessing_other_duration_ns = preprocessing.mean_overall_duration_ns as i128 - preprocessing.mean_indexing_duration_ns as i128;
            measured_durations.append(&mut vec![
                (
                    SegmentKind::Indexing,
                    preprocessing.mean_indexing_duration_ns as i128,
                    None,
                ),
                (
                    SegmentKind::OtherPreprocessing,
                    mean_preprocessing_other_duration_ns,
                    None,
                ),
            ]);
        }
        measured_durations.append(&mut measured_match_durations);

        let total_bar_length = measured_durations
            .iter()
            .map(|&(_, nanos, _)| nanos.max(0))
            .sum::<i128>();

        let segments: Vec<SegmentView> = measured_durations
            .into_iter()
            .map(|(kind, mean_ns, stddev)| {
                SegmentView {
                    kind,
                    parent_total_bar_length: Duration::from_nanos_u128(total_bar_length as u128),
                    mean_ns,
                    stddev: stddev.map(Duration::from_nanos_u128),
                }
            })
            .collect();

        BarView {
            label,
            segments,
            stats: CountingStatsView::from(match_summary.counts),
            top_slow_reject_selectors: build_selector_rows(selector_stats),
            website_for_diagnostics,
        }
    }

    fn total_duration(&self) -> Duration {
        let sum = self.segments
            .iter()
            .map(|seg| seg.mean_ns)
            .sum::<i128>();
        let sum = u128::try_from(sum)
            .unwrap_or_else(|e| panic!(
                "Failed to cast sum of segments from i128 to u128 for {}. The i128 value was {}. Error message: {}",
                self.website_for_diagnostics,
                sum,
                e,
            ));
        Duration::from_nanos_u128(sum)
    }

    fn formatted_total_duration(&self) -> String {
        format_duration(self.total_duration())
    }

    fn total_length(&self) -> Duration {
        let sum = self.segments
            .iter()
            .map(|seg| seg.mean_ns.max(0))
            .sum::<i128>();
        Duration::from_nanos_u128(sum as u128)
    }

    fn formatted_total_length(&self) -> String {
        format_duration(self.total_length())
    }

    fn has_display_length_mismatch(&self) -> bool {
        self.total_length() != self.total_duration()
    }

    fn summary_width_pct(&self, page_max_bar_length: Duration) -> f64 {
        if page_max_bar_length.is_zero() {
            0.0
        } else {
            (self.total_length().as_nanos() as f64 / page_max_bar_length.as_nanos() as f64) * 100.0
        }
    }

    fn slow_reject_duration(&self) -> Duration {
        let Some(segment) = self.segments
            .iter()
            .find(|segment| segment.kind == SegmentKind::SlowRejecting)
        else {
            panic!(
                "SegmentKind::SlowRejecting not found for {}",
                self.website_for_diagnostics
            )
        };
        let ns = segment.mean_ns.try_into().unwrap_or_else(|e|
            panic!(
                "Failed to cast {} slow-reject nanos from i128 to u128. The i128 value was {}. Error message: {}",
                self.website_for_diagnostics,
                segment.mean_ns,
                e
            )
        );
        Duration::from_nanos_u128(ns)
    }
}

struct SegmentView {
    kind: SegmentKind,
    parent_total_bar_length: Duration,
    mean_ns: i128,
    stddev: Option<Duration>,
}

impl SegmentView {
    fn width_pct(&self) -> f64 {
        if self.parent_total_bar_length.is_zero() {
            0.0
        } else {
            (self.mean_ns.max(0) as f64
                / self.parent_total_bar_length.as_nanos() as f64)
                * 100.0
        }
    }

    fn formatted_duration_with_stddev(&self) -> String {
        let formatted_duration = format_signed_duration_ns(self.mean_ns);
        if let Some(stddev) = self.stddev {
            format!("{formatted_duration} \u{00B1} {}", format_duration(stddev))
        } else {
            formatted_duration
        }
    }

    fn is_negative(&self) -> bool {
        self.mean_ns < 0
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

    fn formatted_mean_with_stddev(&self) -> String {
        format!(
            "{} \u{00B1} {}",
            self.formatted_mean_duration(),
            self.formatted_stddev_duration()
        )
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

fn format_signed_duration_ns(duration_ns: i128) -> String {
    if duration_ns < 0 {
        format!(
            "-{}",
            format_duration(Duration::from_nanos_u128(duration_ns.unsigned_abs()))
        )
    } else {
        format_duration(Duration::from_nanos_u128(duration_ns as u128))
    }
}
