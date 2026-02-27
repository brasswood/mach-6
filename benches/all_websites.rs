use log::error;
use mach_6::{self, get_all_documents_and_selectors};
use mach_6::parse::{ParsedWebsite, get_document_and_selectors, websites_path};
use selectors::matching::Statistics;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct TimedResult<R> {
    duration: Duration,
    result: R,
}

struct WebsiteResult {
    website: String,
    duration: Duration,
    stats: Statistics,
}

#[derive(Serialize)]
struct WebsiteJson<'a> {
    website: &'a str,
    total_duration_ns: u128,
    total_duration_display: String,
    stats: WebsiteStatsJson,
}

#[derive(Serialize)]
struct WebsiteStatsJson {
    sharing_instances: Option<usize>,
    selector_map_hits: Option<usize>,
    fast_rejects: Option<usize>,
    slow_rejects: Option<usize>,
    time_spent_slow_rejecting_ns: Option<u128>,
    time_spent_slow_rejecting_display: Option<String>,
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
        let TimedResult {
            duration,
            result: (_matches, stats),
        } = timed_results;
        WebsiteResult {
            website: website.name,
            duration,
            stats,
        }
    }).collect();
    match write_report(&results) {
        Ok(report_dir) => eprintln!("Wrote report to {}", report_dir.display()),
        Err(e) => {
            error!("Failed to write report: {}", e);
            std::process::exit(1);
        }
    }
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

fn write_report(results: &[WebsiteResult]) -> io::Result<PathBuf> {
    let report_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("all_websites_report");
    fs::create_dir_all(&report_dir)?;

    for result in results {
        let file_name = format!("{}.json", make_filename_safe(&result.website));
        let json_path = report_dir.join(file_name);
        let payload = WebsiteJson {
            website: &result.website,
            total_duration_ns: result.duration.as_nanos(),
            total_duration_display: format_duration(result.duration),
            stats: WebsiteStatsJson {
                sharing_instances: result.stats.sharing_instances,
                selector_map_hits: result.stats.selector_map_hits,
                fast_rejects: result.stats.fast_rejects,
                slow_rejects: result.stats.slow_rejects,
                time_spent_slow_rejecting_ns: result
                    .stats
                    .time_spent_slow_rejecting
                    .map(|d| d.as_nanos()),
                time_spent_slow_rejecting_display: result
                    .stats
                    .time_spent_slow_rejecting
                    .map(format_duration),
            },
        };
        let serialized = serde_json::to_string_pretty(&payload)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        fs::write(json_path, serialized)?;
    }

    let html = render_index_html(results);
    fs::write(report_dir.join("index.html"), html)?;
    Ok(report_dir)
}

fn render_index_html(results: &[WebsiteResult]) -> String {
    let max_duration_ns = results
        .iter()
        .map(|result| result.duration.as_nanos())
        .max()
        .unwrap_or(1)
        .max(1);

    let mut sections = String::new();
    for result in results {
        let width_pct = (result.duration.as_nanos() as f64 / max_duration_ns as f64) * 100.0;
        let website = escape_html(&result.website);
        let json_file = format!("{}.json", make_filename_safe(&result.website));
        let slow_rejecting = result
            .stats
            .time_spent_slow_rejecting
            .map(format_duration)
            .unwrap_or_else(|| "N/A".to_string());
        sections.push_str(&format!(
            r#"
<details class="site">
  <summary>
    <div class="row">
      <div class="name">{website}</div>
      <div class="bar-wrap"><div class="bar" style="width: {width_pct:.2}%"></div></div>
      <div class="time">{total_time}</div>
    </div>
  </summary>
  <div class="details">
    <table>
      <tbody>
        <tr><th>Sharing Instances</th><td>{sharing_instances}</td></tr>
        <tr><th>Selector Map Hits</th><td>{selector_map_hits}</td></tr>
        <tr><th>Fast Rejects</th><td>{fast_rejects}</td></tr>
        <tr><th>Slow Rejects</th><td>{slow_rejects}</td></tr>
        <tr><th>Time Spent Slow Rejecting</th><td>{slow_rejecting}</td></tr>
      </tbody>
    </table>
    <p><a href="{json_file}">JSON data</a></p>
  </div>
</details>
"#,
            website = website,
            width_pct = width_pct,
            total_time = format_duration(result.duration),
            sharing_instances = format_optional_usize(result.stats.sharing_instances),
            selector_map_hits = format_optional_usize(result.stats.selector_map_hits),
            fast_rejects = format_optional_usize(result.stats.fast_rejects),
            slow_rejects = format_optional_usize(result.stats.slow_rejects),
            slow_rejecting = slow_rejecting,
            json_file = escape_html(&json_file),
        ));
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>All Websites Benchmark Report</title>
  <style>
    :root {{
      --bg: #f7f7f3;
      --fg: #1d232b;
      --muted: #5b6470;
      --bar: #0f766e;
      --bar-bg: #d9e5e3;
      --card: #ffffff;
      --line: #d8d8d0;
    }}
    body {{
      margin: 0;
      padding: 24px;
      font-family: "Segoe UI", Tahoma, Geneva, Verdana, sans-serif;
      color: var(--fg);
      background: linear-gradient(180deg, #f7f7f3 0%, #f2f5f8 100%);
    }}
    main {{
      max-width: 980px;
      margin: 0 auto;
    }}
    h1 {{
      margin: 0 0 4px 0;
      font-size: 26px;
    }}
    .subtitle {{
      margin: 0 0 20px 0;
      color: var(--muted);
    }}
    .site {{
      background: var(--card);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 10px 12px;
      margin-bottom: 10px;
    }}
    summary {{
      list-style: none;
      cursor: pointer;
    }}
    summary::-webkit-details-marker {{
      display: none;
    }}
    .row {{
      display: grid;
      grid-template-columns: minmax(150px, 220px) minmax(120px, 1fr) 120px;
      align-items: center;
      gap: 12px;
    }}
    .name {{
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-weight: 600;
    }}
    .bar-wrap {{
      width: 100%;
      height: 14px;
      border-radius: 999px;
      background: var(--bar-bg);
      overflow: hidden;
    }}
    .bar {{
      height: 100%;
      background: var(--bar);
    }}
    .time {{
      text-align: right;
      font-variant-numeric: tabular-nums;
    }}
    .details {{
      padding-top: 12px;
      margin-top: 10px;
      border-top: 1px solid var(--line);
    }}
    table {{
      border-collapse: collapse;
      width: 100%;
      max-width: 600px;
    }}
    th, td {{
      text-align: left;
      padding: 4px 8px 4px 0;
      vertical-align: top;
    }}
    th {{
      width: 260px;
      color: var(--muted);
      font-weight: 600;
    }}
    p {{
      margin: 10px 0 0 0;
    }}
    @media (max-width: 700px) {{
      .row {{
        grid-template-columns: 1fr;
        gap: 6px;
      }}
      .time {{
        text-align: left;
      }}
    }}
  </style>
</head>
<body>
  <main>
    <h1>All Websites Benchmark Report</h1>
    <p class="subtitle">Each row shows total runtime; expand for detailed selector statistics and raw JSON.</p>
    {sections}
  </main>
</body>
</html>
"#,
        sections = sections
    )
}

fn format_optional_usize(value: Option<usize>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(|| "N/A".to_string())
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

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
