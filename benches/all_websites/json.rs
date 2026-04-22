use serde::{Deserialize, Serialize};

use super::*;

pub(crate) use overall_summary::*;
pub(crate) use selector_summary::*;

#[derive(Serialize, Deserialize)]
pub(crate) struct ReportMetadataJson {
    pub(crate) time_start: time::OffsetDateTime,
    pub(crate) time_end: time::OffsetDateTime,
    pub(crate) commit_hash: Option<CommitHash>,
    pub(crate) tagline: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) branch: Option<String>,
}

impl ReportMetadataJson {
    pub(crate) fn new(
        git_metadata: Option<ReportGitMetadata>,
        time_start: time::OffsetDateTime,
        time_end: time::OffsetDateTime
    ) -> Self {
        match git_metadata {
            Some(git) => Self {
                time_start,
                time_end,
                commit_hash: Some(git.commit_hash),
                tagline: Some(git.tagline),
                message: Some(git.message),
                branch: git.branch,
            },
            None => Self {
                time_start,
                time_end,
                commit_hash: None,
                tagline: None,
                message: None,
                branch: None,
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct WebsiteJson {
    pub(crate) website: String,
    pub(crate) summary: overall_summary::SummaryJson,
    pub(crate) selector_slow_rejects_summary: selector_summary::SelectorsSummaryJson,
    pub(crate) samples: samples::SamplesJson,
}

impl From<&WebsiteResult> for WebsiteJson {
    fn from(value: &WebsiteResult) -> Self {
        Self {
            website: value.website.clone(),
            summary: overall_summary::SummaryJson::from(value),
            selector_slow_rejects_summary: selector_summary::SelectorsSummaryJson::from(value),
            samples: samples::SamplesJson::from(value),
        }
    }
}

mod overall_summary {
    use serde::{Deserialize, Serialize};

    use crate::WebsiteResult;

    use super::{CountingStats, MatchBenchResult, PreprocessingResult, Samples, TimingStats};

    #[derive(Serialize, Deserialize)]
    pub(crate) struct SummaryJson {
        pub(crate) before_preprocessing: BenchmarkRunSummaryJson,
        pub(crate) preprocessing: PreprocessingSummaryJson,
        pub(crate) after_preprocessing: BenchmarkRunSummaryJson,
    }

    impl From<&WebsiteResult> for SummaryJson {
        fn from(value: &WebsiteResult) -> Self {
            Self {
                before_preprocessing: BenchmarkRunSummaryJson::from(&value.before_preprocessing),
                preprocessing: PreprocessingSummaryJson::from(&value.preprocessing),
                after_preprocessing: BenchmarkRunSummaryJson::from(&value.after_preprocessing),
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct PreprocessingSummaryJson {
        pub(crate) mean_indexing_duration_ns: u128,
        pub(crate) mean_overall_duration_ns: u128,
    }

    impl From<&PreprocessingResult> for PreprocessingSummaryJson {
        fn from(value: &PreprocessingResult) -> Self {
            Self {
                mean_indexing_duration_ns: value.mean_indexing().as_nanos(),
                mean_overall_duration_ns: value.mean_overall().as_nanos(),
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct BenchmarkRunSummaryJson {
        pub(crate) mean_duration_ns: u128,
        pub(crate) counts: CountingStatsJson,
        pub(crate) times: TimingStatsJson,
    }

    impl From<&MatchBenchResult> for BenchmarkRunSummaryJson {
        fn from(value: &MatchBenchResult) -> Self {
            Self {
                mean_duration_ns: value.mean_duration().as_nanos(),
                counts: CountingStatsJson::from(value.counting_stats),
                times: TimingStatsJson::from(&value.timing_stats),
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

    #[derive(Serialize, Deserialize)]
    pub(crate) struct TimingStatsJson {
        pub(crate) means: TimingsJsonBody,
        pub(crate) stddevs: TimingsJsonBody,
    }

    impl From<&Samples<TimingStats>> for TimingStatsJson {
        fn from(value: &Samples<TimingStats>) -> Self {
            Self {
                means: TimingsJsonBody::from(value.mean()),
                stddevs: TimingsJsonBody::from(value.stddev()),
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct TimingsJsonBody {
        pub(crate) updating_bloom_filter_ns: u128,
        pub(crate) slow_rejecting_ns: u128,
        pub(crate) slow_accepting_ns: u128,
        pub(crate) fast_rejecting_ns: u128,
        pub(crate) checking_style_sharing_ns: u128,
        pub(crate) inserting_into_sharing_cache_ns: u128,
        pub(crate) querying_selector_map_ns: u128,
    }

    impl From<TimingStats> for TimingsJsonBody {
        fn from(value: TimingStats) -> Self {
            Self {
                updating_bloom_filter_ns: value.updating_bloom_filter.as_nanos(),
                slow_rejecting_ns: value.slow_rejecting.as_nanos(),
                slow_accepting_ns: value.slow_accepting.as_nanos(),
                fast_rejecting_ns: value.fast_rejecting.as_nanos(),
                checking_style_sharing_ns: value.checking_style_sharing.as_nanos(),
                inserting_into_sharing_cache_ns: value.inserting_into_sharing_cache.as_nanos(),
                querying_selector_map_ns: value.querying_selector_map.as_nanos(),
            }
        }
    }
}

mod selector_summary {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use crate::WebsiteResult;

    use super::{SelectorSlowRejectSamples, SelectorString};

    #[derive(Serialize, Deserialize)]
    pub(crate) struct SelectorsSummaryJson {
        pub(crate) before_preprocessing: SelectorStatsJson,
        pub(crate) after_preprocessing: SelectorStatsJson,
    }

    impl From<&WebsiteResult> for SelectorsSummaryJson {
        fn from(value: &WebsiteResult) -> Self {
            Self {
                before_preprocessing: SelectorStatsJson::from(value.before_preprocessing.selector_slow_reject_times.as_slice()),
                after_preprocessing: SelectorStatsJson::from(value.after_preprocessing.selector_slow_reject_times.as_slice())
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct SelectorStatsJson {
        pub(crate) means_ns: HashMap<SelectorString, u128>,
        pub(crate) stddevs_ns: HashMap<SelectorString, u128>,
    }

    impl From<&[SelectorSlowRejectSamples]> for SelectorStatsJson {
        fn from(value: &[SelectorSlowRejectSamples]) -> Self {
            Self {
                means_ns: value
                    .iter()
                    .map(|row| (row.selector.clone(), row.aggregate_durations.mean().as_nanos()))
                    .collect(),
                stddevs_ns: value
                    .iter()
                    .map(|row| (row.selector.clone(), row.aggregate_durations.stddev().as_nanos()))
                    .collect(),
            }
        }
    }
}

mod samples {
    use std::time::Duration;
    use std::collections::HashMap;

    use selectors::matching::TimingStats;
    use serde::{Deserialize, Serialize};

    use crate::{MatchBenchResult, SelectorString, WebsiteResult};

    #[derive(Serialize, Deserialize)]
    pub(crate) struct SamplesJson {
        pub(crate) before_preprocessing: TimingsSamplesJson,
        pub(crate) after_preprocessing: TimingsSamplesJson,
    }

    impl From<&WebsiteResult> for SamplesJson {
        fn from(value: &WebsiteResult) -> Self {
            Self {
                before_preprocessing: TimingsSamplesJson::from(&value.before_preprocessing),
                after_preprocessing: TimingsSamplesJson::from(&value.after_preprocessing),
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct TimingsSamplesJson {
        pub(crate) updating_bloom_filter_ns: Vec<u128>,
        pub(crate) slow_rejecting_ns: Vec<u128>,
        pub(crate) slow_accepting_ns: Vec<u128>,
        pub(crate) fast_rejecting_ns: Vec<u128>,
        pub(crate) checking_style_sharing_ns: Vec<u128>,
        pub(crate) inserting_into_sharing_cache_ns: Vec<u128>,
        pub(crate) querying_selector_map_ns: Vec<u128>,
        pub(crate) selector_slow_rejects_ns: Option<HashMap<SelectorString, Vec<u128>>>,
    }

    impl From<&MatchBenchResult> for TimingsSamplesJson {
        fn from(value: &MatchBenchResult) -> Self {
            // Codex taught me this `project` trick!
            let get_ns_samples = |project: fn(&TimingStats) -> Duration| -> Vec<u128> {
                value
                    .timing_stats
                    .iter()
                    .map(|sample| project(sample).as_nanos())
                    .collect()
            };
            Self {
                updating_bloom_filter_ns: get_ns_samples(|timing_stats| timing_stats.updating_bloom_filter),
                slow_rejecting_ns: get_ns_samples(|timing_stats| timing_stats.slow_rejecting),
                slow_accepting_ns: get_ns_samples(|timing_stats| timing_stats.slow_accepting),
                fast_rejecting_ns: get_ns_samples(|timing_stats| timing_stats.fast_rejecting),
                checking_style_sharing_ns: get_ns_samples(|timing_stats| timing_stats.checking_style_sharing),
                inserting_into_sharing_cache_ns: get_ns_samples(|timing_stats| timing_stats.inserting_into_sharing_cache),
                querying_selector_map_ns: get_ns_samples(|timing_stats| timing_stats.querying_selector_map),
                #[cfg(not(feature = "serialize_selector_samples"))]
                selector_slow_rejects_ns: None,
                #[cfg(feature = "serialize_selector_samples")]
                selector_slow_rejects_ns: Some(value
                    .selector_slow_reject_times
                    .iter()
                    .map(|row| {
                        (
                            row.selector.clone(),
                            row.aggregate_durations
                                .iter()
                                .map(Duration::as_nanos)
                                .collect(),
                        )
                    })
                    .collect()
                ),
            }
        }
    }
}
