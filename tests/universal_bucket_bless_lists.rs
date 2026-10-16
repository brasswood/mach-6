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
