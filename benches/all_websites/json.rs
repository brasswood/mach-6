use serde::{Deserialize, Serialize};
use time::format_description::well_known::{iso8601, Iso8601};

use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ReportJson {
    pub(crate) metadata: ReportMetadataJson,
    pub(crate) websites: Vec<WebsiteJson>,
}

const CONFIG: iso8601::EncodedConfig = iso8601::Config::DEFAULT
    .set_time_precision(iso8601::TimePrecision::Second { decimal_digits: None })
    .encode();
const FORMAT: Iso8601<CONFIG> = Iso8601::<CONFIG>;

time::serde::format_description!(rfc3339_nodecimal, OffsetDateTime, FORMAT);

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ReportMetadataJson {
    #[serde(with = "rfc3339_nodecimal")]
    pub(crate) time_start: time::OffsetDateTime,
    #[serde(with = "rfc3339_nodecimal")]
    pub(crate) time_end: time::OffsetDateTime,
    pub(crate) commit_hash: Option<CommitHash>,
    pub(crate) tagline: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) dirty: Option<bool>,
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
                dirty: Some(git.dirty),
                branch: git.branch,
            },
            None => Self {
                time_start,
                time_end,
                commit_hash: None,
                tagline: None,
                message: None,
                dirty: None,
                branch: None,
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
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

    #[derive(Clone, Serialize, Deserialize)]
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

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct PreprocessingSummaryJson {
        pub(crate) mean_indexing_cycles: u64,
        pub(crate) mean_overall_cycles: u64,
    }

    impl From<&PreprocessingResult> for PreprocessingSummaryJson {
        fn from(value: &PreprocessingResult) -> Self {
            Self {
                mean_indexing_cycles: value.mean_indexing().cycles(),
                mean_overall_cycles: value.mean_overall().cycles(),
            }
        }
    }

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct BenchmarkRunSummaryJson {
        pub(crate) mean_cycles: u64,
        pub(crate) counts: CountingStatsJson,
        pub(crate) times: TimingStatsJson,
    }

    impl From<&MatchBenchResult> for BenchmarkRunSummaryJson {
        fn from(value: &MatchBenchResult) -> Self {
            Self {
                mean_cycles: value.mean_duration().cycles(),
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

    #[derive(Clone, Serialize, Deserialize)]
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

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct TimingsJsonBody {
        pub(crate) updating_bloom_filter_cycles: u64,
        pub(crate) slow_rejecting_cycles: u64,
        pub(crate) slow_accepting_cycles: u64,
        pub(crate) fast_rejecting_cycles: u64,
        pub(crate) checking_style_sharing_cycles: u64,
        pub(crate) inserting_into_sharing_cache_cycles: u64,
        pub(crate) querying_selector_map_cycles: u64,
    }

    impl From<TimingStats> for TimingsJsonBody {
        fn from(value: TimingStats) -> Self {
            Self {
                updating_bloom_filter_cycles: value.updating_bloom_filter.cycles(),
                slow_rejecting_cycles: value.slow_rejecting.cycles(),
                slow_accepting_cycles: value.slow_accepting.cycles(),
                fast_rejecting_cycles: value.fast_rejecting.cycles(),
                checking_style_sharing_cycles: value.checking_style_sharing.cycles(),
                inserting_into_sharing_cache_cycles: value.inserting_into_sharing_cache.cycles(),
                querying_selector_map_cycles: value.querying_selector_map.cycles(),
            }
        }
    }
}

mod selector_summary {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use crate::WebsiteResult;

    use super::{SelectorSlowRejectSamples, SelectorString};

    #[derive(Clone, Serialize, Deserialize)]
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
    use tsc_timer::Duration;
    use std::collections::HashMap;

    use selectors::matching::TimingStats;
    use serde::{Deserialize, Serialize};

    use crate::{MatchBenchResult, SelectorString, WebsiteResult};

    #[derive(Clone, Serialize, Deserialize)]
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

    #[derive(Clone, Serialize, Deserialize)]
    pub(crate) struct TimingsSamplesJson {
        pub(crate) updating_bloom_filter_cycles: Vec<u64>,
        pub(crate) slow_rejecting_cycles: Vec<u64>,
        pub(crate) slow_accepting_cycles: Vec<u64>,
        pub(crate) fast_rejecting_cycles: Vec<u64>,
        pub(crate) checking_style_sharing_cycles: Vec<u64>,
        pub(crate) inserting_into_sharing_cache_cycles: Vec<u64>,
        pub(crate) querying_selector_map_cycles: Vec<u64>,
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
                updating_bloom_filter_cycles: get_cycles_samples(|timing_stats| timing_stats.updating_bloom_filter),
                slow_rejecting_cycles: get_cycles_samples(|timing_stats| timing_stats.slow_rejecting),
                slow_accepting_cycles: get_cycles_samples(|timing_stats| timing_stats.slow_accepting),
                fast_rejecting_cycles: get_cycles_samples(|timing_stats| timing_stats.fast_rejecting),
                checking_style_sharing_cycles: get_cycles_samples(|timing_stats| timing_stats.checking_style_sharing),
                inserting_into_sharing_cache_cycles: get_cycles_samples(|timing_stats| timing_stats.inserting_into_sharing_cache),
                querying_selector_map_cycles: get_cycles_samples(|timing_stats| timing_stats.querying_selector_map),
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
