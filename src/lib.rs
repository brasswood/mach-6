/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use clap::ValueEnum;
use ::cssparser::ToCss as _;
use derive_more::Display;
use rustc_hash::FxBuildHasher;
use selectors::matching::SelectorStats;
use style::animation::DocumentAnimationSet;
use style::context::SharedStyleContext;
use style::context::StyleSystemOptions;
use style::context::ThreadLocalStyleContext;
#[cfg(feature = "debug_element")]
use style::selector_map::debug_element_selector;
use style::selector_parser::SnapshotMap;
use style::shared_lock::{SharedRwLock, StylesheetGuards};
use style::sharing::StyleSharingElement as _;
use style::stylesheets::DocumentStyleSheet;
use style::stylesheets::UrlExtraData;
use style::stylist::FailCacheBuildTimings;
use style::stylist::Stylist;
use style::traversal_flags::TraversalFlags;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;
use scraper::ElementRef;
use scraper::Html;
use selectors::context::SelectorCaches;
use selectors::matching::{self, Statistics};
use style::context::StyleContext;
use style::rule_tree::CascadeLevel;
use style::selector_map::SelectorMapElement as _;
use style::selector_map::SelectorMap;
use style::servo_arc::Arc;
use style::stylist::CascadeData;
use style::stylist::Rule;
use style::sharing::StyleSharingTarget;
use style::stylesheets::Origin;
use style::thread_state::{self, ThreadState};
use smallvec::SmallVec;
use tsc_timer::Start;

mod stylo_interface;
pub mod parse;
pub mod preprocessing;
pub mod result;
pub mod structs;

pub use parse::get_all_documents_and_selectors;
use crate::parse::ParsedWebsite;
use crate::result::Result;
use crate::structs::owned::OwnedElementMatches;
use crate::structs::owned::OwnedSelectorsOrSharedStyles;
use crate::structs::{
    Element, Selector,
    borrowed::{
        DocumentMatches,
        ElementMatches,
        SelectorsOrSharedStyles,
    },
    owned::OwnedDocumentMatches,
    set::SetDocumentMatches,
};

#[derive(Debug, Display, Clone, Copy, ValueEnum)]
pub enum Algorithm {
    Naive,
    WithStyleSharing,
    WithFailCaches,
    WithIsConversion,
    WithDistribution,
    Mach7,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Optimizations {
    pub is_conversion: bool,
    pub distribution: bool,
    pub fail_caches: bool,
}

impl Optimizations {
    pub fn from_none() -> Self {
        Self {
            ..Default::default()
        }
    }
}

struct PreparedSelectors<'selector> {
    selectors: Vec<Selector>,
    reverse_map: HashMap<String, SmallVec<[&'selector Selector; 2]>>,
    // consider the case that author wrote both `:is(.a, .b)` and `.a`.
    // Two original selectors would be in the reverse map due to distribution.
    // Likewise consider if author wrote both `[class*=-top]` and `.a-top`.
    // Two original selectors would be in the reverse map for :is conversion.
    // Reversing to two original selectors will be correct, because if an
    // element matched one selector, it must have also matched the other.
}

fn prepare_selectors<'selector>(
    document: &Html,
    selectors: &'selector [Selector],
    optimizations: Optimizations,
) -> PreparedSelectors<'selector> {
    let pre_distribution = if optimizations.is_conversion {
        preprocessing::concretize::convert_to_is_selectors(document, selectors)
    } else {
        selectors.to_vec()
    };
    let mut pre_distribution_to_originals: HashMap<String, SmallVec<[&'selector Selector; 2]>> =
        HashMap::with_capacity(pre_distribution.len());
    if optimizations.is_conversion {
        for (preprocessed, original) in pre_distribution.iter().zip(selectors.iter()) {
            pre_distribution_to_originals
                .entry(preprocessed.to_css_string())
                .or_default()
                .push(original);
        }
    } else {
        // TODO: this is shit, should just have an enum variant denoting noop, but whatever
        for selector in selectors {
            pre_distribution_to_originals.insert(
               selector.to_css_string(),
               smallvec::smallvec![selector],
            );
        }
    }

    if optimizations.distribution {
        let mut distributed = Vec::with_capacity(pre_distribution.len());
        let mut reverse_map: HashMap<String, SmallVec<[&'selector Selector; 2]>> =
            HashMap::with_capacity(pre_distribution.len()); // many distributed |-> one original
        for selector in &pre_distribution {
            // TODO: maybe could avoid an extra hashmap lookup when both options are enabled. But this does make the control flow nicer.
            let original_selectors = pre_distribution_to_originals
                .get(&selector.to_css_string())
                .unwrap_or_else(|| {
                    panic!(
                        "failed to find original selector for preprocessing input {}",
                        selector.to_css_string(),
                    )
                });
            for distributed_selector in preprocessing::distribute::DistributedSelectors::from_selector(selector) {
                let distributed_css = distributed_selector.to_css_string();
                reverse_map
                    .entry(distributed_css)
                    .or_default()
                    .extend(original_selectors.iter().copied());
                distributed.push(distributed_selector);
            }
        }
        PreparedSelectors {
            selectors: distributed,
            reverse_map,
        }
    } else {
        PreparedSelectors {
            selectors: pre_distribution,
            reverse_map: pre_distribution_to_originals,
        }
    }
}

fn translate_element_matches_to_original<'new, 'original>(
    element_matches: &ElementMatches<'new>,
    reverse_map: &HashMap<String, SmallVec<[&'original Selector; 2]>>,
) -> OwnedElementMatches {
    let selectors_or_shared_styles = match &element_matches.selectors {
        SelectorsOrSharedStyles::Selectors(selectors) => {
            let mut set: HashSet<by_address::ByAddress<&Selector>> = HashSet::new();
            for selector in selectors {
                let original_selectors = reverse_map
                    .get(&selector.to_css_string())
                    .unwrap_or_else(|| {
                        panic!(
                            "failed to find original selector for preprocessed selector {}",
                            selector.to_css_string(),
                        )
                    });
                for original_selector in original_selectors {
                    set.insert(by_address::ByAddress(*original_selector));
                }
            }
            let selectors = set
                .into_iter()
                .map(|addr|
                    (*addr).clone()
                )
                .collect();
            OwnedSelectorsOrSharedStyles::Selectors(selectors)
        }
        SelectorsOrSharedStyles::SharedWithElement(id) => {
            OwnedSelectorsOrSharedStyles::SharedWithElement(*id)
        }
    };
    OwnedElementMatches {
        element: element_matches.element.into(),
        selectors: selectors_or_shared_styles,
    }
}

fn do_website_with_configured_optimizations(
    website: &ParsedWebsite,
    optimizations: Optimizations,
) -> (OwnedDocumentMatches, Statistics) {
    // must return OwnedDocumentMatches, because the list of input selectors will be owned by this function
    let document = website.document();
    let selectors = website.get_matcher().get_selectors();
    let prepared = prepare_selectors(document, &selectors, optimizations);
    let (stylesheet, stylesheet_lock) = stylesheet_from_selectors(prepared.selectors.iter());
    let matching_context = MatchingContext::new(
        std::iter::once(&stylesheet),
        stylesheet_lock,
        optimizations.fail_caches,
    );
    let (matches, stats) = match_selectors_with_style_sharing(
        document,
        &matching_context,
        optimizations,
        None,
    );
    let owned = OwnedDocumentMatches(
        matches
            .0
            .iter()
            .map(|element_matches| {
                translate_element_matches_to_original(element_matches, &prepared.reverse_map)
            })
            .collect(),
    );
    (owned, stats)
}


pub struct MatchingContext {
    stylesheet_lock: SharedRwLock,
    stylist: Stylist,
}

impl MatchingContext {
    pub fn new<'a>(
        stylesheets: impl Iterator<Item = &'a DocumentStyleSheet>,
        stylesheet_lock: SharedRwLock,
        build_fail_cache_entries: bool,
    ) -> Self {
        let mut stylist = Stylist::new(
            stylo_interface::mock_device(),
            selectors::matching::QuirksMode::NoQuirks,
            false,
            build_fail_cache_entries,
        );
        for sheet in stylesheets {
            stylist.append_stylesheet(sheet.clone(), &stylesheet_lock.read());
        }
        let ua_or_user_lock = SharedRwLock::new();
        let ua_or_user_guard = ua_or_user_lock.read();
        stylist.flush_without_invalidation(&StylesheetGuards {
            author: &stylesheet_lock.read(),
            ua_or_user: &ua_or_user_guard,
        });
        Self {
            stylesheet_lock,
            stylist,
        }
    }

    pub fn stylesheet_lock(&self) -> &SharedRwLock {
        &self.stylesheet_lock
    }

    pub fn stylist(&self) -> &Stylist {
        &self.stylist
    }

    pub fn fail_cache_build_timings(&self) -> FailCacheBuildTimings {
        self.stylist.fail_cache_build_timings()
    }

    pub fn get_selectors(&self) -> Vec<Selector> {
        let mut selectors = BTreeMap::new();
        let cascade_data = self.stylist.cascade_data().borrow_for_origin(Origin::Author);
        if let Some(map) = cascade_data.normal_rules(&[]) {
            collect_selectors_from_map(map, &mut selectors);
        }
        selectors.into_values().collect()
    }
}

fn element_to_string(el: ElementRef<'_>) -> String {
    let name = el.value().name();
    let mut out = String::new();
    write!(&mut out, "<{name}").unwrap();
    for (k, v) in el.value().attrs() {
        write!(&mut out, " {k}=\"{v}\"").unwrap();
    }
    out.push('>');
    out
} // thanks, ChatGPT

fn assert_childrens_parent_is_me(parent: &ElementRef) {
    // assert that all of my children's parent is me
    for child in parent.child_elements() {
        if child.traversal_parent().unwrap() != *parent {
            let mut msg = String::new();
            writeln!(&mut msg, "me: {:?}", parent).unwrap();
            writeln!(&mut msg, "my child: {:?}", child).unwrap();
            writeln!(&mut msg, "my child's traversal_parent: {:?}", child.traversal_parent().unwrap()).unwrap();
            panic!("child's traversal_parent was not equal to me!\n{msg}");
        }
    }
}

pub fn do_all_websites(websites: &Path, algorithm: Algorithm) -> Result<impl Iterator<Item = Result<(String, SetDocumentMatches, Statistics)>>> {
    Ok(get_all_documents_and_selectors(websites)?
        .map(move |r| {
            r.map(|w| do_website(&w, algorithm, None))
        })
    )
}

pub fn do_website(website: &ParsedWebsite, algorithm: Algorithm, mach7_oracle: Option<&DocumentMatches>) -> (String, SetDocumentMatches, Statistics){
    let matching_context = website.get_matcher();
    let (matches, stats) = match algorithm {
        Algorithm::Naive => (
            OwnedDocumentMatches::from(&match_selectors(&website.document(), &matching_context.get_selectors())),
            Statistics::default()
        ),
        Algorithm::WithStyleSharing => {
            let (matches, stats) =
                match_selectors_with_style_sharing(
                    &website.document(),
                    &matching_context,
                    Optimizations::from_none(),
                    None,
                );
            (OwnedDocumentMatches::from(&matches), stats)
        },
        Algorithm::WithFailCaches => {
            let (matches, stats) =
                match_selectors_with_style_sharing(
                    &website.document(),
                    &matching_context,
                    Optimizations {
                        fail_caches: true,
                        ..Optimizations::from_none()
                    },
                    None,
                );
            (OwnedDocumentMatches::from(&matches), stats)
        },
        Algorithm::WithIsConversion =>
            do_website_with_configured_optimizations(
                website,
                Optimizations {
                    is_conversion: true,
                    distribution: false,
                    ..Optimizations::from_none()
                },
            ),
        Algorithm::WithDistribution =>
            do_website_with_configured_optimizations(
                website,
                Optimizations {
                    is_conversion: true,
                    distribution: true,
                    ..Optimizations::from_none()
                },
            ),
        Algorithm::Mach7 => {
            if let Some(document_matches) = mach7_oracle {
                (
                    OwnedDocumentMatches::from(&mach_7(document_matches)),
                    Statistics::default()
                )
            } else {
                let selectors = matching_context.get_selectors();
                let document_matches = match_selectors(&website.document(), &selectors);
                (
                    OwnedDocumentMatches::from(&mach_7(&document_matches)),
                    Statistics::default()
                )
            }
        },
    };
    (website.name.clone(), matches.into(), stats)
}
// TODO: figure out why iteration yields more elements than traversal
pub fn match_selectors<'a>(document: &'a Html, selectors: &'a [Selector]) -> DocumentMatches<'a>
{
    fn preorder_traversal<'a>(
        element: ElementRef<'a>, 
        selectors: &'a [Selector],
        matches: &mut Vec<ElementMatches<'a>>,
        caches: &mut SelectorCaches,
    ) {
        // 1. do thing
        // 1.1: create a MatchingContext
        let mut context = matching::MatchingContext::new(
            matching::MatchingMode::Normal,
            None,
            caches,
            matching::QuirksMode::NoQuirks,
            matching::NeedsSelectorFlags::No,
            matching::MatchingForInvalidation::No,
        );
        // 1.2: get matching selectors naively
        let matched_selectors = selectors
            .iter()
            .filter(|s| {
                // Debug element if applicable
                #[cfg(feature = "debug_element")]
                debug_element_selector(element, &element_to_string(element), s);
                let (res, stats) = matching::matches_selector(s, 0, None, &element, &mut context);
                debug_assert_eq!(stats.time_fast_rejecting, None);
                res
            })
            .collect();
        matches.push(ElementMatches{ element, selectors: SelectorsOrSharedStyles::Selectors(matched_selectors) });
        // 2. traverse children
        for child in element.child_elements() {
            preorder_traversal(child, selectors, matches, caches);
        }
    }
    let mut caches: SelectorCaches = Default::default();
    let mut result = Vec::new();
    preorder_traversal(document.root_element(), selectors, &mut result, &mut caches);
    DocumentMatches(result)
}

pub fn stylesheet_from_selectors<'sel>(
    selectors: impl Iterator<Item = &'sel Selector>,
 ) -> (DocumentStyleSheet, SharedRwLock) {
    let stylesheet_lock = SharedRwLock::new();
    let css = selectors
        .map(|selector| format!("{} {{}}", selector.to_css_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let stylesheet = parse::parse_stylesheet(
        &css,
        UrlExtraData::from(url::Url::parse("about:blank").unwrap()),
        &stylesheet_lock,
    )
    .expect("synthetic selector stylesheet should parse");
    (stylesheet, stylesheet_lock)
}

fn collect_selectors_from_map(
    map: &SelectorMap<Rule>,
    out: &mut BTreeMap<(u32, String), Selector>,
) {
    let mut push_rule = |rule: &Rule| {
        out.entry((rule.source_order, rule.selector.to_css_string()))
            .or_insert_with(|| rule.selector.clone());
    };

    for rule in &map.root {
        push_rule(rule);
    }
    for rule in &map.common_pseudo_classes {
        push_rule(rule);
    }
    for rule in &map.rare_pseudo_classes {
        push_rule(rule);
    }
    for rule in &map.other {
        push_rule(rule);
    }
    for (_, bucket) in map.id_hash.iter() {
        for rule in bucket {
            push_rule(rule);
        }
    }
    for (_, bucket) in map.class_hash.iter() {
        for rule in bucket {
            push_rule(rule);
        }
    }
    for bucket in map.attribute_hash.values() {
        for rule in bucket {
            push_rule(rule);
        }
    }
    for bucket in map.local_name_hash.values() {
        for rule in bucket {
            push_rule(rule);
        }
    }
    for bucket in map.namespace_hash.values() {
        for rule in bucket {
            push_rule(rule);
        }
    }
}

pub fn match_selectors_with_style_sharing<'document>(
    document: &'document Html,
    matching_context: &'document MatchingContext,
    _optimizations: Optimizations,
    selector_stats: Option<&mut SmallVec<[(&'document Selector, SelectorStats); 16]>>,
) -> (DocumentMatches<'document>, Statistics) {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>,
        element_depth: usize,
        context: &mut StyleContext<ElementRef<'a>>,
        matches: &mut Vec<ElementMatches<'a>>,
        mut selector_stats: Option<&mut SmallVec<[(&'a Selector, SelectorStats); 16]>>,
        selector_map: &'a SelectorMap<Rule>,
        cascade_data: &CascadeData,
        optimizations: Optimizations,
        stats: &mut Statistics,
    ) {
        // 0. debug element if applicable
        let debug_html_str: Option<String> = None;
        #[cfg(feature = "debug_element")]
        let debug_html_str = Some(element_to_string(element));
        // 1. do thing
        // 1.1: Set thread state to layout (needed to avoid debug_assert panic)
        thread_state::initialize(ThreadState::LAYOUT);
        // 1.2: update the bloom filter with the current element
        let start = tsc_timer::Start::now();
        context.thread_local.bloom_filter.insert_parents_recovering(element, element_depth);
        stats.times.updating_bloom_filter += start.elapsed();
        // 1.3: Check if we can share styles
        let mut target = StyleSharingTarget::new(element);
        let start = Start::now();
        let style_sharing_result = target.share_style_if_possible(context);
        stats.times.checking_style_sharing += start.elapsed();
        match style_sharing_result {
            Some((other_element, shared_styles)) => {
                // If we can share styles, do that.
                // First, update the data with the new styles.
                // My first version of this passed the `&element` and the
                // `&element.mutate_data()` as two separate parameters to
                // `preorder_traversal`, so that it would look like the
                // signature of `compute_style`. However, this led to the
                // mutable `RefCell` borrow of the element being held on past
                // the recursive call to `preorder_traversal`, which would then
                // try to immutably borrow its parent (the original element)
                // when testing candidate, leading to a panic. The solution
                // would be to drop the mutable borrow before the recursive call
                // to `preorder_traversal`, but if the mutable reference is just
                // a dumb reference, this does nothing (as Thalia explained,
                // once you're in the function `preorder_traversal(&elt, &mut
                // elt.data)`, there is an implicit `RefMut` owned by the caller
                // which &mut elt.data borrows from, so there's no way to drop
                // that). Thalia said the better solution is to just have
                // `preorder_traversal` own the `RefMut` in such a way that it
                // can drop it before recursing.
                // As for how servo gets away with doing it the way they do, it
                // looks like their `process_preorder` function doesn't recurse,
                // which means the `RefMut` can be dropped before
                // `process_preorder` (and all of its callees like
                // `recalc_style_at` and eventually `compute_style`) are called
                // again. As Thalia said, they are probably using some sort of
                // tree-walking machinery.
                element.mutate_data().unwrap().set_styles(shared_styles);
                let other_element = Element::from(other_element);
                matches.push(ElementMatches{ element, selectors: SelectorsOrSharedStyles::SharedWithElement(other_element.id) });
                stats.counts.sharing_instances += 1;
            },
            None => {
                // If we can't share styles, go through the selector map and bloom filter.
                // 1.3.1: create a MatchingContext (after updating style_bloom to avoid borrow check error)
                let mut matching_context = matching::MatchingContext::new(
                    matching::MatchingMode::Normal,
                    Some(context.thread_local.bloom_filter.filter()),
                    &mut context.thread_local.selector_caches,
                    matching::QuirksMode::NoQuirks,
                    matching::NeedsSelectorFlags::No,
                    matching::MatchingForInvalidation::No,
                );
                matching_context.set_use_fail_caches(optimizations.fail_caches);
                // 1.3.2: Use the selector map to get matching rules
                let mut matched_selectors = SmallVec::new();
                let mut sel_stats = selector_stats.is_some().then(SmallVec::new);
                *stats += selector_map.get_all_matching_rules(
                    element,
                    element, // TODO: ????
                    &mut SmallVec::new(),
                    Some(&mut matched_selectors),
                    sel_stats.as_mut(),
                    &mut matching_context,
                    CascadeLevel::same_tree_author_normal(),
                    cascade_data,
                    context.shared.stylist,
                    debug_html_str.as_ref().map(|debug_html_str| debug_html_str.as_str()),
                );
                // 1.3.3: add the matched selectors to the list
                matches.push(
                    ElementMatches{
                        element,
                        selectors: SelectorsOrSharedStyles::Selectors(matched_selectors)
                    }
                );
                if let Some(selector_stats) = selector_stats.as_deref_mut() {
                    selector_stats.extend(sel_stats.unwrap().into_iter())
                }
                // 1.3.4: insert the element into the style sharing cache
                let start = Start::now();
                context.thread_local.sharing_cache.insert_if_possible(
                    &element ,
                    &stylo_interface::default_style(), // We can just insert the default style here because all this is used for is to compute some bool called `considered_nontrivial_scoped_style`, and I commented all usage of that out anyway.
                    // The actual style we end up getting from the cache (if hit) comes from the element that we put in, so pointers will be shared :).
                    None,
                    element_depth,
                    &context.shared,
                );
                stats.times.inserting_into_sharing_cache += start.elapsed();
            }
        }
        // 2. traverse children
        for child in element.child_elements() {
            preorder_traversal(
                child,
                element_depth+1,
                context,
                matches,
                selector_stats.as_deref_mut(),
                selector_map,
                cascade_data,
                optimizations,
                stats
            );
        }
    }
    let author_guard = matching_context.stylesheet_lock().read();
    let ua_or_user_lock = SharedRwLock::new();
    let ua_or_user_guard = ua_or_user_lock.read();
    let stylist = matching_context.stylist();
    let cascade_data = stylist.cascade_data().borrow_for_origin(Origin::Author);
    // TODO: It's evident from this that we get one selector map per origin. How do real browsers handle all three origins (Author, User, User Agent)?
    let selector_map = cascade_data.normal_rules(&[]).unwrap();
    let shared_style_context = SharedStyleContext {
        stylist,
        visited_styles_enabled: true,
        options: StyleSystemOptions {
            disable_style_sharing_cache: false,
            dump_style_statistics: false, // TODO: maybe change this later
            style_statistics_threshold: 0, // TODO: maybe change this later
        },
        guards: StylesheetGuards {
            author: &author_guard,
            ua_or_user: &ua_or_user_guard
        },
        current_time_for_animations: 0.0,
        traversal_flags: TraversalFlags::empty(),
        snapshot_map: &SnapshotMap::new(),
        animations: DocumentAnimationSet {
            sets: Arc::new(parking_lot::RwLock::new(HashMap::with_hasher(FxBuildHasher))),
        },
        registered_speculative_painters: &stylo_interface::MyRegisteredSpeculativePainters,
    };
    let mut style_context = StyleContext {
        shared: &shared_style_context,
        thread_local: &mut ThreadLocalStyleContext::new(false),
    };
    let mut result = Vec::new();
    let mut stats = Statistics::default();

    let root = document.root_element();
    preorder_traversal(
        root,
        0,
        &mut style_context,
        &mut result,
        selector_stats,
        selector_map,
        cascade_data,
        _optimizations,
        &mut stats
    );
    (DocumentMatches(result), stats)
}

pub fn mach_7<'a>(matches: &DocumentMatches<'a>) -> DocumentMatches<'a> {
    let mut res = Vec::new();
    let mut caches: SelectorCaches = Default::default();
    for element_matches in &matches.0 {
        let mut context = matching::MatchingContext::new(
            matching::MatchingMode::Normal,
            None,
            &mut caches,
            matching::QuirksMode::NoQuirks,
            matching::NeedsSelectorFlags::No,
            matching::MatchingForInvalidation::No,
        );
        let SelectorsOrSharedStyles::Selectors(selectors) = &element_matches.selectors else {
            panic!("Unexpected shared style passed to mach-7.") 
        };
        let element = element_matches.element;
        let matched_selectors = selectors
            .into_iter()
            .filter(|s| {
                let (res, stats) = matching::matches_selector(
                    s,
                    0,
                    None,
                    &element,
                    &mut context
                );
                debug_assert!(res);
                debug_assert_eq!(stats.time_fast_rejecting, None);
                res
            })
            .cloned()
            .collect();
        res.push(ElementMatches{ element, selectors: SelectorsOrSharedStyles::Selectors(matched_selectors) });
    }
    DocumentMatches(res)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use crate::result::Result;
    use crate::parse::{get_document_and_selectors, websites_path};
    use crate::structs::set::{SetDocumentMatches, SetSelectorsOrSharedStyles};
    use crate::structs::Selector;
    use crate::{Optimizations, do_website};
    use crate::preprocessing::concretize::convert_to_is_selectors;
    use crate::Algorithm;
    use cssparser::ToCss as _;
    use style::selector_parser::SelectorParser;
    use style::stylesheets::UrlExtraData;
    use test_log::test;

    #[test]
    fn sharable_styles_are_shared() -> Result<()> {
        let website = get_document_and_selectors(
            &websites_path().join("ten_divs_style_sharing")
        )?.unwrap();
        let (_, _, stats) = do_website(&website, Algorithm::WithStyleSharing, None);
        assert_eq!(stats.counts.sharing_instances, 9);
        Ok(())
    }

    #[test]
    // TODO: This test doesn't actually test what I want
    fn nonshareable_styles_are_not_shared() -> Result<()> {
        let website = get_document_and_selectors(
            &websites_path().join("ten_divs_style_sharing_2")
        )?.unwrap();
        let (_, _, stats) = do_website(&website, Algorithm::WithStyleSharing, None);
        assert_eq!(stats.counts.sharing_instances, 5);
        Ok(())
    }

    #[test]
    // looks like bad grammar, but this tests that the conversion to "is()" selectors works
    fn is_conversion_works() -> Result<()> {
        let website = get_document_and_selectors(
            &websites_path().join("is_conversion_test")
        )?.unwrap();
        let converted: Vec<_> =
            convert_to_is_selectors(&website.document(), &website.get_matcher().get_selectors())
                .iter()
                .map(Selector::to_css_string)
                .collect();
        let expected: Vec<_> = [
            ":is(.bottom-red, .bottom-green, .bottom-blue)",
            "div:is(.bottom-blue, .top-blue)", // Note: .bottom-blue appears in preorder before .top-blue
            "div.top-green div:is(.top-red, .bottom-red)",
        ].iter().map(|selector_str| {
            SelectorParser::parse_author_origin_no_namespace(
                selector_str,
                &UrlExtraData::from(url::Url::parse("about:blank").unwrap()),
            ).unwrap().slice()[0].clone().to_css_string() // god damn insane api
        }).collect();
        assert_eq!(expected, converted, "\nexpected: {:?}\nactual: {:?}", expected, converted);
        Ok(())
    }

    fn parse_selector(selector_str: &str) -> Selector {
        SelectorParser::parse_author_origin_no_namespace(
            selector_str,
            &UrlExtraData::from(url::Url::parse("about:blank").unwrap()),
        ).unwrap().slice()[0].clone()
    }

    fn selectors_for_element(matches: &SetDocumentMatches, html_substring: &str) -> BTreeSet<String> {
        let element = matches
            .0
            .values()
            .find(|element_matches| element_matches.element.html.contains(html_substring))
            .unwrap_or_else(|| panic!("failed to find element containing {html_substring}"));
        match &element.selectors {
            SetSelectorsOrSharedStyles::Selectors(selectors) => {
                selectors.iter().cloned().collect()
            }
            SetSelectorsOrSharedStyles::SharedWithElement(id) => {
                matches.find_selectors(*id).iter().cloned().collect()
            }
        }
    }

    #[test]
    fn prepare_selectors_without_optimizations_preserves_identity() {
        let selector = parse_selector(".foo");
        let selectors = vec![selector.clone()];
        let document = scraper::Html::parse_document("<html><body><div class='foo'></div></body></html>");
        let prepared = super::prepare_selectors(&document, &selectors, Optimizations::from_none());
        let reverse_map: HashMap<_, Vec<_>> = prepared
            .reverse_map
            .iter()
            .map(|(selector, originals)| {
                (
                    selector.clone(),
                    originals.iter().map(|original| original.to_css_string()).collect(),
                )
            })
            .collect();
        assert_eq!(
            prepared.selectors.iter().map(Selector::to_css_string).collect::<Vec<_>>(),
            vec![".foo"]
        );
        assert_eq!(reverse_map.get(".foo").cloned(), Some(vec![".foo".to_string()]));
    }

    #[test]
    fn prepare_selectors_maps_is_conversion_back_to_original() -> Result<()> {
        let website = get_document_and_selectors(
            &websites_path().join("is_conversion_test")
        )?.unwrap();
        let selectors = website.get_matcher().get_selectors();
        let prepared = super::prepare_selectors(
            &website.document(),
            &selectors,
            Optimizations {
                is_conversion: true,
                distribution: false,
                ..Optimizations::from_none()
            },
        );
        let reverse_map: HashMap<_, Vec<_>> = prepared
            .reverse_map
            .iter()
            .map(|(selector, originals)| {
                (
                    selector.clone(),
                    originals.iter().map(|original| original.to_css_string()).collect(),
                )
            })
            .collect();
        assert!(prepared.selectors.iter().any(|selector| selector.to_css_string().contains(":is(")));
        for original in &selectors {
            assert!(
                reverse_map
                    .values()
                    .any(|mapped| mapped.iter().any(|mapped_selector| mapped_selector == &original.to_css_string()))
            );
        }
        Ok(())
    }

    #[test]
    fn prepare_selectors_maps_distribution_back_to_original() {
        let selector = parse_selector("div:is(.left, .right)");
        let selectors = vec![selector.clone()];
        let document = scraper::Html::parse_document("<html><body><div class='left'></div><div class='right'></div></body></html>");
        let prepared = super::prepare_selectors(
            &document,
            &selectors,
            Optimizations {
                is_conversion: false,
                distribution: true,
                ..Optimizations::from_none()
            },
        );
        let prepared_css: BTreeSet<_> = prepared.selectors.iter().map(Selector::to_css_string).collect();
        assert_eq!(
            prepared_css,
            BTreeSet::from([
                "div.left".to_string(),
                "div.right".to_string(),
            ])
        );
        for selector_css in prepared_css {
            let originals = prepared.reverse_map.get(&selector_css).unwrap();
            assert_eq!(originals.len(), 1);
            assert_eq!(originals[0].to_css_string(), "div:is(.left, .right)");
        }
    }

    #[test]
    fn optimized_matching_returns_original_selectors() -> Result<()> {
        let website = get_document_and_selectors(
            &websites_path().join("distribute_test")
        )?.unwrap();
        let (_, distributed_matches, _) = do_website(&website, Algorithm::WithDistribution, None);
        let actual = selectors_for_element(
            &distributed_matches,
            "masonry-up",
        );
        assert_eq!(actual, BTreeSet::from([".section[class*=\"-up\"]".to_string()]));
        Ok(())
    }
}
