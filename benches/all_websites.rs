use log::error;
use mach_6::{self, get_all_documents_and_selectors};
use mach_6::parse::{ParsedWebsite, get_document_and_selectors, websites_path};
use std::time::{Duration, Instant};

struct TimedResult<R> {
    duration: Duration,
    result: R,
}

fn main() {
    env_logger::Builder::new().filter_level(log::LevelFilter::Warn).init();
    let website_filter = std::env::args().nth(1);
    let websites = get_documents(website_filter.as_deref());
    let results: Vec<_> = websites.map(|website| {
        let selector_map = mach_6::build_selector_map(&website.selectors);
        let timed_results = bench_function(
            &website.name,
            || mach_6::match_selectors_with_style_sharing(&website.document, &selector_map),
        );
        (website.name, timed_results)
    }).collect();
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

fn bench_function<F, R>(name: &str, func: F) -> TimedResult<R>
where
    F: Fn() -> R,
{
    eprint!("Benchmarking {name}...");
    let start = Instant::now();
    let result = func();
    let duration = start.elapsed();
    eprintln!("done. ({})", format_duration(duration));
    TimedResult {
        duration,
        result,
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
