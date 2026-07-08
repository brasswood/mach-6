use serde::{Deserialize, Serialize};
use time::format_description::well_known::{iso8601, Iso8601};

use super::*;

const BASELINE_VARIANT_ID: usize = 0;
const OPTIMIZED_VARIANT_ID: usize = 1;

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
    vec![
        VariantManifestEntryJson {
            id: BASELINE_VARIANT_ID,
            label: None,
            optimizations: Optimizations::from_none(),
        },
        VariantManifestEntryJson {
            id: OPTIMIZED_VARIANT_ID,
            label: None,
            optimizations: Optimizations {
                is_conversion: true,
                distribution: true,
            },
        },
    ]
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
        Self {
            website: value.website.clone(),
            variants: vec![
                WebsiteVariantJson {
                    variant_id: BASELINE_VARIANT_ID,
                    summary: overall_summary::BenchmarkRunSummaryJson::new(
                        &value.before_preprocessing,
                        None,
                    ),
                    selector_slow_rejects_summary: selector_summary::SelectorStatsJson::from(
                        value.before_preprocessing.selector_slow_reject_times.as_slice(),
                    ),
                    samples: samples::TimingsSamplesJson::from(&value.before_preprocessing),
                },
                WebsiteVariantJson {
                    variant_id: OPTIMIZED_VARIANT_ID,
                    summary: overall_summary::BenchmarkRunSummaryJson::new(
                        &value.after_preprocessing,
                        Some(&value.preprocessing),
                    ),
                    selector_slow_rejects_summary: selector_summary::SelectorStatsJson::from(
                        value.after_preprocessing.selector_slow_reject_times.as_slice(),
                    ),
                    samples: samples::TimingsSamplesJson::from(&value.after_preprocessing),
                },
            ],
        }
    }
}

mod overall_summary {
    use serde::{Deserialize, Serialize};

    use super::{CountingStats, MatchBenchResult, PreprocessingResult, Samples, SegmentKindJson, SegmentSummaryJson, TimingStats};

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct BenchmarkRunSummaryJson {
        pub(crate) mean_cycles: u64,
        pub(crate) counts: CountingStatsJson,
        pub(crate) times: Vec<SegmentSummaryJson>,
    }

    impl BenchmarkRunSummaryJson {
        pub(crate) fn new(
            value: &MatchBenchResult,
            preprocessing: Option<&PreprocessingResult>,
        ) -> Self {
            let mut times = Vec::new();
            if let Some(preprocessing) = preprocessing {
                times.extend(preprocessing_segments(preprocessing));
            }
            times.extend(matching_timing_segments(&value.timing_stats));
            Self {
                mean_cycles: value.mean_duration().cycles(),
                counts: CountingStatsJson::from(value.counting_stats),
                times,
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

    fn preprocessing_segments(value: &PreprocessingResult) -> [SegmentSummaryJson; 3] {
        [
            SegmentSummaryJson {
                kind: SegmentKindJson::Indexing,
                mean_cycles: value.mean_indexing().cycles(),
                stddev_cycles: None,
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::IsConversion,
                mean_cycles: value.mean_non_indexing().cycles(),
                stddev_cycles: None,
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::Distribution,
                mean_cycles: value.mean_distributing().cycles(),
                stddev_cycles: None,
            },
        ]
    }

    fn matching_timing_segments(value: &Samples<TimingStats>) -> [SegmentSummaryJson; 7] {
        let means = value.mean();
        let stddevs = value.stddev();
        [
            SegmentSummaryJson {
                kind: SegmentKindJson::UpdatingBloomFilter,
                mean_cycles: means.updating_bloom_filter.cycles(),
                stddev_cycles: Some(stddevs.updating_bloom_filter.cycles()),
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::CheckingStyleSharing,
                mean_cycles: means.checking_style_sharing.cycles(),
                stddev_cycles: Some(stddevs.checking_style_sharing.cycles()),
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::QueryingSelectorMap,
                mean_cycles: means.querying_selector_map.cycles(),
                stddev_cycles: Some(stddevs.querying_selector_map.cycles()),
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::FastRejecting,
                mean_cycles: means.fast_rejecting.cycles(),
                stddev_cycles: Some(stddevs.fast_rejecting.cycles()),
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::SlowRejecting,
                mean_cycles: means.slow_rejecting.cycles(),
                stddev_cycles: Some(stddevs.slow_rejecting.cycles()),
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::SlowAccepting,
                mean_cycles: means.slow_accepting.cycles(),
                stddev_cycles: Some(stddevs.slow_accepting.cycles()),
            },
            SegmentSummaryJson {
                kind: SegmentKindJson::InsertingIntoSharingCache,
                mean_cycles: means.inserting_into_sharing_cache.cycles(),
                stddev_cycles: Some(stddevs.inserting_into_sharing_cache.cycles()),
            },
        ]
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

    use selectors::matching::TimingStats;
    use serde::{Deserialize, Serialize};
    use tsc_timer::Duration;

    use crate::{MatchBenchResult, SelectorString};

    use super::{SegmentKindJson, SegmentSamplesJson};

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct TimingsSamplesJson {
        pub(crate) times: Vec<SegmentSamplesJson>,
        pub(crate) selector_slow_rejects_cycles: Option<HashMap<SelectorString, Vec<u64>>>,
    }

    impl From<&MatchBenchResult> for TimingsSamplesJson {
        fn from(value: &MatchBenchResult) -> Self {
            // Codex taught me this `project` trick!
            let get_cycles_samples = |project: fn(&TimingStats) -> Duration| -> Vec<u64> {
                value
                    .timing_stats
                    .iter()
                    .map(|sample| project(sample).cycles())
                    .collect()
            };
            Self {
                times: vec![
                    SegmentSamplesJson {
                        kind: SegmentKindJson::UpdatingBloomFilter,
                        samples_cycles: get_cycles_samples(|timing_stats| timing_stats.updating_bloom_filter),
                    },
                    SegmentSamplesJson {
                        kind: SegmentKindJson::CheckingStyleSharing,
                        samples_cycles: get_cycles_samples(|timing_stats| timing_stats.checking_style_sharing),
                    },
                    SegmentSamplesJson {
                        kind: SegmentKindJson::QueryingSelectorMap,
                        samples_cycles: get_cycles_samples(|timing_stats| timing_stats.querying_selector_map),
                    },
                    SegmentSamplesJson {
                        kind: SegmentKindJson::FastRejecting,
                        samples_cycles: get_cycles_samples(|timing_stats| timing_stats.fast_rejecting),
                    },
                    SegmentSamplesJson {
                        kind: SegmentKindJson::SlowRejecting,
                        samples_cycles: get_cycles_samples(|timing_stats| timing_stats.slow_rejecting),
                    },
                    SegmentSamplesJson {
                        kind: SegmentKindJson::SlowAccepting,
                        samples_cycles: get_cycles_samples(|timing_stats| timing_stats.slow_accepting),
                    },
                    SegmentSamplesJson {
                        kind: SegmentKindJson::InsertingIntoSharingCache,
                        samples_cycles: get_cycles_samples(|timing_stats| timing_stats.inserting_into_sharing_cache),
                    },
                ],
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
}
