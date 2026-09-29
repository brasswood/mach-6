use mach_6::parse::{ParsedWebsite, get_document_and_selectors};
use mach_6::structs::{owned::OwnedDocumentMatches, set::SetDocumentMatches};
use mach_6::{Optimizations, match_selectors_with_style_sharing};
use scraper::ElementRef;
use selectors::matching::Statistics;
use std::fmt::Write as _;

struct Run {
    matches: SetDocumentMatches,
    stats: Statistics,
    filled_caches: usize,
    cache_entries: usize,
    prefix_interning_cycles: u64,
    prefix_interning_calls: u64,
}

fn website(html: &str, selectors: &[String]) -> (tempfile::TempDir, ParsedWebsite) {
    let directory = tempfile::tempdir().unwrap();
    let mut contents = String::from("<html><head><style>");
    for selector in selectors {
        writeln!(&mut contents, "{selector} {{ color: red; }}").unwrap();
    }
    contents.push_str("</style></head><body>");
    contents.push_str(html);
    contents.push_str("</body></html>");
    std::fs::write(directory.path().join("case.html"), contents).unwrap();
    let parsed = get_document_and_selectors(directory.path())
        .unwrap()
        .expect("test website should contain HTML");
    (directory, parsed)
}

fn run(html: &str, selectors: &[String], fail_caches: bool, inspect_caches: bool) -> Run {
    let (_directory, parsed) = website(html, selectors);
    let matcher = parsed.get_matcher(Optimizations {
        selector_map: true,
        bloom_filter: true,
        fail_caches,
        lazy_fail_cache_prefixes: fail_caches,
        ..Optimizations::default()
    });
    let (matches, stats) = match_selectors_with_style_sharing(
        parsed.document(),
        &matcher, None,
    );
    let (filled_caches, cache_entries) = if inspect_caches && fail_caches {
        let max_id = if selectors.len() <= 4 {
            u16::try_from(selectors.len() * 3).unwrap()
        } else {
            0
        };
        parsed.document().tree.nodes().filter_map(ElementRef::wrap).fold(
            (0, 0),
            |(filled, entries), element| {
                let cache = element.value().borrow_data().fail_cache;
                let entries = entries + (1..=max_id)
                    .filter(|id| cache.contains(*id))
                    .count();
                (filled + usize::from(cache.filled_once()), entries)
            },
        )
    } else {
        (0, 0)
    };
    let timings = matcher.stylist().fail_cache_build_timings();
    Run {
        matches: SetDocumentMatches::from(OwnedDocumentMatches::from(&matches)),
        stats,
        filled_caches,
        cache_entries,
        prefix_interning_cycles: timings.prefix_interning.cycles(),
