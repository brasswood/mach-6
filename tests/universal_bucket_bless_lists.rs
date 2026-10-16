use mach_6::structs::{owned::OwnedDocumentMatches, set::SetDocumentMatches};
use mach_6::{match_selectors_with_style_sharing, MatchingContext, Optimizations};
use scraper::Html;
use selectors::matching::Statistics;
use style::context::QuirksMode;
use style::media_queries::MediaList;
use style::servo_arc::Arc;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet, UrlExtraData};

fn run(html: &str, selectors: &[&str], enabled: bool) -> (SetDocumentMatches, Statistics) {
    // Exercise the public whole-document traversal, including its fragment context element.
    let document = Html::parse_fragment(html);
    let stylesheet_lock = SharedRwLock::new();
    let css = selectors
        .iter()
        .map(|selector| format!("{selector} {{}}"))
        .collect::<Vec<_>>()
        .join("\n");
    let stylesheet = DocumentStyleSheet(Arc::new(Stylesheet::from_str_with_has(
        &css,
        UrlExtraData::from(url::Url::parse("about:blank").unwrap()),
        Origin::Author,
        Arc::new(stylesheet_lock.wrap(MediaList::empty())),
        stylesheet_lock.clone(),
        None,
        None,
        QuirksMode::NoQuirks,
        AllowImportRules::No,
        true,
    )));
    let optimizations = Optimizations {
        universal_bucket_bless_lists: enabled,
        ..Optimizations::from_none()
    };
    let context =
        MatchingContext::new(std::iter::once(&stylesheet), stylesheet_lock, optimizations);
    let (matches, stats, _, _) = match_selectors_with_style_sharing(document, context, false);
    (
        SetDocumentMatches::from(matches),
        stats,
    )
}

#[test]
fn universal_tail_uses_the_parent_activator() {
    let html = "<div class='a'><div class='b'><div class='c'></div></div></div>";
    let (before, before_stats) = run(html, &[".b .a *"], false);
    let (after, after_stats) = run(html, &[".b .a *"], true);
    assert_eq!(before, after);
    assert_eq!(before_stats.counts.slow_rejects, 1);
    assert_eq!(before_stats.counts.selector_map_hits, 4);
    assert_eq!(after_stats.counts.slow_rejects, 0);
    assert_eq!(after_stats.counts.selector_map_hits, 1);
    assert_eq!(after_stats.counts.bless_list_hits, 0);
}

#[test]
fn other_universal_bucket_selectors_use_the_parent_activator() {
    let mut html = String::from("<div class='a1 a2 a3 a4 a5 a6'><div class='b'>");
    for index in 1..=5 {
        html.push_str(&format!("<div class='c{index}'></div>"));
    }
    html.push_str("</div></div>");
    let selectors = [
        ".b .a1 *",
        ".b .a2 :not(.d)",
        ".b .a3 :has(.d)",
        ".b .a4 :empty",
        ".b .a5 :nth-child(even)",
        ".b .a6 :nth-last-child(odd)",
    ];
    let (before, before_stats) = run(&html, &selectors, false);
    let (after, after_stats) = run(&html, &selectors, true);
    assert_eq!(before, after);
    assert_eq!(before_stats.counts.selector_map_hits, 48);
    assert_eq!(before_stats.counts.slow_rejects, 30);
    assert_eq!(after_stats.counts.selector_map_hits, 6);
    assert_eq!(after_stats.counts.slow_rejects, 0);
    assert_eq!(after_stats.counts.bless_list_hits, 0);
}

#[test]
fn universal_bless_lists_persist_through_descendants() {
    let html = "<div class='a'><div class='b'><div class='c'><div class='d'></div></div><div class='e'><div class='f'></div></div></div></div>";
    let selectors = [
        ".a *",
        ".a .b :not(.d)",
        ".a .b .c :has(.d)",
        ".a .b .e :nth-child(even)",
    ];
    let (before, before_stats) = run(html, &selectors, false);
    let (after, after_stats) = run(html, &selectors, true);
    assert_eq!(before, after);
    assert_eq!(before_stats.counts.selector_map_hits, 28);
    assert_eq!(
        before_stats.counts.slow_rejects + before_stats.counts.fast_rejects,
        20
    );
    assert_eq!(after_stats.counts.selector_map_hits, 4);
    assert_eq!(after_stats.counts.bless_list_hits, 11);
    assert_eq!(
        after_stats.counts.slow_rejects + after_stats.counts.fast_rejects,
        3
    );
}

#[test]
fn child_activations_do_not_persist_beyond_immediate_children() {
    let html =
        "<div class='a'><div class='b'></div><div class='a'><div class='d'></div></div></div>";
    let (before, before_stats) = run(html, &[".a > :not(.x)"], false);
    let (after, after_stats) = run(html, &[".a > :not(.x)"], true);
    assert_eq!(before, after);
    assert_eq!(before_stats.counts.selector_map_hits, 5);
    assert_eq!(before_stats.counts.slow_accepts, 3);
    assert_eq!(after_stats.counts.selector_map_hits, 2);
    assert_eq!(after_stats.counts.bless_list_hits, 3);
    assert_eq!(after_stats.counts.slow_accepts, 3);
}

#[test]
fn universal_bless_lists_do_not_duplicate_matching_selectors() {
    let html = "<div class='a'><div class='a'><div class='a'><div class='a'><div class='a'></div></div></div></div></div>";
    let (before, before_stats) = run(html, &[".a *"], false);
    let (after, after_stats) = run(html, &[".a *"], true);
    assert_eq!(before, after);
    assert_eq!(before_stats.counts.selector_map_hits, 6);
    assert_eq!(before_stats.counts.slow_accepts, 4);
    assert_eq!(after_stats.counts.selector_map_hits, 1);
    assert_eq!(after_stats.counts.bless_list_hits, 4);
    assert_eq!(after_stats.counts.slow_accepts, 4);
}

#[test]
fn universal_bless_lists_scale_to_many_selectors() {
    let mut html = String::new();
    let mut selectors = Vec::with_capacity(1000);
    for index in 1..=1000 {
        html.push_str(&format!("<div class='n{index}'>"));
        selectors.push(format!(".n{index} *"));
    }
    for _ in 0..1000 {
        html.push_str("</div>");
    }
    std::thread::Builder::new()
        .name("deep-universal-bucket-bless-lists".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let selector_refs = selectors.iter().map(String::as_str).collect::<Vec<_>>();
            let before_stats = run(&html, &selector_refs, false).1;
            assert_eq!(before_stats.counts.selector_map_hits, 1_001_000);
            assert_eq!(before_stats.counts.slow_accepts, 499_500);
            let after_stats = run(&html, &selector_refs, true).1;
            assert_eq!(after_stats.counts.selector_map_hits, 1000);
            assert_eq!(after_stats.counts.bless_list_hits, 499_500);
            assert_eq!(after_stats.counts.slow_accepts, 499_500);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn universal_bucket_bless_lists_follow_selector_map_bucket_classification() {
    let html =
        "<div class='a'><div class='b'></div><div class='c'></div><div class='d'></div></div>";
    let (before, before_stats) = run(html, &[".a :is(.b, .c)"], false);
    let (after, after_stats) = run(html, &[".a :is(.b, .c)"], true);
    assert_eq!(before, after);
    assert_eq!(before_stats.counts.selector_map_hits, 2);
    assert_eq!(after_stats.counts.selector_map_hits, 2);
    assert_eq!(after_stats.counts.bless_list_hits, 0);

    let (before, before_stats) = run(html, &[".a :is(*, .b)"], false);
    let (after, after_stats) = run(html, &[".a :is(*, .b)"], true);
    assert_eq!(before, after);
    assert_eq!(before_stats.counts.selector_map_hits, 5);
    assert_eq!(after_stats.counts.selector_map_hits, 1);
    assert_eq!(after_stats.counts.bless_list_hits, 3);
}
