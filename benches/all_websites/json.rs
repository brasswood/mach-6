use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(super) struct WebsiteJson<'a> {
    website: &'a str,
    summary: SummaryJson,
    samples: SamplesJson,
}

#[derive(Serialize, Deserialize)]
struct SummaryJson {
    before_preprocessing: BenchmarkRunSummaryJson,
    preprocessing: PreprocessingSummaryJson,
    after_preprocessing: BenchmarkRunSummaryJson,
}

#[derive(Serialize, Deserialize)]
struct SamplesJson {
    before_preprocessing: TimingsJsonBody<Vec<u128>>,
    after_preprocessing: TimingsJsonBody<Vec<u128>>,
}

#[derive(Serialize, Deserialize)]
struct BenchmarkRunSummaryJson {
    mean_duration_ns: u128,
    counts: CountingStatsJson,
    times: TimingStatsJson,
}

#[derive(Serialize, Deserialize)]
struct PreprocessingSummaryJson {
    mean_indexing_duration_ns: u128,
    mean_preprocessing_duration_ns: u128,
}

#[derive(Serialize, Deserialize)]
struct CountingStatsJson {
    sharing_instances: usize,
    selector_map_hits: usize,
    fast_rejects: usize,
    slow_rejects: usize,
    slow_accepts: usize,
}

#[derive(Serialize, Deserialize)]
struct TimingStatsJson {
    means: TimingsJsonBody<u128>,
    stddevs: TimingsJsonBody<u128>,
}

#[derive(Serialize, Deserialize)]
struct TimingsJsonBody<T> {
    updating_bloom_filter_ns: T,
    slow_rejecting_ns: T,
    slow_accepting_ns: T,
    fast_rejecting_ns: T,
    checking_style_sharing_ns: T,
    inserting_into_sharing_cache_ns: T,
    querying_selector_map_ns: T,
}

pub(super) fn website_json(result: &WebsiteResult) -> WebsiteJson<'_> {
    WebsiteJson {
        website: &result.website,
        before_preprocessing: variant_json("before_preprocessing", &result.before_preprocessing),
        preprocessing: preprocessing_json(&result.preprocessing),
        after_preprocessing: variant_json("after_preprocessing", &result.after_preprocessing),
    }
}

fn variant_json(label: &'static str, result: &MatchBenchResult) -> BenchmarkRunJson {
    let mean_timing_stats = result.timing_stats.mean();
    BenchmarkRunJson {
        label,
        mean_duration_ns: result.mean_duration().as_nanos(),
        stats: WebsiteStatsJson {
            sharing_instances: result.counting_stats.sharing_instances,
            selector_map_hits: result.counting_stats.selector_map_hits,
            fast_rejects: result.counting_stats.fast_rejects,
            slow_rejects: result.counting_stats.slow_rejects,
            slow_accepts: result.counting_stats.slow_accepts,
            time_spent_updating_bloom_filter_ns: mean_timing_stats.updating_bloom_filter.as_nanos(),
            time_spent_slow_rejecting_ns: mean_timing_stats.slow_rejecting.as_nanos(),
            time_spent_slow_accepting_ns: mean_timing_stats.slow_accepting.as_nanos(),
            time_spent_fast_rejecting_ns: mean_timing_stats.fast_rejecting.as_nanos(),
            time_spent_checking_style_sharing_ns: mean_timing_stats
                .checking_style_sharing
                .as_nanos(),
            time_spent_inserting_into_sharing_cache_ns: mean_timing_stats
                .inserting_into_sharing_cache
                .as_nanos(),
            time_spent_querying_selector_map_ns: mean_timing_stats.querying_selector_map.as_nanos(),
        },
    }
}

fn preprocessing_json(result: &PreprocessingResult) -> PreprocessingJson {
    PreprocessingJson {
        indexing_duration_ns: result.mean_indexing().as_nanos(),
        preprocessing_duration_ns: result.mean_overall().as_nanos(),
    }
}
