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
        fail_caches,
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
        prefix_interning_calls: timings.prefix_interning_calls,
    }
}

fn assert_parity(html: &str, selectors: &[String]) -> Run {
    let cached = run(html, selectors, true, true);
    let uncached = run(html, selectors, false, false);
    assert_eq!(cached.matches, uncached.matches);
    cached
}

fn strings(selectors: &[&str]) -> Vec<String> {
    selectors.iter().map(|selector| (*selector).to_owned()).collect()
}

fn numbered_html(depth: usize, leaf_classes: &str, add_bloom_class: bool) -> String {
    let mut html = String::new();
    for number in 1..=depth {
        let bloom_class = if add_bloom_class && number == 1 { " a" } else { "" };
        write!(&mut html, "<div class='n{number}{bloom_class}'>").unwrap();
    }
    write!(&mut html, "<div class='{leaf_classes}'></div>").unwrap();
    for _ in 0..depth {
        html.push_str("</div>");
    }
    html
}

fn numbered_selectors(depth: usize, suffix: &str) -> Vec<String> {
    (1..=depth)
        .map(|number| format!(".n{number} .a .{suffix}"))
        .collect()
}

#[test]
fn fail_cache_works_cases_1_and_2() {
    let selectors = strings(&[".a.b .c", ".a.b .c .c"]);
    let case1 = assert_parity(
        "<div class='a'><div class='b'><div class='c'></div></div></div>",
        &selectors,
    );
    assert_eq!(case1.stats.counts.slow_rejects, 1);
    assert_eq!(case1.stats.counts.fast_rejects, 1);
    assert_eq!(case1.stats.counts.fail_cache_rejects, 0);

    let case2 = assert_parity(
        "<div class='a'><div class='b'><div class='c 1'></div><div class='c 2'></div></div></div>",
        &selectors,
    );
    assert_eq!(case2.stats.counts.slow_rejects, 2);
    assert_eq!(case2.stats.counts.fast_rejects, 2);
    assert_eq!(case2.stats.counts.fail_cache_rejects, 0);
}

#[test]
fn fail_cache_works_cases_3_and_4() {
    let case3 = assert_parity(
        "<div class='a'><div class='b'><div class='c d'></div></div></div>",
        &strings(&[".a.b .a .c", ".a.b .a .d"]),
    );
    assert_eq!(case3.stats.counts.slow_rejects, 1);
    assert_eq!(case3.stats.counts.fail_cache_rejects, 1);

    let case4 = assert_parity(
        "<div class='a'><div class='b'><div class='c'><div class='d'></div></div></div></div>",
        &strings(&[".b .a .c .d", ".b .a .d"]),
    );
    assert_eq!(case4.stats.counts.slow_rejects, 1);
    assert_eq!(case4.stats.counts.fail_cache_rejects, 1);
}

#[test]
fn fail_cache_hit_can_seed_another_element_cache() {
    let run = assert_parity(
        "<div class='a'><div class='b'><div class='c'><div class='d'></div><div class='e'><div class='f'></div></div></div></div></div>",
        &strings(&[".b .a .c .d", ".b .a .e .f", ".b .a .d", ".b .a .f"]),
    );
    assert_eq!(run.stats.counts.slow_rejects, 1);
    assert_eq!(run.stats.counts.fail_cache_rejects, 3);
    assert!(run.cache_entries >= 3);
}

#[test]
fn fill_counters_only_measure_bloom_positive_slow_failures() {
    for depth in [17, 15] {
        let selectors = ["b", "c"]
            .into_iter()
            .flat_map(|suffix| numbered_selectors(depth, suffix))
            .collect::<Vec<_>>();
        let original = assert_parity(&numbered_html(depth, "b c", false), &selectors);
        assert_eq!(original.stats.counts.slow_rejects, 0);
        assert_eq!(original.stats.counts.fast_rejects, depth * 2);
        assert_eq!(original.stats.counts.fail_cache_rejects, 0);
        assert_eq!(original.filled_caches, 0);

        let bloom_positive = assert_parity(&numbered_html(depth, "b c", true), &selectors);
        assert_eq!(bloom_positive.stats.counts.fast_rejects, 0);
        assert_eq!(bloom_positive.stats.counts.slow_rejects, depth * 2);
        assert_eq!(bloom_positive.stats.counts.fail_cache_rejects, 0);
        #[cfg(feature = "measure_fail_cache_fill")]
        assert_eq!(bloom_positive.filled_caches, depth - 1);
    }
}

fn interning_selectors(depth: usize) -> Vec<String> {
    (1..=depth)
        .map(|number| format!(".b .a .n{number} .c"))
        .collect()
}

fn compare_interning_time(
    name: &str,
    small: &str,
    large: &str,
    selectors: &[String],
) {
    let _ = run(small, selectors, true, false);
    let small = run(small, selectors, true, true);
    let large = run(large, selectors, true, true);
    assert!(small.prefix_interning_calls > 0, "{name} must exercise prefix interning");
    assert_eq!(small.prefix_interning_calls, large.prefix_interning_calls, "{name}");
    #[cfg(feature = "measure_fail_cache_fill")]
    assert_eq!(small.filled_caches, 0, "{name} small fixture unexpectedly filled a cache");
    assert_eq!(large.filled_caches, 0, "{name} large fixture unexpectedly filled a cache");
    let difference = i128::from(large.prefix_interning_cycles)
        - i128::from(small.prefix_interning_cycles);
    eprintln!(
        "{name}: small={} cycles, large={} cycles, difference={difference} cycles, calls={}",
        small.prefix_interning_cycles, large.prefix_interning_cycles,
        small.prefix_interning_calls,
    );
}

fn numeric_classes(depth: usize) -> String {
    (1..=depth).map(|n| format!("n{n}")).collect::<Vec<_>>().join(" ")
}

#[test]
fn prefix_interning_is_independent_of_html_size() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(prefix_interning_is_independent_of_html_size_inner)
        .unwrap()
        .join()
        .unwrap();
}

fn prefix_interning_is_independent_of_html_size_inner() {
    let selector = strings(&[".b .a .c"]);
    let global_small = "<div class='a'><div class='b'><div class='n1'><div class='c'></div></div></div></div>";
    let global_large = format!(
        "<div class='a'><div class='b'>{}</div></div>",
        (1..=1000).map(|n| format!("<div class='n{n}'><div class='c'></div></div>")).collect::<String>(),
    );
    compare_interning_time("global", global_small, &global_large, &selector);

    let local_small = "<div class='a'><div class='b'><div class='c'></div></div></div>";
    let mut nested = String::new();
    for n in 1..=1000 { write!(&mut nested, "<div class='c n{n}'>").unwrap(); }
    nested.push_str("<i></i>");
    for _ in 1..=1000 { nested.push_str("</div>"); }
    let local_large = format!("<div class='a'><div class='b'>{nested}</div></div>");
    compare_interning_time("local", local_small, &local_large, &selector);
}
