/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use derive_more::Display;
use log::info;
use rustc_hash::FxBuildHasher;
use selectors::Element as _;
use style::animation::DocumentAnimationSet;
use style::bloom::StyleBloom;
use style::context::SharedStyleContext;
use style::context::StyleSystemOptions;
use style::context::ThreadLocalStyleContext;
use style::selector_parser::SnapshotMap;
use style::shared_lock::StylesheetGuards;
use style::sharing::StyleSharingElement as _;
use style::traversal_flags::TraversalFlags;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;
use scraper::ElementRef;
use scraper::Html;
use selectors::context::SelectorCaches;
use selectors::matching::{self, Statistics};
use style::context::StyleContext;
use style::rule_tree::CascadeLevel;
use style::selector_map::SelectorMapElement as _;
use style::selector_map::SelectorMap;
use style::servo_arc::Arc;
use style::shared_lock::SharedRwLock;
use style::stylist::CascadeData;
use style::stylist::Rule;
use style::stylist::Stylist;
use style::values::AtomIdent;
use style::sharing::StyleSharingTarget;
use style::thread_state::{self, ThreadState};
use smallvec::SmallVec;

mod stylo_interface;
pub mod parse;
pub mod result;
pub mod structs;

pub use parse::get_all_documents_and_selectors;
use crate::parse::ParsedWebsite;
use crate::result::Result;
use crate::structs::{
    Element, Selector,
    borrowed::{
        DocumentMatches,
        ElementMatches,
        SelectorsOrSharedStyles,
    },
    owned::{
        OwnedDocumentMatches,
        OwnedElementMatches,
        OwnedSelectorsOrSharedStyles,
    },
    set::SetDocumentMatches,
};

#[derive(Debug, Display, Clone, Copy)]
pub enum Algorithm {
    Naive,
    WithSelectorMap,
    WithBloomFilter,
    WithStyleSharing,
    Mach7,
}

fn debug_element(element: &ElementRef) {
    if element.has_id(&AtomIdent::from("PRINT ME"), scraper::CaseSensitivity::CaseSensitive) {
        let mut msg = String::new();
        writeln!(&mut msg, "PRINT ME element encountered!").unwrap();
        writeln!(&mut msg, "I am {:?}", element).unwrap();
        writeln!(&mut msg, "My children are:").unwrap();
        for child in element.children() {
            writeln!(&mut msg, "  {:?}", child.value()).unwrap();
        }
        info!("{}", msg);
    }
}

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
            r.map(|w| do_website(&w, algorithm))
        })
    )
}

pub fn do_website(website: &ParsedWebsite, algorithm: Algorithm) -> (String, SetDocumentMatches, Statistics){
    let (matches, stats) = match algorithm {
        Algorithm::Naive => (
            OwnedDocumentMatches::from(match_selectors(&website.document, &website.selectors)),
            Statistics::default()
        ),
        Algorithm::WithSelectorMap => {
            let selector_map = build_selector_map(&website.selectors);
            match_selectors_with_selector_map(&website.document, &selector_map)
        }
        Algorithm::WithBloomFilter => {
            let selector_map = build_selector_map(&website.selectors);
            match_selectors_with_bloom_filter(&website.document, &selector_map)
        }
        Algorithm::WithStyleSharing => {
            let selector_map = build_selector_map(&website.selectors);
            match_selectors_with_style_sharing(&website.document, &selector_map)
        }
        Algorithm::Mach7 => {
            let document_matches = match_selectors(&website.document, &website.selectors);
            (
                OwnedDocumentMatches::from(mach_7(&document_matches)),
                Statistics::default()
            )
        }
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
                let (res, stats) = matching::matches_selector(s, 0, None, &element, &mut context);
                debug_assert_eq!(stats.fast_rejects, Some(0));
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

pub fn build_selector_map<'a, I>(selectors: I) -> SelectorMap<Rule>
where
    I: IntoIterator<Item = &'a Selector>,
{
    let mut selector_map: SelectorMap<Rule> = SelectorMap::new();
    let iter = selectors.into_iter()
        .map(Clone::clone)
        .enumerate();
    for (i, selector) in iter {
        use style::context::QuirksMode;
        let hashes = selectors::parser::AncestorHashes::new(&selector, QuirksMode::NoQuirks); // needed to avoid borrow after move. TODO: look at what this does.
        let rule = Rule {
            selector,
            hashes, 
            source_order: i.try_into().unwrap(),
            layer_id: style::stylist::LayerId::root(),
            container_condition_id: style::stylist::ContainerConditionId::none(),
            is_starting_style: false,
            scope_condition_id: style::stylist::ScopeConditionId::none(),
            style_source: style::rule_tree::StyleSource::from_declarations(Arc::new(SharedRwLock::new().wrap(Default::default()))),
        };
        selector_map.insert(rule, QuirksMode::NoQuirks).unwrap();
    }
    selector_map
}

pub fn match_selectors_with_selector_map(document: &Html, selector_map: &SelectorMap<Rule>) -> (OwnedDocumentMatches, Statistics) {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>, 
        matches: &mut Vec<OwnedElementMatches>,
        selector_map: &SelectorMap<Rule>,
        caches: &mut SelectorCaches,
        stats: &mut Statistics,
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
        // 1.2: Use the selector map to get matching rules
        let mut matched_selectors = SmallVec::new();
        *stats += selector_map.get_all_matching_rules(
            element,
            element, // TODO: ????
            &mut SmallVec::new(),
            &mut Some(&mut matched_selectors),
            &mut context,
            CascadeLevel::UANormal, // TODO: ??????
            &CascadeData::new(),
            &Stylist::new(stylo_interface::mock_device(), matching::QuirksMode::NoQuirks)
        );
        matches.push(OwnedElementMatches{ element: Element::from(element), selectors: OwnedSelectorsOrSharedStyles::Selectors(matched_selectors) });
        // 2. traverse children
        for child in element.child_elements() {
            preorder_traversal(child, matches, selector_map, caches, stats);
        }
    }

    let mut caches = SelectorCaches::default();
    let mut result = Vec::new();
    let mut stats = Statistics::default();
    preorder_traversal(document.root_element(), &mut result, selector_map, &mut caches, &mut stats);
    debug_assert_eq!((stats.fast_rejects, stats.sharing_instances), (Some(0), None));
    (stats.fast_rejects, stats.slow_rejects, stats.time_spent_slow_rejecting) = (None, None, None);
    (OwnedDocumentMatches(result), stats)
}

pub fn match_selectors_with_bloom_filter(document: &Html, selector_map: &SelectorMap<Rule>) -> (OwnedDocumentMatches, Statistics) {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>, 
        element_depth: usize,
        matches: &mut Vec<OwnedElementMatches>,
        selector_map: &SelectorMap<Rule>,
        style_bloom: &mut StyleBloom<ElementRef<'a>>,
        caches: &mut SelectorCaches,
        stats: &mut Statistics,
    ) {
        // 1. do thing
        // 1.1: update the bloom filter with the current element
        style_bloom.insert_parents_recovering(element, element_depth);
        // 1.2: create a MatchingContext (after updating style_bloom to avoid borrow check error)
        let mut context = matching::MatchingContext::new(
            matching::MatchingMode::Normal,
            Some(style_bloom.filter()),
            caches,
            matching::QuirksMode::NoQuirks,
            matching::NeedsSelectorFlags::No,
            matching::MatchingForInvalidation::No,
        );
        // 1.3: Use the selector map to get matching rules
        let mut matched_selectors = SmallVec::new();
        *stats += selector_map.get_all_matching_rules(
            element,
            element, // TODO: ????
            &mut SmallVec::new(),
            &mut Some(&mut matched_selectors),
            &mut context,
            CascadeLevel::UANormal, // TODO: ??????
            &CascadeData::new(),
            &Stylist::new(stylo_interface::mock_device(), matching::QuirksMode::NoQuirks)
        );
        matches.push(OwnedElementMatches{ element: Element::from(element), selectors: OwnedSelectorsOrSharedStyles::Selectors(matched_selectors) });
        // 2. traverse children
        #[cfg(debug_assertions)]
        {
            debug_element(&element);
            assert_childrens_parent_is_me(&element);
        }
        for child in element.child_elements() {
            preorder_traversal(child, element_depth+1, matches, selector_map, style_bloom, caches, stats);
        }
    }
    let mut bloom_filter = StyleBloom::new();
    let mut caches = SelectorCaches::default();
    let mut result = Vec::new();
    let mut stats = Statistics::default();
    preorder_traversal(document.root_element(), 0, &mut result, selector_map, &mut bloom_filter, &mut caches, &mut stats);
    debug_assert_eq!(stats.sharing_instances, None);
    (OwnedDocumentMatches(result), stats)
}

pub fn match_selectors_with_style_sharing(document: &Html, selector_map: &SelectorMap<Rule>) -> (OwnedDocumentMatches, Statistics) {
    #[derive(Default)]
    struct NonOptionalStats {
        sharing_instances: usize,
        sharing_check_duration: Duration,
    }
    fn preorder_traversal<'a>(
        element: ElementRef<'a>,
        element_depth: usize,
        context: &mut StyleContext<ElementRef<'a>>,
        matches: &mut Vec<OwnedElementMatches>,
        selector_map: &SelectorMap<Rule>,
        caches: &mut SelectorCaches,
        stats: &mut Statistics,
        non_optional_stats: &mut NonOptionalStats,
    ) {
        // 1. do thing
        // 1.1: Set thread state to layout (needed to avoid debug_assert panic)
        thread_state::initialize(ThreadState::LAYOUT);
        // 1.2: update the bloom filter with the current element
        context.thread_local.bloom_filter.insert_parents_recovering(element, element_depth);
        // 1.3: Check if we can share styles
        let mut target = StyleSharingTarget::new(element);
        let start = Instant::now();
        let style_sharing_result = target.share_style_if_possible(context);
        non_optional_stats.sharing_check_duration += start.elapsed();
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
                let element = Element::from(element);
                let other_element = Element::from(other_element);
                matches.push(OwnedElementMatches{ element, selectors: OwnedSelectorsOrSharedStyles::SharedWithElement(other_element.id) });
                non_optional_stats.sharing_instances += 1;
            },
            None => {
                // If we can't share styles, go through the selector map and bloom filter.
                // 1.3.1: create a MatchingContext (after updating style_bloom to avoid borrow check error)
                let mut matching_context = matching::MatchingContext::new(
                    matching::MatchingMode::Normal,
                    Some(context.thread_local.bloom_filter.filter()),
                    caches,
                    matching::QuirksMode::NoQuirks,
                    matching::NeedsSelectorFlags::No,
                    matching::MatchingForInvalidation::No,
                );
                // 1.3.2: Use the selector map to get matching rules
                let mut matched_selectors = SmallVec::new();
                *stats += selector_map.get_all_matching_rules(
                    element,
                    element, // TODO: ????
                    &mut SmallVec::new(),
                    &mut Some(&mut matched_selectors),
                    &mut matching_context,
                    CascadeLevel::UANormal, // TODO: ??????
                    &CascadeData::new(),
                    context.shared.stylist,
                );
                // 1.3.3: add the matched selectors to the list
                matches.push(
                    OwnedElementMatches{
                        element: Element::from(element),
                        selectors: OwnedSelectorsOrSharedStyles::Selectors(matched_selectors)
                    }
                );
                // 1.3.4: insert the element into the style sharing cache
                context.thread_local.sharing_cache.insert_if_possible(
                    &element,
                    &stylo_interface::default_style(), // We can just insert the default style here because all this is used for is to compute some bool called `considered_nontrivial_scoped_style`, and I commented all usage of that out anyway.
                    // The actual style we end up getting from the cache (if hit) comes from the element that we put in, so pointers will be shared :).
                    None,
                    element_depth,
                    &context.shared,
                );
            }
        }
        // 2. traverse children
        #[cfg(debug_assertions)]
        {
            debug_element(&element);
            assert_childrens_parent_is_me(&element);
        }
        for child in element.child_elements() {
            preorder_traversal(child, element_depth+1, context, matches, selector_map, caches, stats, non_optional_stats);
        }
    }
    // TODO: I probably want to put the creation of the Stylist outside of the benchmark, but I don't see a very easy way to do that at the moment. Will need to do pinning and a self-referential struct and all that, or a macro.
    let stylist = Stylist::new(stylo_interface::mock_device(), matching::QuirksMode::NoQuirks);
    let author_lock = SharedRwLock::new();
    let ua_or_user_lock = SharedRwLock::new();
    let shared_style_context = SharedStyleContext {
        stylist: &stylist,
        visited_styles_enabled: true,
        options: StyleSystemOptions {
            disable_style_sharing_cache: false,
            dump_style_statistics: false, // TODO: maybe change this later
            style_statistics_threshold: 0, // TODO: maybe change this later
        },
        guards: StylesheetGuards {
            author: &author_lock.read(),
            ua_or_user: &ua_or_user_lock.read()
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
        thread_local: &mut ThreadLocalStyleContext::new(),
    };
    let mut caches = SelectorCaches::default();
    let mut result = Vec::new();
    let mut stats = Statistics::default();
    let mut non_optional_stats = NonOptionalStats::default();

    let root = document.root_element();
    preorder_traversal(root, 0, &mut style_context, &mut result, selector_map, &mut caches, &mut stats, &mut non_optional_stats);
    stats.sharing_instances = Some(non_optional_stats.sharing_instances);
    stats.time_spent_checking_style_sharing = Some(non_optional_stats.sharing_check_duration);
    (OwnedDocumentMatches(result), stats)
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
                debug_assert_eq!(stats.fast_rejects, Some(0));
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
    use crate::result::Result;
    use crate::parse::{get_document_and_selectors, websites_path};
    use crate::do_website;
    use crate::Algorithm;
    use test_log::test;

    #[test]
    fn sharable_styles_are_shared() -> Result<()> {
        let website = get_document_and_selectors(
            &websites_path().join("ten_divs_style_sharing")
        )?.unwrap();
        let (_, _, stats) = do_website(&website, Algorithm::WithStyleSharing);
        assert_eq!(stats.sharing_instances, Some(9));
        Ok(())
    }

    #[test]
    // TODO: This test doesn't actually test what I want
    fn nonshareable_styles_are_not_shared() -> Result<()> {
        let website = get_document_and_selectors(
            &websites_path().join("ten_divs_style_sharing_2")
        )?.unwrap();
        let (_, _, stats) = do_website(&website, Algorithm::WithStyleSharing);
        assert_eq!(stats.sharing_instances, Some(5));
        Ok(())
    }
}