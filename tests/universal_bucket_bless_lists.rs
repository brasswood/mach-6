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
    let (matches, stats) = match_selectors_with_style_sharing(&document, &context, None);
    (
        SetDocumentMatches::from(OwnedDocumentMatches::from(&matches)),
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
