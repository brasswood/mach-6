use log::error;
use mach_6::{self, build_substr_selector_index, convert_to_is_selectors, get_all_documents_and_selectors, stylist_from_selectors, substrings_from_selectors};
use mach_6::parse::{ParsedWebsite, get_document_and_selectors, websites_path};
use mach_6::structs::{Element, Selector};
use scraper::Html;
use selectors::matching::{CountingStats, SelectorStats, Statistics, TimingStats};
use smallvec::SmallVec;
use style::shared_lock::SharedRwLock;
use style::stylist::Stylist;
use std::collections::HashMap;
use std::cmp::Reverse;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use cssparser::ToCss as _;

use crate::json::WebsiteJson;

mod html;
mod json;

#[derive(Clone, Debug, Default)]
struct Samples<T>(Vec<T>); 

impl<T> Samples<T> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn mean(&self) -> <T as Mean>::Output
    where
        T: Mean,
    {
        T::mean(self.as_slice())
    }

    fn stddev(&self) -> <T as StdDev>::Output
    where
        T: Mean + StdDev,
    {
        let mean = self.mean();
        T::stddev(self.as_slice(), &mean)
    }

    fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    fn into_iter(self) -> std::vec::IntoIter<T> {
        self.0.into_iter()
    }

    fn as_slice(&self) -> &[T] {
        &self.0
    }

    fn first(&self) -> Option<&T> {
        self.0.first()
    }
}

#[derive(Debug, Clone, Copy)]
struct OnlineDurationStats {
    num_samples: usize,
    mean_ns: f64,
    /// The running sum of squared deviations from the mean
    m2_ns: f64,
}

impl OnlineDurationStats {
    fn push(&mut self, sample: Duration) {
        let x = sample.as_nanos() as f64;
        self.num_samples += 1;
        let delta = x - self.mean_ns;
        self.mean_ns += delta / self.num_samples as f64;
        let delta2 = x - self.mean_ns;
        self.m2_ns += delta * delta2;
    }

    fn mean(&self) -> Duration {
        assert!(self.num_samples != 0, "tried to compute online mean with no samples");
        Duration::from_nanos(self.mean_ns.round() as u64)
    }

    fn stddev(&self) -> Duration {
        assert!(self.num_samples != 0, "tried to compute online stddev with no samples");
        let variance = self.m2_ns / self.num_samples as f64;
        Duration::from_nanos(variance.sqrt().round() as u64)
    }
}

struct TimedResults<R> {
    total_duration: Duration,
    samples: Samples<R>,
}

impl<R> TimedResults<R> {
    fn overall_mean(&self) -> Duration {
        assert!(self.samples.len() != 0, "tried to compute overall mean on result with no samples");
        self.total_duration / u32::try_from(self.samples.len()).unwrap()
    }
}

trait Mean {
    type Output;

    fn mean(samples: &[Self]) -> Self::Output
    where
        Self: Sized;
}

trait StdDev: Mean {
    type Output;

    fn stddev(samples: &[Self], mean: &<Self as Mean>::Output) -> <Self as StdDev>::Output
    where
        Self: Sized;
}

impl Mean for Duration {
    type Output = Duration;

    fn mean(samples: &[Self]) -> Self::Output {
        assert!(!samples.is_empty(), "tried to compute mean of empty sample set");
        let total: Duration = samples.iter().copied().sum();
        total / samples.len() as u32
    }
}

impl StdDev for Duration {
    type Output = Duration;

    fn stddev(samples: &[Self], mean: &<Self as Mean>::Output) -> <Self as StdDev>::Output {
        assert!(
            !samples.is_empty(),
            "tried to compute standard deviation of empty sample set"
        );
        let variance = samples
            .iter()
            .map(|sample| {
                let delta = sample.as_nanos() as f64 - mean.as_nanos() as f64;
                delta * delta
            })
            .sum::<f64>()
            / samples.len() as f64;
        Duration::from_nanos(variance.sqrt().round() as u64)
    }
}

impl Mean for TimingStats {
    type Output = TimingStats;

    fn mean(samples: &[Self]) -> Self::Output {
        assert!(!samples.is_empty(), "tried to compute mean of empty sample set");
        let iter = samples.iter().copied();
        let sum = iter
            .reduce(|l, r| l + r)
            .expect("tried to compute mean of empty sample set");
        sum / samples.len() as u32
    }
}

impl StdDev for TimingStats {
    type Output = TimingStats;

    fn stddev(samples: &[Self], mean: &<Self as Mean>::Output) -> <Self as StdDev>::Output {
        assert!(
            !samples.is_empty(),
            "tried to compute standard deviation of empty sample set"
        );

        let stddev = |project: fn(&TimingStats) -> Duration| {
            let variance = samples
                .iter()
                .map(|sample| {
                    let delta = project(sample).as_nanos() as f64 - project(mean).as_nanos() as f64;
                    delta * delta
                })
                .sum::<f64>()
                / samples.len() as f64;
            Duration::from_nanos(variance.sqrt().round() as u64)
        };

        TimingStats {
            updating_bloom_filter: stddev(|sample| sample.updating_bloom_filter),
            checking_style_sharing: stddev(|sample| sample.checking_style_sharing),
            querying_selector_map: stddev(|sample| sample.querying_selector_map),
            fast_rejecting: stddev(|sample| sample.fast_rejecting),
            slow_rejecting: stddev(|sample| sample.slow_rejecting),
            slow_accepting: stddev(|sample| sample.slow_accepting),
            inserting_into_sharing_cache: stddev(|sample| sample.inserting_into_sharing_cache),
            _time_inside_buckets: stddev(|sample| sample._time_inside_buckets),
        }
    }
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
    aggregate_durations: Samples<Duration>,
}

/// Aggregated data for one matching variant in the website report.
///
/// A "variant" here means one of the two selector-matching configurations we
/// compare for a website, such as before preprocessing vs. after preprocessing.
/// This is the per-variant payload consumed by both the HTML report and the
/// per-website JSON output.
#[derive(Clone, Debug)]
struct MatchBenchResult {
    /// The total duration of the benched website
    total_duration: Duration,
    /// Counting stats of one sample (should be the same accross all samples)
    counting_stats: CountingStats,
    /// Per-sample timing stats
    timing_stats: Samples<TimingStats>,
    /// All slow-rejecting selectors and their aggregate slow-reject durations
    /// for each sample. Sorted in descending order by mean.
    selector_slow_reject_times: Vec<SelectorSlowRejectSamples>,
}

impl MatchBenchResult {
    fn new(
        stats: TimedResults<Statistics>,
        per_match_stats: TimedResults<SmallVec<[((Element, Selector), SelectorStats); 16]>>,
    ) -> Self {
        assert_eq!(stats.samples.len(), per_match_stats.samples.len());
        let counting_stats = stats
            .samples
            .first()
            .expect("expected at least one sample result")
            .counts;

        let timing_stats = stats
            .samples
            .0
            .iter()
            .map(|stats| stats.times)
            .collect();

        let mut map: HashMap<SelectorString, Vec<Duration>> = HashMap::new();
        for (i, per_match_stats) in per_match_stats.samples.into_iter().enumerate() {
            for ((_element, selector), selector_stats) in per_match_stats {
                let slow_reject_duration = match selector_stats {
                    SelectorStats::Bloom(bq) =>
                        bq.time_slow_rejecting.unwrap_or_default(),
                    SelectorStats::ScopeProximity(sp) =>
                        sp.time_slow_rejecting,
                };
                let samples = map.entry(SelectorString::from(&selector)).or_default();
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
            SelectorSlowRejectSamples { selector, aggregate_durations: Samples(durations) }
        ).collect();
        sorted.sort_unstable_by_key(|sel| Reverse(sel.aggregate_durations.mean()));
        MatchBenchResult {
            total_duration: stats.total_duration,
            counting_stats,
            timing_stats: Samples(timing_stats),
            selector_slow_reject_times: sorted,
        }
    }

    fn mean_duration(&self) -> Duration {
        self.total_duration / self.timing_stats.len() as u32
    }
}

/// Timing data for the preprocessing stage that sits between the two matching
/// variants in the report.
struct PreprocessingResult {
    /// The substring-indexing results
    indexing: TimedResults<()>,
    /// The total preprocessing results. This has `indexing` included in it.
    overall_preprocessing: TimedResults<()>,
}
impl PreprocessingResult {
    fn new(indexing: TimedResults<()>, overall_preprocessing: TimedResults<()>) -> Self {
        Self {
            indexing,
            overall_preprocessing,
        }
    }
    fn mean_indexing(&self) -> Duration {
        self.indexing.total_duration / self.indexing.samples.len() as u32
    }
    fn mean_overall(&self) -> Duration {
        self.overall_preprocessing.total_duration / self.overall_preprocessing.samples.len() as u32
    }
    fn mean_non_indexing(&self) -> Duration {
        self.mean_overall() - self.mean_indexing()
    }
}

/// All report data for one website: the baseline matching variant, the
/// preprocessing step, and the post-preprocessing matching variant.
struct WebsiteResult {
    website: String,
    before_preprocessing: MatchBenchResult,
    preprocessing: PreprocessingResult,
    after_preprocessing: MatchBenchResult,
}

fn main() {
    env_logger::Builder::new().filter_level(log::LevelFilter::Warn).init();
    let website_filter = std::env::args().nth(1).unwrap(); // will either be a website filter or --bench
    let website_filter = if website_filter == "--bench" {None} else {Some(website_filter)};
    let websites = get_documents(website_filter.as_deref());
    let results = websites.map(|w| {
        let before_preprocessing = bench_website(&format!("{} before preprocessing", w.name), &w.document, &w.stylist(), &w.stylesheet_lock);
        let substrings =
          substrings_from_selectors(w.selectors().iter());
        let indexing_results = bench_function(
          &format!("{} indexing", w.name),
          || { build_substr_selector_index(&w.document, substrings.clone()); }
        );
        let overall_preprocessing_results = bench_function(
          &format!("{} preprocessing", w.name),
          || { convert_to_is_selectors(&w.document, &w.selectors()); }
        );
        let preprocessed_selectors = convert_to_is_selectors(&w.document, &w.selectors());
        drop(substrings); // Why doesn't the compiler do this automatically? I don't know.
        let (preprocessed_stylist, preprocessed_lock) = stylist_from_selectors(&preprocessed_selectors);
        let after_preprocessing = bench_website(&format!("{} after preprocessing", w.name), &w.document, &preprocessed_stylist, &preprocessed_lock);
        let result = WebsiteResult {
            website: w.name,
            before_preprocessing,
            preprocessing: PreprocessingResult::new(
                indexing_results,
                overall_preprocessing_results,
            ),
            after_preprocessing,
        };
        result
    });
    let json_results = match results.map(|res| write_json(&res)).collect::<io::Result<Vec<_>>>() {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to write JSON: {}", e);
            std::process::exit(1);
        },
    };
    match write_report(&json_results) {
        Ok(report_dir) => eprintln!("Wrote report to {}", report_dir.display()),
        Err(e) => {
            error!("Failed to write report: {}", e);
            std::process::exit(1);
        }
    }
}

fn bench_website(benchmark_name: &str, document: &Html, stylist: &Stylist, stylesheet_lock: &SharedRwLock) -> MatchBenchResult {
    let overall_stats = bench_function(
        &format!("{benchmark_name} (without selector stats)"),
        || {
            let (_, overall_stats) =
                mach_6::match_selectors_with_style_sharing(
                    document,
                    stylist,
                    stylesheet_lock,
                    None,
                );
            overall_stats
        },
    );
    let per_match_stats = bench_function(
        &format!("{benchmark_name} (with selector stats)"),
        || {
            let mut per_match_stats = SmallVec::new();
            mach_6::match_selectors_with_style_sharing(
                document,
                stylist,
                stylesheet_lock,
                Some(&mut per_match_stats),
            );
            per_match_stats
        },
    );
    MatchBenchResult::new(overall_stats, per_match_stats)
}

fn get_documents(website_filter: Option<&str>) -> Box<dyn Iterator<Item = ParsedWebsite>> {
    if let Some(website_filter) = website_filter {
        let website_location = websites_path().join(website_filter);
        let website = match get_document_and_selectors(&website_location) {
            Ok(Some(website)) => website,
            Ok(None) => {
                eprintln!("{} is not a directory or contains no html files.", website_location.display());
                std::process::exit(1);
            },
            Err(e) => {
                error!("Could not parse website at {}: {}", website_location.display(), e);
                std::process::exit(1);
            },
        };
        Box::new(std::iter::once(website))
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

fn bench_function<F, R>(name: &str, func: F) -> TimedResults<R>
where
    F: Fn() -> R,
{
    const NUM_SAMPLES: u32 = 25;
    const WARM_UP_TIME: Duration = Duration::from_secs(5);
    let mut samples_vec = Vec::with_capacity(NUM_SAMPLES as usize);
    eprint!("Benchmarking {name}...warming up for {} seconds...", WARM_UP_TIME.as_secs_f32());
    warm_up(&WARM_UP_TIME, &func);
    eprint!("measuring {NUM_SAMPLES} samples...");
    let start = Instant::now();
    for _ in 0..NUM_SAMPLES {
      samples_vec.push(func());
    }
    let total_duration = start.elapsed();
    eprintln!("done. ({}, {} total)", format_duration(total_duration / NUM_SAMPLES), format_duration(total_duration));
    TimedResults {
        total_duration,
        samples: Samples(samples_vec),
    }
}

fn warm_up<F, R>(warm_up_time: &Duration, func: &F)
where
    F: Fn() -> R
{
    let start = Instant::now();
    while start.elapsed() < *warm_up_time {
        func();
    }
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

fn report_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("all_websites_report")
}

fn write_json(result: &WebsiteResult) -> io::Result<WebsiteJson>{
    let report_dir = report_dir();
    let json_dir = report_dir.join("json");
    fs::create_dir_all(&report_dir)?;
    fs::create_dir_all(&json_dir)?;
    let json = WebsiteJson::from(result);
    let file_name = format!("{}.json", make_filename_safe(&json.website));
    let json_path = json_dir.join(file_name);
    let serialized = serde_json::to_string_pretty(&json)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    fs::write(json_path, serialized)?;
    Ok(json)
}

fn write_report(json_results: &[WebsiteJson]) -> io::Result<PathBuf> {
    let report_dir = report_dir();
    let report = html::ReportTemplate::from(json_results);
    let html = report.to_string();
    fs::write(report_dir.join("index.html"), html)?;
    Ok(report_dir)
}

fn make_filename_safe(string: &str) -> String {
    let mut string = string.replace(
        &['?', '"', '/', '\\', '*', '<', '>', ':', '|', '^'][..],
        "_",
    );
    if string.len() > 240 {
        let mut boundary = 240;
        while boundary > 0 && !string.is_char_boundary(boundary) {
            boundary -= 1;
        }
        string.truncate(boundary);
    }
    string
}
