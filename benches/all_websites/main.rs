use log::{error, warn};
use mach_6::{self, get_all_documents_and_selectors, stylist_from_selectors};
use mach_6::parse::{ParsedWebsite, get_document_and_selectors, websites_path};
use mach_6::preprocessing::{self, concretize, distribute};
use mach_6::structs::Selector;
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
use std::process::Command;
use cssparser::ToCss as _;
use time::OffsetDateTime;

use crate::json::{ReportJson, ReportMetadataJson, WebsiteJson};
use crate::stats::Samples;

mod json;
mod stats;

struct TimedResults<R> {
    total_duration: tsc_timer::Duration,
    samples: Samples<R>,
}

impl<R> TimedResults<R> {
    fn overall_mean(&self) -> tsc_timer::Duration {
        assert!(self.samples.len() != 0, "tried to compute overall mean on result with no samples");
        self.total_duration / u64::try_from(self.samples.len()).unwrap()
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
    aggregate_durations: Samples<tsc_timer::Duration>,
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
    total_duration: tsc_timer::Duration,
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
        per_match_stats: TimedResults<SmallVec<[(&Selector, SelectorStats); 16]>>,
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
            .collect();

        let mut map: HashMap<SelectorString, Vec<tsc_timer::Duration>> = HashMap::new();
        for (i, per_match_stats) in per_match_stats.samples.into_iter().enumerate() {
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
        MatchBenchResult {
            total_duration: stats.total_duration,
            counting_stats,
            timing_stats: Samples::from_vec(timing_stats),
            selector_slow_reject_times: sorted,
        }
    }

    fn mean_duration(&self) -> tsc_timer::Duration {
        self.total_duration / self.timing_stats.len() as u64
    }
}

/// Timing data for the preprocessing stage that sits between the two matching
/// variants in the report.
struct PreprocessingResult {
    /// The substring-indexing results
    indexing: TimedResults<()>,
    /// The total is conversion results. This has `indexing` included in it.
    overall_is_conversion: TimedResults<()>,
    /// The :is() distribution results
    distribution: TimedResults<()>
}
impl PreprocessingResult {
    fn new(indexing: TimedResults<()>, overall_is_conversion: TimedResults<()>, distribution: TimedResults<()>) -> Self {
        Self {
            indexing,
            overall_is_conversion,
            distribution,
        }
    }
    fn mean_indexing(&self) -> tsc_timer::Duration {
        self.indexing.total_duration / self.indexing.samples.len() as u64
    }
    fn mean_is_conversion(&self) -> tsc_timer::Duration {
        self.overall_is_conversion.total_duration / self.overall_is_conversion.samples.len() as u64
    }
    fn mean_non_indexing(&self) -> tsc_timer::Duration {
        self.mean_is_conversion() - self.mean_indexing()
    }
    fn mean_distributing(&self) -> tsc_timer::Duration {
        self.distribution.total_duration / self.distribution.samples.len() as u64
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

const NUM_SAMPLES: u64 = 25;

fn main() {
    env_logger::Builder::new().filter_level(log::LevelFilter::Warn).init();
    let time_start = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let website_filter: Vec<String> = std::env::args()
        .skip(1) // the executable name
        .filter(|a| !a.starts_with("--"))
        .collect();
    let websites = get_documents(website_filter.iter().map(String::as_str));
    let results = websites.map(|w| {
        let before_preprocessing = bench_website(&format!("{} before preprocessing", w.name), w.document(), w.stylist(), w.stylesheet_lock());
        let substrings =
          concretize::substrings_from_selectors(w.selectors().iter());
        let indexing_results = bench_function(
          &format!("{} indexing", w.name),
          || { concretize::build_substr_selector_index(w.document(), substrings.clone()); },
          NUM_SAMPLES,
        );
        drop(substrings); // Why doesn't the compiler do this automatically? I don't know.
        let overall_is_conversion_results = bench_function(
          &format!("{} :is() conversion", w.name),
          || { concretize::convert_to_is_selectors(w.document(), w.selectors()); },
          NUM_SAMPLES,
        );
        let is = concretize::convert_to_is_selectors(w.document(), w.selectors());
        let distribute = || {
            let _: Vec<_> = is
                .iter()
                .flat_map(distribute::DistributedSelectors::from_selector)
                .collect();
        };
        let distributing_results = bench_function(
            &format!("{} :is() distribution", w.name),
            distribute,
            NUM_SAMPLES,
        );
        let preprocessed_selectors = preprocessing::preprocess(w.document(), w.selectors());
        let (preprocessed_stylist, preprocessed_lock) = stylist_from_selectors(&preprocessed_selectors);
        let after_preprocessing = bench_website(&format!("{} after preprocessing", w.name), w.document(), &preprocessed_stylist, &preprocessed_lock);
        let result = WebsiteResult {
            website: w.name,
            before_preprocessing,
            preprocessing: PreprocessingResult::new(
                indexing_results,
                overall_is_conversion_results,
                distributing_results,
            ),
            after_preprocessing,
        };
        result
    });
    let websites_json = results
        .map(|res| WebsiteJson::from(&res))
        .collect::<Vec<_>>();

    let git_metadata = match collect_report_git_metadata() {
        Ok(git) => Some(git),
        Err(e) => {
            warn!("Failed to collect git metadata: {}", e);
            None
        },
    };
    let time_end = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let metadata = ReportMetadataJson::new(git_metadata, time_start, time_end);

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

fn bench_website(benchmark_name: &str, document: &Html, stylist: &Stylist, stylesheet_lock: &SharedRwLock) -> MatchBenchResult {
    let overall_stats = bench_function(
        benchmark_name,
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
        NUM_SAMPLES,
    );
    print!("Getting selector stats for {benchmark_name}...");
    let mut per_match_stats = SmallVec::new();
    mach_6::match_selectors_with_style_sharing(
        document,
        stylist,
        stylesheet_lock,
        Some(&mut per_match_stats),
    );
    println!("done.");
    let results = TimedResults {
        total_duration: tsc_timer::Duration::from_cycles(0), // whatever
        samples: Samples::from_vec(vec![per_match_stats]),
    };
    MatchBenchResult::new(overall_stats, results)
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
    const WARM_UP_TIME: std::time::Duration = std::time::Duration::from_secs(5);
    let mut samples_vec = Vec::with_capacity(num_samples as usize);
    eprint!("Benchmarking {name}...warming up for {} seconds...", WARM_UP_TIME.as_secs_f32());
    let num_warmup_iterations = warm_up(&WARM_UP_TIME, &func);
    eprint!("done ({num_warmup_iterations} iterations), measuring {num_samples} samples...");
    let start = tsc_timer::Start::now();
    for _ in 0..num_samples {
      samples_vec.push(func());
    }
    let total_duration = start.elapsed();
    eprintln!("done. ({}, {} total)", format_duration(total_duration / num_samples), format_duration(total_duration));
    TimedResults {
        total_duration,
        samples: Samples::from_vec(samples_vec),
    }
}

fn warm_up<F, R>(warm_up_time: &std::time::Duration, func: F) -> usize
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

