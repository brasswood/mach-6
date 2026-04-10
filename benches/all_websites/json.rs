use super::*;
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct WebsiteJson<'a> {
    website: &'a str,
    before_preprocessing: BenchmarkRunJson,
    preprocessing: PreprocessingJson,
    after_preprocessing: BenchmarkRunJson,
}

#[derive(Serialize)]
struct BenchmarkRunJson {
    label: &'static str,
    mean_duration_ns: u128,
    stats: WebsiteStatsJson,
}

#[derive(Serialize)]
struct PreprocessingJson {
    indexing_duration_ns: u128,
    preprocessing_duration_ns: u128,
}

#[derive(Serialize)]
struct WebsiteStatsJson {
    sharing_instances: usize,
    selector_map_hits: usize,
    fast_rejects: usize,
    slow_rejects: usize,
    slow_accepts: usize,
    time_spent_updating_bloom_filter_ns: u128,
    time_spent_slow_rejecting_ns: u128,
    time_spent_slow_accepting_ns: u128,
    time_spent_fast_rejecting_ns: u128,
    time_spent_checking_style_sharing_ns: u128,
    time_spent_inserting_into_sharing_cache_ns: u128,
    time_spent_querying_selector_map_ns: u128,
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
