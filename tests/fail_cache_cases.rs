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
