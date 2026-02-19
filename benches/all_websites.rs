use criterion::{criterion_group, Criterion};
use log::error;
use mach_6;
use mach_6::structs::{Element, Selector};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use mach_6::structs::borrowed::{DocumentMatches, ElementMatches, SelectorsOrSharedStyles};

pub fn bench_all_websites(c: &mut Criterion, website_filter: Option<&str>) {
    let websites_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("websites");
    let documents_selectors: Box<dyn Iterator<Item = _>> = match website_filter {
        Some(website) => match mach_6::parse::get_document_and_selectors(&websites_path.join(website)) {
            Ok(Some(document_selectors)) => Box::new(std::iter::once(Ok(document_selectors))),
            Ok(None) => return,
            Err(e) => return error!("{e}"),
        },
        None => match mach_6::get_all_documents_and_selectors(&websites_path) {
            Ok(documents_selectors) => Box::new(documents_selectors),
            Err(e) => return error!("{e}"),
        },
    };
    let mut all_stats: StatsFile = StatsFile::default();
    for res in documents_selectors {
        match res {
            Ok((name, document, selectors)) => {
                let selector_map = mach_6::build_selector_map(&selectors);
                let mut group = c.benchmark_group(&name);

                let naive_matches = mach_6::match_selectors(&document, &selectors);
                group.bench_function("Naive", |b| b.iter(|| {
                    mach_6::match_selectors(&document, &selectors);
                }));

                let (_, selector_map_stats) =
                    mach_6::match_selectors_with_selector_map(&document, &selector_map);
                group.bench_function("With SelectorMap", |b| b.iter(|| {
                    mach_6::match_selectors_with_selector_map(&document, &selector_map);
                }));

                let (_, bloom_filter_stats) =
                    mach_6::match_selectors_with_bloom_filter(&document, &selector_map);
                group.bench_function("With SelectorMap and Bloom Filter", |b| b.iter(|| {
                    mach_6::match_selectors_with_bloom_filter(&document, &selector_map);
                }));

                let (_, style_sharing_stats) =
                    mach_6::match_selectors_with_style_sharing(&document, &selector_map);
                group.bench_function("With SelectorMap, Bloom Filter, and Style Sharing", |b| b.iter(|| {
                    mach_6::match_selectors_with_style_sharing(&document, &selector_map);
                }));

                group.bench_function("Speed of Light", |b| b.iter(|| mach_6::mach_7(&naive_matches)));
                group.finish();

                let counts = counts_from(&naive_matches);

                let mut algorithm_stats = BTreeMap::new();
                algorithm_stats.insert(
                    "Naive".to_string(),
                    Some(StatsEntry::new(
                        selectors.len(),
                        counts,
                        None,
                    )),
                );
                algorithm_stats.insert(
                    "With SelectorMap".to_string(),
                    Some(StatsEntry::new(
                        selectors.len(),
                        counts,
                        Some(&selector_map_stats),
                    )),
                );
                algorithm_stats.insert(
                    "With SelectorMap and Bloom Filter".to_string(),
                    Some(StatsEntry::new(
                        selectors.len(),
                        counts,
                        Some(&bloom_filter_stats),
                    )),
                );
                algorithm_stats.insert(
                    "With SelectorMap and Bloom Filter".to_string(),
                    Some(StatsEntry::new(
                        selectors.len(),
                        counts,
                        Some(&bloom_filter_stats),
                    )),
                );
                algorithm_stats.insert(
                    "With SelectorMap, Bloom Filter, and Style Sharing".to_string(),
                    Some(StatsEntry::new(
                        selectors.len(),
                        counts,
                        Some(&style_sharing_stats),
                    )),
                );
                algorithm_stats.insert(
                    "Speed of Light".to_string(),
                    Some(StatsEntry::new(
                        selectors.len(),
                        counts,
                        None,
                    )),
                );
                all_stats
                    .websites
                    .insert(name, algorithm_stats);
            },
            Err(e) => {
                error!("{e}");
            }
        }
    }

    if let Err(e) = write_stats_json(&all_stats) {
        error!("unable to write stats.json: {e}");
    }
}

fn bench_all_websites_full(c: &mut Criterion) {
    bench_all_websites(c, None);
}

criterion_group!(benches, bench_all_websites_full);

fn main() {
    env_logger::Builder::new().filter_level(log::LevelFilter::Warn).init();
    let website_filter = website_filter_from_args();
    if let Some(filter) = website_filter.as_deref() {
        let mut c = Criterion::default();
        bench_all_websites(&mut c, Some(filter));
        c.final_summary();
    } else {
        benches();
        criterion::Criterion::default()
            .configure_from_args()
            .final_summary();
    }
    if let Err(e) = postprocess_reports() {
        error!("unable to post-process Criterion reports: {e}");
    }
}

fn website_filter_from_args() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 {
        let candidate = args[0].as_str();
        if !candidate.starts_with('-') {
            return Some(candidate.to_string());
        }
    }
    None
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct StatsFile {
    websites: BTreeMap<String, BTreeMap<String, Option<StatsEntry>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatsEntry {
    num_elements: usize,
    num_selectors: usize,
    matching_pairs: usize,
    sharing_instances: Option<usize>,
    selector_map_hits: Option<usize>,
    fast_rejects: Option<usize>,
    slow_rejects: Option<usize>,
    time_spent_slow_rejecting: Option<Duration>,
}

impl StatsEntry {
    fn new(
        num_selectors: usize,
        counts: MatchCounts,
        stats: Option<&selectors::matching::Statistics>,
    ) -> Self {
        Self {
            num_elements: counts.num_elements,
            num_selectors,
            matching_pairs: counts.matching_pairs,
            sharing_instances: stats.and_then(|s| s.sharing_instances),
            selector_map_hits: stats.and_then(|s| s.selector_map_hits),
            fast_rejects: stats.and_then(|s| s.fast_rejects),
            slow_rejects: stats.and_then(|s| s.slow_rejects),
            time_spent_slow_rejecting: stats.and_then(|s| s.time_spent_slow_rejecting),
        }
    }
}

fn write_stats_json(stats: &StatsFile) -> io::Result<()> {
    let criterion_dir = criterion_dir();
    fs::create_dir_all(&criterion_dir)?;
    let stats_path = criterion_dir.join("stats.json");
    let payload = serde_json::to_string_pretty(stats).expect("stats.json serialization failed");
    fs::write(stats_path, payload)
}

fn postprocess_reports() -> io::Result<()> {
    let criterion_dir = criterion_dir();
    let stats_path = criterion_dir.join("stats.json");
    let stats_text = fs::read_to_string(&stats_path)?;
    let stats: StatsFile = serde_json::from_str(&stats_text)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    for (website, algorithms) in stats.websites {
        let website_dir = match find_dir(&criterion_dir, &website) {
            Some(dir) => dir,
            None => continue,
        };

        let group_report = website_dir.join("report/index.html");
        if group_report.exists() {
            let html = fs::read_to_string(&group_report)?;
            let updated = inject_group_report(&html, &website, &algorithms);
            if updated != html {
                fs::write(&group_report, updated)?;
            }
        }

        for (algorithm, stats_entry) in &algorithms {
            let algo_dir = match find_dir(&website_dir, algorithm) {
                Some(dir) => dir,
                None => continue,
            };
            let algo_report = algo_dir.join("report/index.html");
            if algo_report.exists() {
                let html = fs::read_to_string(&algo_report)?;
                let updated = inject_algorithm_report(&html, &website, algorithm, stats_entry.as_ref());
                if updated != html {
                    fs::write(&algo_report, updated)?;
                }
            }
        }
    }

    Ok(())
}

fn criterion_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("criterion")
}

fn find_dir(base: &Path, name: &str) -> Option<PathBuf> {
    let mut candidate = base.join(make_filename_safe(name));
    if candidate.exists() {
        return Some(candidate);
    }
    let lower = make_filename_safe(name).to_lowercase();
    candidate = base.join(lower);
    if candidate.exists() {
        return Some(candidate);
    }
    None
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

fn inject_group_report(
    html: &str,
    website: &str,
    algorithms: &BTreeMap<String, Option<StatsEntry>>,
) -> String {
    let mut updated = html.to_string();
    updated = inject_styles_once(&updated);

    for (algorithm, stats) in algorithms {
        let title = format!("{}/{}", website, algorithm);
        let needle = format!("<h4>{}</h4>", title);
        if let Some(h4_pos) = updated.find(&needle) {
            if let Some(a_close_rel) = updated[h4_pos..].find("</a>") {
                let after_anchor = h4_pos + a_close_rel + "</a>".len();
                if let Some(table_close_rel) = updated[after_anchor..].find("</table>") {
                    let insert_at = after_anchor + table_close_rel + "</table>".len();
                    let stats_html = render_stats_block(stats.as_ref());
                    if !has_stats_block_nearby(&updated, insert_at) {
                        updated.insert_str(insert_at, &stats_html);
                    }
                }
            }
        }
    }
    updated
}

fn inject_algorithm_report(
    html: &str,
    website: &str,
    algorithm: &str,
    stats: Option<&StatsEntry>,
) -> String {
    let mut updated = html.to_string();
    updated = inject_styles_once(&updated);

    let title = format!("{}/{}", website, algorithm);
    let needle = format!("<h2>{}</h2>", title);
    if let Some(h2_pos) = updated.find(&needle) {
        let after_title = h2_pos + needle.len();
        if let Some(plots_start_rel) = updated[after_title..].find("<section class=\"plots\">") {
            let plots_start = after_title + plots_start_rel;
            if let Some(plots_end_rel) = updated[plots_start..].find("</section>") {
                let insert_at = plots_start + plots_end_rel + "</section>".len();
                let stats_html = render_stats_block(stats);
                if !has_stats_block_nearby(&updated, insert_at) {
                    updated.insert_str(insert_at, &stats_html);
                }
            }
        }
    }
    updated
}

fn inject_styles_once(html: &str) -> String {
    if html.contains(".mach6-stats") {
        return html.to_string();
    }
    let style_inject = r#"
        .mach6-stats {
            margin: 12px 0 8px 0;
            padding: 8px 12px;
            border: 1px solid #d0d0d0;
            border-radius: 6px;
            background: #fafafa;
        }
        .mach6-stats h5 {
            margin: 0 0 6px 0;
            font-size: 14px;
            font-weight: 600;
        }
        .mach6-stats table {
            border-collapse: collapse;
        }
        .mach6-stats th {
            text-align: left;
            padding-right: 10px;
            font-weight: 500;
        }
        .mach6-stats td {
            padding-right: 10px;
        }
    "#;
    let needle = "<style type=\"text/css\">";
    if let Some(pos) = html.find(needle) {
        let insert_at = pos + needle.len();
        let mut updated = html.to_string();
        updated.insert_str(insert_at, style_inject);
        return updated;
    }
    html.to_string()
}

fn render_stats_block(stats: Option<&StatsEntry>) -> String {
    match stats {
        Some(stats) => {
            let mut rows = format!(
                r#"
                    <tr><th>Number of Elements</th><td>{}</td></tr>
                    <tr><th>Number of Selectors</th><td>{}</td></tr>
                    <tr><th>Matching Pairs</th><td>{}</td></tr>
"#,
                stats.num_elements, stats.num_selectors, stats.matching_pairs,
            );
            if let Some(selector_map_hits) = stats.selector_map_hits {
                rows.push_str(&format!(
                    r#"                    <tr><th>Selector Map Hits</th><td>{}</td></tr>
"#,
                    selector_map_hits
                ));
            }
            if let Some(fast_rejects) = stats.fast_rejects {
                rows.push_str(&format!(
                    r#"                    <tr><th>Fast Rejects</th><td>{}</td></tr>
"#,
                    fast_rejects
                ));
            }
            if let Some(slow_rejects) = stats.slow_rejects {
                rows.push_str(&format!(
                    r#"                    <tr><th>Slow Rejects</th><td>{}</td></tr>
"#,
                    slow_rejects
                ));
            }
            if let Some(time_spent_slow_rejecting) = stats.time_spent_slow_rejecting {
                rows.push_str(&format!(
                    r#"                    <tr><th>Time Spent Slow Rejecting</th><td>{}</td></tr>
"#,
                    format_duration(time_spent_slow_rejecting)
                ));
            }
            if let Some(sharing_instances) = stats.sharing_instances {
                rows.push_str(&format!(
                    r#"                    <tr><th>Sharing Instances</th><td>{}</td></tr>
"#,
                    sharing_instances
                ));
            }
            format!(
                r#"
        <section class="mach6-stats">
            <h5>Selector Stats</h5>
            <table>
                <tbody>
{}
                </tbody>
            </table>
        </section>
"#,
                rows
            )
        }
        None => r#"
        <section class="mach6-stats">
            <h5>Selector Stats</h5>
            <div>No stats available.</div>
        </section>
"#
        .to_string(),
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.3} ms", duration.as_secs_f64() * 1_000.0)
}

fn has_stats_block_nearby(html: &str, insert_at: usize) -> bool {
    let start = insert_at.saturating_sub(200);
    let end = (insert_at + 200).min(html.len());
    html[start..end].contains("mach6-stats")
}

#[derive(Clone, Copy, Debug)]
struct MatchCounts {
    num_elements: usize,
    matching_pairs: usize,
}

fn counts_from(matches: &DocumentMatches) -> MatchCounts {
    fn find_selectors<'a, 'b>(map: &HashMap<u64, &'b ElementMatches<'a>>, id: u64) -> &'b SmallVec<[&'a Selector; 16]> {
        match &map.get(&id).unwrap().selectors {
            SelectorsOrSharedStyles::Selectors(selectors) => selectors,
            SelectorsOrSharedStyles::SharedWithElement(id) => find_selectors(map, *id),
        }
    }
    let num_elements = matches.0.len();
    let keyed: HashMap<_, _> = matches
        .0
        .iter()
        .map(|em| (Element::from(em.element).id, em))
        .collect();
    debug_assert_eq!(num_elements, keyed.len());
    
    let matching_pairs = keyed
        .keys()
        .map(|id| find_selectors(&keyed, *id).len())
        .sum();
    MatchCounts {
        num_elements,
        matching_pairs,
    }
}
