use log::{error, warn};
use mach_6::{self, MatchingContext, Optimizations, get_all_documents_and_selectors, stylesheet_from_selectors};
use mach_6::parse::{ParsedWebsite, get_document_and_selectors, websites_path};
use mach_6::preprocessing::{concretize, distribute};
use mach_6::structs::Selector;
use scraper::Html;
use selectors::matching::{CountingStats, SelectorStats, Statistics, TimingStats};
use smallvec::SmallVec;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use cssparser::ToCss as _;
use time::OffsetDateTime;

use crate::json::{ReportJson, ReportMetadataJson, ReportSourceJson, WebsiteJson};
use crate::stats::Samples;

mod json;
mod stats;

struct TimedResults<R> {
    samples: Samples<R>,
    // measure duration of individual samples here;
    // don't assume `Samples<R>` will do it.
    sample_durations: Samples<tsc_timer::Duration>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct SelectorString(String);
impl From<&Selector> for SelectorString {
    fn from(value: &Selector) -> Self {
        Self(value.to_css_string())
    }
}

/// A selector and its aggregate (i.e. for every element) slow-reject time in
/// each sample
#[derive(Clone, Debug)]
struct SelectorSlowRejectSamples {
    selector: SelectorString,
    aggregate_durations: Samples<tsc_timer::Duration>,
}

#[derive(Clone, Debug)]
struct VariantTimingSegments {
    updating_bloom_filter: Samples<tsc_timer::Duration>,
    checking_style_sharing: Samples<tsc_timer::Duration>,
    querying_selector_map: Samples<tsc_timer::Duration>,
    fast_rejecting: Samples<tsc_timer::Duration>,
    slow_rejecting: Samples<tsc_timer::Duration>,
    slow_accepting: Samples<tsc_timer::Duration>,
    inserting_into_sharing_cache: Samples<tsc_timer::Duration>,
    indexing: Option<Samples<tsc_timer::Duration>>,
    overall_is_conversion: Option<Samples<tsc_timer::Duration>>,
    distribution: Option<Samples<tsc_timer::Duration>>,
}

impl VariantTimingSegments {
    fn from_matching_stats(value: &Samples<TimingStats>) -> Self {
        let project = |project: fn(&TimingStats) -> tsc_timer::Duration| {
            Samples::from_vec(
                value.iter()
                    .map(|sample| project(sample))
                    .collect(),
            )
        };
        Self {
            updating_bloom_filter: project(|stats| stats.updating_bloom_filter),
            checking_style_sharing: project(|stats| stats.checking_style_sharing),
            querying_selector_map: project(|stats| stats.querying_selector_map),
            fast_rejecting: project(|stats| stats.fast_rejecting),
            slow_rejecting: project(|stats| stats.slow_rejecting),
            slow_accepting: project(|stats| stats.slow_accepting),
            inserting_into_sharing_cache: project(|stats| stats.inserting_into_sharing_cache),
            indexing: None,
            overall_is_conversion: None,
            distribution: None,
        }
    }

    fn mean_total_duration(&self) -> tsc_timer::Duration {
        let mut total = self.updating_bloom_filter.mean()
            + self.checking_style_sharing.mean()
            + self.querying_selector_map.mean()
            + self.fast_rejecting.mean()
            + self.slow_rejecting.mean()
            + self.slow_accepting.mean()
            + self.inserting_into_sharing_cache.mean();
        if let Some(indexing) = self.indexing.as_ref() {
            total += indexing.mean();
        }
        if let Some(overall_is_conversion) = self.overall_is_conversion.as_ref() {
            total += overall_is_conversion.mean();
        }
        if let Some(distribution) = self.distribution.as_ref() {
            total += distribution.mean();
        }
        total
    }

    fn derived_is_conversion_mean(&self) -> Option<tsc_timer::Duration> {
        let indexing = self.indexing.as_ref()?;
        let overall_is_conversion = self.overall_is_conversion.as_ref()?;
        Some(overall_is_conversion.mean() - indexing.mean())
    }
}

/// Aggregated data for one benchmarked optimization variant.
#[derive(Clone, Debug)]
struct VariantResult {
    /// Counting stats of one sample (should be the same accross all samples)
    counting_stats: CountingStats,
    /// Timing samples
    timing_segments: VariantTimingSegments,
    /// All slow-rejecting selectors and their aggregate slow-reject durations
    /// for each sample. Sorted in descending order by mean.
    selector_slow_reject_times: Vec<SelectorSlowRejectSamples>,
}

impl VariantResult {
    fn new(
        stats: TimedResults<Statistics>,
        per_match_stats: Samples<SmallVec<[(&Selector, SelectorStats); 16]>>,
    ) -> Self {
        let counting_stats = stats
            .samples
            .first()
            .expect("expected at least one sample result")
            .counts;

        let timing_stats = stats
            .samples
            .as_slice()
            .iter()
            .map(|stats| stats.times)
            .collect::<Vec<_>>();

        let mut map: HashMap<SelectorString, Vec<tsc_timer::Duration>> = HashMap::new();
        for (i, per_match_stats) in per_match_stats.into_iter().enumerate() {
            for (selector, selector_stats) in per_match_stats {
                let slow_reject_duration = match selector_stats {
                    SelectorStats::Bloom(bq) =>
                        bq.time_slow_rejecting.unwrap_or_default(),
                    SelectorStats::ScopeProximity(sp) =>
                        sp.time_slow_rejecting,
                };
                let samples = map.entry(SelectorString::from(selector)).or_default();
                // If this is the first time we have touched the vector at this
                // selector for this sample (samples.len() == i), push a new
                // Duration onto the end. Otherwise, samples.len() == i + 1,
                // which means we have already started building up an aggregate
                // duration for this sample, so just accumulate that.
                if samples.len() == i {
                    samples.push(slow_reject_duration);
                } else {
                    samples[i] += slow_reject_duration;
                }
            }
        }

        let mut sorted: Vec<_> = map.into_iter().map(|(selector, durations)|
            SelectorSlowRejectSamples { selector, aggregate_durations: Samples::from_vec(durations) }
        ).collect();
        sorted.sort_unstable_by_key(|sel| Reverse(sel.aggregate_durations.mean()));
        let result = VariantResult {
            counting_stats,
            timing_segments: VariantTimingSegments::from_matching_stats(&Samples::from_vec(timing_stats)),
            selector_slow_reject_times: sorted,
        };
        result
    }

    fn add_is_conversion_timings(
        &mut self,
        indexing: Samples<tsc_timer::Duration>,
        overall_is_conversion: Samples<tsc_timer::Duration>,
    ) {
        self.timing_segments.indexing = Some(indexing);
        self.timing_segments.overall_is_conversion = Some(overall_is_conversion);
    }

    fn add_distribution(&mut self, distribution: Samples<tsc_timer::Duration>) {
        self.timing_segments.distribution = Some(distribution);
    }

    fn mean_duration(&self) -> tsc_timer::Duration {
        self.timing_segments.mean_total_duration()
    }
}

#[derive(Clone, Copy)]
struct VariantSpec {
    id: usize,
    label: Option<&'static str>,
    optimizations: Optimizations,
}

const VARIANT_SPECS: [VariantSpec; 2] = [
    VariantSpec {
        id: 0,
        label: None,
        optimizations: Optimizations {
            is_conversion: false,
            distribution: false,
        },
    },
    VariantSpec {
        id: 1,
        label: None,
        optimizations: Optimizations {
            is_conversion: true,
            distribution: true,
        },
    },
];

fn variant_specs() -> &'static [VariantSpec] {
    &VARIANT_SPECS
}

struct WebsiteResult {
    website: String,
    variants: Vec<VariantResult>,
}

const NUM_SAMPLES: u64 = 25;

fn main() {
    env_logger::Builder::new().filter_level(log::LevelFilter::Warn).init();
    let git_metadata = match collect_report_git_metadata() {
        Ok(git) => Some(git),
        Err(e) => {
            warn!("Failed to collect git metadata: {}", e);
            None
        },
    };
    let time_start = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let website_filter: Vec<String> = std::env::args()
        .skip(1) // the executable name
        .filter(|a| !a.starts_with("--"))
        .collect();
    let websites = get_documents(website_filter.iter().map(String::as_str));
    let results = websites.map(|w| {
        let variants = variant_specs()
            .iter()
            .map(|variant_spec| bench_variant(&w, variant_spec))
            .collect();
        let result = WebsiteResult {
            website: w.name,
            variants,
        };
        result
    });
    let websites_json = results
        .map(|res| WebsiteJson::from(&res))
        .collect::<Vec<_>>();

    let time_end = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let metadata = ReportMetadataJson::new(report_source_from_env(), git_metadata, time_start, time_end);

    let report_json = ReportJson {
        metadata,
        websites: websites_json,
    };

    match fs::create_dir_all(&report_dir()) {
        Ok(()) => (),
        Err(e) => {
            error!("Failed to create report directory: {e}");
            return
        },
    };
    let report_json_result = write_report_json(&report_json);
    let html_result = copy_html_js();
    match report_json_result.and(html_result)
    {
        Ok(()) => eprintln!("Wrote report to {}", report_dir().display()),
        Err(e) => error!("{e}"),
    };
}

fn bench_variant(website: &ParsedWebsite, variant_spec: &VariantSpec) -> VariantResult {
    // TODO: Html will not be able to be reused
    // between variant runs once we add fail caches back
    // in
    let document = website.document();
    let matching_context = website.get_matcher();
    let benchmark_name = format!("{} variant {}", website.name, variant_spec.id);

    if !variant_spec.optimizations.is_conversion && !variant_spec.optimizations.distribution {
        return bench_website(&benchmark_name, document, &matching_context);
    }

    let selectors = matching_context.get_selectors();
    let (selectors_after_is_conversion, indexing_durations, overall_is_conversion_durations) =
        if variant_spec.optimizations.is_conversion {
            let substrings = concretize::substrings_from_selectors(selectors.iter());
            let indexing_results = bench_function(
                &format!("{} indexing", website.name),
                || { concretize::build_substr_selector_index(document, substrings.clone()); },
                NUM_SAMPLES,
            );
            let overall_is_conversion_results = bench_function(
                &format!("{} :is() conversion", website.name),
                || { concretize::convert_to_is_selectors(document, &selectors); },
                NUM_SAMPLES,
            );
            let converted_selectors = concretize::convert_to_is_selectors(document, &selectors);
            (
                converted_selectors,
                Some(indexing_results.sample_durations),
                Some(overall_is_conversion_results.sample_durations),
            )
        } else {
            (selectors.to_vec(), None, None)
        };

    let (preprocessed_selectors, distribution_durations) =
        if variant_spec.optimizations.distribution {
            let distributing_results = bench_function(
                &format!("{} :is() distribution", website.name),
                || {
                    let _: Vec<_> = selectors_after_is_conversion
                        .iter()
                        .flat_map(distribute::DistributedSelectors::from_selector)
                        .collect();
                },
                NUM_SAMPLES,
            );
            let preprocessed_selectors = selectors_after_is_conversion
                .iter()
                .flat_map(distribute::DistributedSelectors::from_selector)
                .collect();
            (preprocessed_selectors, Some(distributing_results.sample_durations))
        } else {
            (selectors_after_is_conversion, None)
        };

    let (preprocessed_stylesheet, preprocessed_lock) =
        stylesheet_from_selectors(preprocessed_selectors.iter());
    let preprocessed_context = MatchingContext::new(
        std::iter::once(&preprocessed_stylesheet),
        preprocessed_lock,
    );
    let mut variant_result = bench_website(
        &benchmark_name,
        document,
        &preprocessed_context,
    );

    if let (Some(indexing_durations), Some(overall_is_conversion_durations)) =
        (indexing_durations, overall_is_conversion_durations)
    {
        variant_result.add_is_conversion_timings(
            indexing_durations,
            overall_is_conversion_durations,
        );
    }
    if let Some(distribution_durations) = distribution_durations {
        variant_result.add_distribution(distribution_durations);
    }

    variant_result
}

fn bench_website(
    benchmark_name: &str,
    document: &Html,
    matching_context: &MatchingContext,
) -> VariantResult {
    let overall_stats = bench_function(
        benchmark_name,
        || {
            let (_, overall_stats) =
                mach_6::match_selectors_with_style_sharing(
                    document,
                    matching_context,
                    Optimizations::from_none(),
                    None,
                );
            overall_stats
        },
        NUM_SAMPLES,
    );
    print!("Getting selector stats for {benchmark_name}...");
    let mut per_match_stats = SmallVec::new();
    mach_6::match_selectors_with_style_sharing(
        document,
        matching_context,
        Optimizations::from_none(),
        Some(&mut per_match_stats),
    );
    println!("done.");
    VariantResult::new(overall_stats, Samples::from_vec(vec![per_match_stats]))
}

fn get_documents<'a>(website_filter: impl Iterator<Item = &'a str> + 'a) -> Box<dyn Iterator<Item = ParsedWebsite> + 'a> {
    let mut website_filter = website_filter.peekable();
    if website_filter.peek().is_some() {
        let websites = website_filter.map(|website_name| {
            let website_location = websites_path().join(website_name);
            match get_document_and_selectors(&website_location) {
                Ok(Some(website)) => website,
                Ok(None) => {
                    eprintln!("{} is not a directory or contains no html files.", website_location.display());
                    std::process::exit(1);
                },
                Err(e) => {
                    error!("Could not parse website at {}: {}", website_location.display(), e);
                    std::process::exit(1);
                },
            }
        });
        Box::new(websites)
    } else {
        let res = match get_all_documents_and_selectors(&websites_path()) {
            Ok(websites) => {
                websites.filter_map(|website_result| {
                    match website_result {
                        Ok(website) => Some(website),
                        Err(e) => {
                            error!("Could not parse website at {}: {}", e.path.as_deref().unwrap().display(), e);
                            None
                        }
                    }
                })
            },
            Err(e) => {
                error!("Could not get websites from {}: {}", websites_path().display(), e);
                std::process::exit(1);
            }
        };
        Box::new(res)
    }
}

fn bench_function<F, R>(name: &str, func: F, num_samples: u64) -> TimedResults<R>
where
    F: Fn() -> R,
{
    const WARM_UP_ITERATIONS: usize = 100;
    const WARM_UP_TIME: std::time::Duration = std::time::Duration::from_millis(500);
    let mut samples_vec = Vec::with_capacity(num_samples as usize);
    let mut sample_durations = Vec::with_capacity(num_samples as usize);
    eprint!("Benchmarking {name}...warming up for {} seconds...", WARM_UP_TIME.as_secs_f32());
    warm_up_time(&WARM_UP_TIME, &func);
    eprint!("measuring {num_samples} samples...");
    for _ in 0..num_samples {
      let sample_start = tsc_timer::Start::now();
      samples_vec.push(func());
      sample_durations.push(sample_start.elapsed());
    }
    let total_duration = sample_durations.iter().fold(Default::default(), |acc, elt| acc + *elt);
    let sample_durations = Samples::from_vec(sample_durations);
    eprintln!("done. ({}, {} total)", format_duration(sample_durations.mean()), format_duration(total_duration));
    TimedResults {
        samples: Samples::from_vec(samples_vec),
        sample_durations: sample_durations,
    }
}

fn warm_up_iterations<F, R>(warm_up_iterations: usize, func: F)
where
    F: Fn() -> R
{
    for _ in 0..warm_up_iterations {
        func();
    }
}

fn warm_up_time<F, R>(warm_up_time: &std::time::Duration, func: F) -> usize
where
    F: Fn() -> R
{
    let mut num_iterations = 0;
    let start = std::time::Instant::now();
    while start.elapsed() < *warm_up_time {
        func();
        num_iterations += 1;
    }
    num_iterations
}

fn sample_here<F, R>(num_iterations: usize, func: F)
where
    F: Fn() -> R
{
    for _ in 0..num_iterations {
        func();
    }
}

fn format_duration(duration: tsc_timer::Duration) -> String {
    let (multiplier, divisor) = if duration.cycles() >= 1_000_000_000_000 {
        ("T", 1_000_000_000_000.0)
    } else if duration.cycles() >= 1_000_000_000 {
        ("B", 1_000_000_000.0)
    } else if duration.cycles() >= 1_000_000 {
        ("M", 1_000_000.0)
    } else if duration.cycles() >= 1_000 {
        ("K", 1_000.0)
    } else {
        ("", 1.0)
    };
    format!("{:.3}{} cycles", duration.cycles() as f64 / divisor, multiplier)
}

fn report_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("all_websites_report")
}

fn report_source_from_env() -> ReportSourceJson {
    match std::env::var("NIGHTLY").ok().as_deref() {
        Some("1") => ReportSourceJson::Nightly,
        _ => ReportSourceJson::Local,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct CommitHash(String);

struct ReportGitMetadata {
    commit_hash: CommitHash,
    tagline: String,
    message: String,
    dirty: bool,
    branch: Option<String>,
}

fn collect_report_git_metadata() -> io::Result<ReportGitMetadata> {
    Ok(ReportGitMetadata {
        commit_hash: CommitHash(git_output(&["rev-parse", "HEAD"])?),
        tagline: git_output(&["show", "-s", "--format=%s", "HEAD"])?,
        message: git_output(&["show", "-s", "--format=%b", "HEAD"])?,
        dirty: git_is_dirty()?,
        branch: {
            let branch = git_output(&["branch", "--show-current"])?;
            let trimmed = branch.trim().to_owned();
            (!trimmed.is_empty()).then_some(trimmed)
        },
    })
}

fn git_output(args: &[&str]) -> io::Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "git {} failed with status {}",
            args.join(" "),
            output.status,
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let trimmed = stdout.trim().to_owned();
    Ok(trimmed)
}

fn git_is_dirty() -> io::Result<bool> {
    let status = Command::new("git")
        .args(["diff-index", "--quiet", "--ignore-submodules=none", "HEAD", "--"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()?;

    match status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(io::Error::other(format!(
            "git diff-index failed with status {status}"
        ))),
    }
}

fn write_report_json(json: &ReportJson) -> io::Result<()> {
    let report_json = serde_json::to_string_pretty(json)
        .map_err(|err|
            io::Error::new(io::ErrorKind::InvalidData, format!("Failed to serialize report.json: {err}"))
        )?;
    fs::write(report_dir().join("report.json"), report_json)
        .map_err(|err|
            io::Error::new(err.kind(), format!("Failed to write report.json: {err}"))
        )?;
    Ok(())
}

fn copy_html_js() -> io::Result<()> {
    let report_dir = report_dir();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches").join("all_websites").join("ui").join("report.html"),
        report_dir.join("index.html"),
    )
        .map_err(|err|
            io::Error::new(err.kind(), format!("Failed to copy report.html to index.html: {err}"))
        )?;
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("all_websites_ui").join("report.js"),
        report_dir.join("report.js"),
    )
        .map_err(|err|
            io::Error::new(err.kind(), format!("Failed to copy compiled report.js: {err}"))
        )?;
    Ok(())
}
