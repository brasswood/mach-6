use mach_6::structs::{owned::OwnedDocumentMatches, set::SetDocumentMatches};
use mach_6::{match_selectors_with_style_sharing, MatchingContext, Optimizations};
use scraper::Html;
use selectors::matching::Statistics;
use style::context::QuirksMode;
use style::media_queries::MediaList;
use style::servo_arc::Arc;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet, UrlExtraData};
