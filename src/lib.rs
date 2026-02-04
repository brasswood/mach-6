/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use derive_more::Display;
use rustc_hash::FxBuildHasher;
use selectors::Element as _;
use style::animation::DocumentAnimationSet;
use style::bloom::StyleBloom;
use style::context::SharedStyleContext;
use style::context::StyleSystemOptions;
use style::context::ThreadLocalStyleContext;
use style::selector_parser::SnapshotMap;
use style::shared_lock::StylesheetGuards;
use style::traversal_flags::TraversalFlags;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use scraper::ElementRef;
use scraper::Html;
use selectors::context::SelectorCaches;
use selectors::matching;
use style::context::StyleContext;
use style::rule_tree::CascadeLevel;
use style::selector_map::SelectorMapElement as _;
use style::selector_map::{SelectorMap, Statistics};
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

pub use parse::get_documents_and_selectors;
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
}

pub fn do_all_websites(websites: &Path, algorithm: Algorithm) -> Result<impl Iterator<Item = Result<(String, SetDocumentMatches)>>> {
    Ok(get_documents_and_selectors(websites)?
        .map(move |r| {
            r.map(|(w, h, s)| {
                let matches = match algorithm {
                    Algorithm::Naive => OwnedDocumentMatches::from(match_selectors(&h, &s)),
                    Algorithm::WithSelectorMap => {
                        let selector_map = build_selector_map(&s);
                        match_selectors_with_selector_map(&h, &selector_map).0
                    }
                    Algorithm::WithBloomFilter => {
                        let selector_map = build_selector_map(&s);
                        match_selectors_with_bloom_filter(&h, &selector_map).0
                    }
                    Algorithm::WithStyleSharing => {
                        let selector_map = build_selector_map(&s);
                        match_selectors_with_style_sharing(&h, &selector_map).0
                    }
                };
                (w, SetDocumentMatches::from(matches))
            })
        })
    )
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
                let (res, fast_rejects) = matching::matches_selector(s, 0, None, &element, &mut context);
                debug_assert_eq!(fast_rejects, 0);
                res
            })
            .collect();
        matches.push(ElementMatches{ element: Element::from(element), selectors: SelectorsOrSharedStyles::Selectors(matched_selectors) });
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
        if element.has_id(&AtomIdent::from("PRINT ME"), scraper::CaseSensitivity::CaseSensitive) {
            println!("PRINT ME element encountered!");
            println!("I am {:?}", element);
            println!("My children are:");
            for child in element.children() {
                println!("  {:?}", child.value());
            }
        }
        for child in element.child_elements() {
            // assert that all of my children's parent is me
            if child.traversal_parent().unwrap() != element {
                let mut msg = String::new();
                writeln!(&mut msg, "me: {:?}", element);
                writeln!(&mut msg, "my child: {:?}", child);
                writeln!(&mut msg, "my child's traversal_parent: {:?}", child.traversal_parent().unwrap());
                panic!("child's traversal_parent was not equal to me!\n{msg}");
            }
            preorder_traversal(child, element_depth+1, matches, selector_map, style_bloom, caches, stats);
        }
    }
    let mut bloom_filter = StyleBloom::new();
    let mut caches = SelectorCaches::default();
    let mut result = Vec::new();
    let mut stats = Statistics::default();
    preorder_traversal(document.root_element(), 0, &mut result, selector_map, &mut bloom_filter, &mut caches, &mut stats);
    (OwnedDocumentMatches(result), stats)
}

pub fn match_selectors_with_style_sharing(document: &Html, selector_map: &SelectorMap<Rule>) -> (OwnedDocumentMatches, Statistics) {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>, 
        element_depth: usize,
        context: &mut StyleContext<ElementRef<'a>>,
        matches: &mut Vec<OwnedElementMatches>,
        selector_map: &SelectorMap<Rule>,
        caches: &mut SelectorCaches,
        stats: &mut Statistics,
        sharing_instances: &mut usize,
    ) {
        // 1. do thing
        // 1.1: Set thread state to layout (needed to avoid debug_assert panic)
        thread_state::initialize(ThreadState::LAYOUT);
        // 1.2: update the bloom filter with the current element
        context.thread_local.bloom_filter.insert_parents_recovering(element, element_depth);
        // 1.3: Check if we can share styles
        let mut target = StyleSharingTarget::new(element);
        match target.share_style_if_possible(context) {
            Some((other_element, _shared_styles)) => {
                // If we can share styles, do that.
                let element = Element::from(element);
                let other_element = Element::from(other_element);
                matches.push(OwnedElementMatches{ element, selectors: OwnedSelectorsOrSharedStyles::SharedWithElement(other_element.id) });
                *sharing_instances += 1;
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
                matches.push(OwnedElementMatches{ element: Element::from(element), selectors: OwnedSelectorsOrSharedStyles::Selectors(matched_selectors) });
            }
        }
        // 2. traverse children
        if element.has_id(&AtomIdent::from("PRINT ME"), scraper::CaseSensitivity::CaseSensitive) {
            println!("PRINT ME element encountered!");
            println!("I am {:?}", element);
            println!("My children are:");
            for child in element.children() {
                println!("  {:?}", child.value());
            }
        }
        for child in element.child_elements() {
            // assert that all of my children's parent is me
            if child.traversal_parent().unwrap() != element {
                let mut msg = String::new();
                writeln!(&mut msg, "me: {:?}", element);
                writeln!(&mut msg, "my child: {:?}", child);
                writeln!(&mut msg, "my child's traversal_parent: {:?}", child.traversal_parent().unwrap());
                panic!("child's traversal_parent was not equal to me!\n{msg}");
            }
            preorder_traversal(child, element_depth+1, context, matches, selector_map, caches, stats, sharing_instances);
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
    let mut sharing_instances = 0;
    preorder_traversal(document.root_element(), 0, &mut style_context, &mut result, selector_map, &mut caches, &mut stats, &mut sharing_instances);
    stats.sharing_instances = Some(sharing_instances);
    (OwnedDocumentMatches(result), stats)
}