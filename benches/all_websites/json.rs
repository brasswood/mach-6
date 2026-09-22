use serde::{Deserialize, Serialize};
use time::format_description::well_known::{iso8601, Iso8601};

use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ReportJson {
    pub(crate) metadata: ReportMetadataJson,
    pub(crate) websites: Vec<WebsiteJson>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportSourceJson {
    Nightly,
    Local,
}

const CONFIG: iso8601::EncodedConfig = iso8601::Config::DEFAULT
    .set_time_precision(iso8601::TimePrecision::Second { decimal_digits: None })
    .encode();
const FORMAT: Iso8601<CONFIG> = Iso8601::<CONFIG>;

time::serde::format_description!(rfc3339_nodecimal, OffsetDateTime, FORMAT);

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct VariantManifestEntryJson {
    pub(crate) id: usize,
    pub(crate) label: Option<String>,
    pub(crate) optimizations: Optimizations,
}

// TODO: Generates temporary constant variant manifest, until benchmark machinery gets updated
fn report_variants_manifest() -> Vec<VariantManifestEntryJson> {
    variant_specs()
        .iter()
        .map(|variant_spec| VariantManifestEntryJson {
            id: variant_spec.id,
            label: variant_spec.label.map(str::to_owned),
            optimizations: variant_spec.optimizations,
        })
        .collect()
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ReportMetadataJson {
    #[serde(with = "rfc3339_nodecimal")]
    pub(crate) time_start: time::OffsetDateTime,
    #[serde(with = "rfc3339_nodecimal")]
    pub(crate) time_end: time::OffsetDateTime,
    pub(crate) report_source: ReportSourceJson,
    pub(crate) commit_hash: Option<CommitHash>,
    pub(crate) tagline: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) dirty: Option<bool>,
    pub(crate) branch: Option<String>,
    pub(crate) variants: Vec<VariantManifestEntryJson>,
}

impl ReportMetadataJson {
    pub(crate) fn new(
        report_source: ReportSourceJson,
        git_metadata: Option<ReportGitMetadata>,
        time_start: time::OffsetDateTime,
        time_end: time::OffsetDateTime
    ) -> Self {
        let variants = report_variants_manifest();
        match git_metadata {
            Some(git) => Self {
                time_start,
                time_end,
                report_source,
                commit_hash: Some(git.commit_hash),
                tagline: Some(git.tagline),
                message: Some(git.message),
                dirty: Some(git.dirty),
                branch: git.branch,
                variants,
            },
            None => Self {
                time_start,
                time_end,
                report_source,
                commit_hash: None,
                tagline: None,
                message: None,
                dirty: None,
                branch: None,
                variants,
            }
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SegmentKindJson {
    Indexing,
    IsConversion,
    Distribution,
    UpdatingBloomFilter,
    CheckingStyleSharing,
    QueryingSelectorMap,
    FastRejecting,
    SlowRejecting,
    SlowAccepting,
    InsertingIntoSharingCache,
    Other,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SegmentSummaryJson {
    pub(crate) kind: SegmentKindJson,
    pub(crate) mean_cycles: u64,
    pub(crate) stddev_cycles: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SegmentSamplesJson {
    pub(crate) kind: SegmentKindJson,
    pub(crate) samples_cycles: Vec<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WebsiteVariantJson {
    pub(crate) variant_id: usize,
    pub(crate) summary: overall_summary::BenchmarkRunSummaryJson,
    pub(crate) selector_slow_rejects_summary: selector_summary::SelectorStatsJson,
    pub(crate) samples: samples::TimingsSamplesJson,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WebsiteJson {
    pub(crate) website: String,
    pub(crate) variants: Vec<WebsiteVariantJson>,
}

impl From<&WebsiteResult> for WebsiteJson {
    fn from(value: &WebsiteResult) -> Self {
        assert_eq!(
            value.variants.len(),
            variant_specs().len(),
            "website variant count did not match configured variant manifest",
        );
        Self {
            website: value.website.clone(),
            variants: variant_specs()
                .iter()
                .zip(value.variants.iter())
                .map(|(variant_spec, variant)| WebsiteVariantJson {
                    variant_id: variant_spec.id,
                    summary: overall_summary::BenchmarkRunSummaryJson::new(variant),
                    selector_slow_rejects_summary: selector_summary::SelectorStatsJson::from(
                        variant.selector_slow_reject_times.as_slice(),
                    ),
                    samples: samples::TimingsSamplesJson::from(variant),
                })
                .collect(),
        }
    }
}

mod overall_summary {
    use serde::{Deserialize, Serialize};

    use super::{CountingStats, Samples, SegmentKindJson, SegmentSummaryJson, VariantResult};

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct BenchmarkRunSummaryJson {
        pub(crate) mean_cycles: u64,
        pub(crate) counts: CountingStatsJson,
        pub(crate) times: Vec<SegmentSummaryJson>,
    }

    impl BenchmarkRunSummaryJson {
        pub(crate) fn new(value: &VariantResult) -> Self {
            Self {
                mean_cycles: value.mean_duration().cycles(),
                counts: CountingStatsJson::from(value.counting_stats),
                times: timing_segments(value),
            }
        }
    }

    #[derive(Clone, Copy, Serialize, Deserialize)]
    pub(crate) struct CountingStatsJson {
        pub(crate) sharing_instances: usize,
        pub(crate) selector_map_hits: usize,
        pub(crate) fast_rejects: usize,
        pub(crate) slow_rejects: usize,
        pub(crate) slow_accepts: usize,
    }

    impl From<CountingStats> for CountingStatsJson {
        fn from(value: CountingStats) -> Self {
            Self {
                sharing_instances: value.sharing_instances,
                selector_map_hits: value.selector_map_hits,
                fast_rejects: value.fast_rejects,
                slow_rejects: value.slow_rejects,
                slow_accepts: value.slow_accepts,
            }
        }
    }

    fn segment_summary_json(kind: SegmentKindJson, value: &Samples<tsc_timer::Duration>) -> SegmentSummaryJson {
        SegmentSummaryJson {
            kind,
            mean_cycles: value.mean().cycles(),
            stddev_cycles: Some(value.stddev().cycles()),
        }
    }

    fn derived_segment_summary_json(kind: SegmentKindJson, mean: tsc_timer::Duration) -> SegmentSummaryJson {
        SegmentSummaryJson {
            kind,
            mean_cycles: mean.cycles(),
            stddev_cycles: None,
        }
    }

    fn timing_segments(value: &VariantResult) -> Vec<SegmentSummaryJson> {
        let mut segments = Vec::new();
        if let Some(indexing) = value.timing_segments.indexing.as_ref() {
            segments.push(segment_summary_json(SegmentKindJson::Indexing, indexing));
        }
        if let Some(is_conversion_mean) = value.timing_segments.derived_is_conversion_mean() {
            segments.push(derived_segment_summary_json(SegmentKindJson::IsConversion, is_conversion_mean));
        }
        if let Some(distribution) = value.timing_segments.distribution.as_ref() {
            segments.push(segment_summary_json(SegmentKindJson::Distribution, distribution));
        }
        segments.extend([
            segment_summary_json(SegmentKindJson::UpdatingBloomFilter, &value.timing_segments.updating_bloom_filter),
            segment_summary_json(SegmentKindJson::CheckingStyleSharing, &value.timing_segments.checking_style_sharing),
            segment_summary_json(SegmentKindJson::QueryingSelectorMap, &value.timing_segments.querying_selector_map),
            segment_summary_json(SegmentKindJson::FastRejecting, &value.timing_segments.fast_rejecting),
            segment_summary_json(SegmentKindJson::SlowRejecting, &value.timing_segments.slow_rejecting),
            segment_summary_json(SegmentKindJson::SlowAccepting, &value.timing_segments.slow_accepting),
            segment_summary_json(SegmentKindJson::InsertingIntoSharingCache, &value.timing_segments.inserting_into_sharing_cache),
        ]);
        segments
    }

}

mod selector_summary {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use super::{SelectorSlowRejectSamples, SelectorString};

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct SelectorStatsJson {
        pub(crate) means_cycles: HashMap<SelectorString, u64>,
        pub(crate) stddevs_cycles: HashMap<SelectorString, u64>,
    }

    impl From<&[SelectorSlowRejectSamples]> for SelectorStatsJson {
        fn from(value: &[SelectorSlowRejectSamples]) -> Self {
            Self {
                means_cycles: value
                    .iter()
                    .map(|row| (row.selector.clone(), row.aggregate_durations.mean().cycles()))
                    .collect(),
                stddevs_cycles: value
                    .iter()
                    .map(|row| (row.selector.clone(), row.aggregate_durations.stddev().cycles()))
                    .collect(),
            }
        }
    }
}

mod samples {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};
    #[cfg(feature = "serialize_selector_samples")]
    use tsc_timer::Duration;

    use crate::{SelectorString, VariantResult};

    use super::{Samples, SegmentKindJson, SegmentSamplesJson};

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct TimingsSamplesJson {
        pub(crate) times: Vec<SegmentSamplesJson>,
        pub(crate) selector_slow_rejects_cycles: Option<HashMap<SelectorString, Vec<u64>>>,
    }

    impl From<&VariantResult> for TimingsSamplesJson {
        fn from(value: &VariantResult) -> Self {
            let mut times = Vec::new();
            if let Some(indexing) = value.timing_segments.indexing.as_ref() {
                times.push(segment_samples_json(SegmentKindJson::Indexing, indexing));
            }
            if let Some(distribution) = value.timing_segments.distribution.as_ref() {
                times.push(segment_samples_json(SegmentKindJson::Distribution, distribution));
            }
            times.extend([
                segment_samples_json(SegmentKindJson::UpdatingBloomFilter, &value.timing_segments.updating_bloom_filter),
                segment_samples_json(SegmentKindJson::CheckingStyleSharing, &value.timing_segments.checking_style_sharing),
                segment_samples_json(SegmentKindJson::QueryingSelectorMap, &value.timing_segments.querying_selector_map),
                segment_samples_json(SegmentKindJson::FastRejecting, &value.timing_segments.fast_rejecting),
                segment_samples_json(SegmentKindJson::SlowRejecting, &value.timing_segments.slow_rejecting),
                segment_samples_json(SegmentKindJson::SlowAccepting, &value.timing_segments.slow_accepting),
                segment_samples_json(SegmentKindJson::InsertingIntoSharingCache, &value.timing_segments.inserting_into_sharing_cache),
            ]);
            Self {
                times,
                #[cfg(not(feature = "serialize_selector_samples"))]
                selector_slow_rejects_cycles: None,
                #[cfg(feature = "serialize_selector_samples")]
                selector_slow_rejects_cycles: Some(value
                    .selector_slow_reject_times
                    .iter()
                    .map(|row| {
                        (
                            row.selector.clone(),
                            row.aggregate_durations
                                .iter()
                                .map(Duration::cycles)
                                .collect(),
                        )
                    })
                    .collect()
                ),
            }
        }
    }

    fn segment_samples_json(kind: SegmentKindJson, value: &Samples<tsc_timer::Duration>) -> SegmentSamplesJson {
        SegmentSamplesJson {
            kind,
            samples_cycles: value.iter().map(|duration| duration.cycles()).collect(),
        }
    }
}
