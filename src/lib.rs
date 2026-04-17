/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use aho_corasick::AhoCorasick;
use clap::ValueEnum;
use ::cssparser::ToCss as _;
use derive_more::Display;
use indexmap::IndexSet;
use rustc_hash::FxBuildHasher;
use selectors::attr::AttrSelectorOperator;
use selectors::attr::ParsedAttrSelectorOperation;
use selectors::builder::SelectorBuilder;
use selectors::matching::SelectorStats;
use selectors::parser::Component;
use selectors::SelectorList;
use style::animation::DocumentAnimationSet;
use style::context::SharedStyleContext;
use style::context::StyleSystemOptions;
use style::context::ThreadLocalStyleContext;
#[cfg(feature = "debug_element")]
use style::selector_map::debug_element_selector;
use style::selector_parser::SnapshotMap;
use style::shared_lock::SharedRwLockReadGuard;
use style::shared_lock::{SharedRwLock, StylesheetGuards};
use style::sharing::StyleSharingElement as _;
use style::stylesheets::DocumentStyleSheet;
use style::stylesheets::UrlExtraData;
use style::stylist::Stylist;
use style::traversal_flags::TraversalFlags;
use style::values::AtomIdent;
use style::values::AtomString;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
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
use style::stylist::CascadeData;
use style::stylist::Rule;
use style::sharing::StyleSharingTarget;
use style::stylesheets::Origin;
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

#[derive(Debug, Display, Clone, Copy, ValueEnum)]
pub enum Algorithm {
    Naive,
    WithStyleSharing,
    WithPreprocessing,
    Mach7,
}

type MatchPair = (Element, Selector);

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
    let (matches, stats) = match algorithm {
        Algorithm::Naive => (
            OwnedDocumentMatches::from(&match_selectors(&website.document, website.selectors())),
            Statistics::default()
        ),
        Algorithm::WithStyleSharing => {
            match_selectors_with_style_sharing(&website.document, website.stylist(), &website.stylesheet_lock, None)
        },
        Algorithm::WithPreprocessing => {
            let preprocessed_selectors =
                convert_to_is_selectors( &website.document, website.selectors());
            let reverse_map: HashMap<String, &Selector> = preprocessed_selectors
                .iter()
                .zip(website.selectors().iter())
                .map(|(preprocessed, original)| (preprocessed.to_css_string(), original))
                .collect();
            let (preprocessed_stylist, preprocessed_lock) = stylist_from_selectors(&preprocessed_selectors);
            let mut result = match_selectors_with_style_sharing(
                &website.document,
                &preprocessed_stylist,
                &preprocessed_lock,
                None,
            );
            for oem in result.0.0.iter_mut() {
                if let OwnedSelectorsOrSharedStyles::Selectors(selectors) = &mut oem.selectors {
                    for selector in selectors.iter_mut() {
                        *selector = reverse_map
                            .get(&selector.to_css_string())
                            .copied()
                            .unwrap_or_else(|| {
                                panic!(
                                    "failed to reverse preprocessed selector {}",
                                    selector.to_css_string()
                                )
                            })
                            .clone();
                    }
                }
            }
            result
        }
        Algorithm::Mach7 => {
            if let Some(document_matches) = mach7_oracle {
                (
                    OwnedDocumentMatches::from(&mach_7(document_matches)),
                    Statistics::default()
                )
            } else {
                let document_matches = match_selectors(&website.document, website.selectors());
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

pub fn build_substr_selector_index<'substr, 'class>(
    document: &'class Html,
    substrings: impl Iterator<Item = &'substr AtomString>,
) -> HashMap<&'substr AtomString, IndexSet<&'class AtomIdent>> {
    // instead of taking a &[&AtomString] from the getgo, we will
    // memoize it here so that we can change within here to see if
    // it actually speeds things up.
    let substrings: Vec<&AtomString> = substrings.collect();
    // build the aho-corasick automaton
    let ac = AhoCorasick::new(substrings.iter().map(AsRef::as_ref)).unwrap();
    let mut ret: HashMap<&AtomString, IndexSet<&AtomIdent>> = HashMap::new();

    fn preorder_traversal<'substr, 'class>(
        map: &mut HashMap<&'substr AtomString, IndexSet<&'class AtomIdent>>,
        substrings: &[&'substr AtomString],
        ac: &AhoCorasick,
        element: ElementRef<'class>,
    ) {
        // substring not present in the map -> substring never encountered in the selector list
        // or substring encountered but no classes in DOM match
        // substring present in map, value has classes -> classes with substring found.
        for class in element.value().classes_atom() {
            for ac_match in ac.find_overlapping_iter(class.as_ref()) {
                let pat = ac_match.pattern();
                let matching_substr = substrings[pat.as_usize()];
                let matching_classes = map.entry(matching_substr).or_default(); // TODO: Should I make the value type an Option instead of having an empty map when no matching class is found?
                matching_classes.insert(class);
            }
        }
        for child in element.child_elements() {
            preorder_traversal(
                map,
                substrings,
                ac,
                child,
            );
        }
    }
    preorder_traversal(
        &mut ret,
        &substrings[..],
        &ac,
        document.root_element(),
    );
    ret
}

pub fn substrings_from_selectors<'a>(selectors: impl Iterator<Item = &'a Selector> + Clone) -> impl Iterator<Item = &'a AtomString> + Clone {
    selectors
        .flat_map(|selector| 
            selector.iter_raw_parse_order_from(0)
        )
        .filter_map(optimizable_substring_from_component)
}

fn optimizable_substring_from_component(
    component: &Component<style::selector_parser::SelectorImpl>
) -> Option<&AtomString> {
    let substring = match component {
        Component::AttributeInNoNamespace {
            local_name,
            operator: AttrSelectorOperator::Substring,
            value: substring,
            ..
        } if local_name.as_ref() == "class" => substring,
        Component::AttributeOther(attr) 
            if attr.local_name.as_ref() == "class"
        => {
            let ParsedAttrSelectorOperation::WithValue {
                operator: AttrSelectorOperator::Substring,
                value: ref substring,
                ..
            } = attr.operation else { return None };
            substring
        },
        _ => return None,
    };
    // only return a substring if it doesn't contain whitespace
    (!substring.0.contains(" ")).then_some(substring)
}


pub fn convert_to_is_selectors(
    document: &Html,
    selectors: &[Selector],
) -> Vec<Selector> {
    // Helper function to turn a list of class names into a `SelectorList`
    fn create_class_selector_list(classes: impl ExactSizeIterator<Item = AtomIdent>) -> SelectorList<style::selector_parser::SelectorImpl> {
        let selectors = classes.map(|class_str| {
            let mut builder = SelectorBuilder::default();
            builder.push_simple_selector(Component::Class(class_str));
            builder.build_selector(selectors::parser::ParseRelative::No)
        });
        SelectorList::from_iter(selectors)
    }

        
    // Helper function which takes a Component; if it's an attribute selector
    // with "class*=", look it up in the map and convert it to an equivalent
    // `is()` selector. Otherwise, just clone the component and return it.
    fn convert_to_is_component(
        map: &HashMap<&AtomString, IndexSet<&AtomIdent>>, // mapping from substrings to lists of classes which match
        component: &Component<style::selector_parser::SelectorImpl>,
    ) -> Component<style::selector_parser::SelectorImpl>{
        match optimizable_substring_from_component(component) {
            Some(substring) => Component::Is(
                match map.get(substring) {
                    Some(set) =>
                        create_class_selector_list(set.iter().copied().cloned()),
                    None => create_class_selector_list(std::iter::empty()),
                }
            ),
            None => component.clone()
        }
    }

    let substr_to_classes  =
        build_substr_selector_index(document, substrings_from_selectors(selectors.iter()));
    selectors.into_iter().map(|selector| {
        let mut builder = SelectorBuilder::default();
        for component in selector.iter_raw_parse_order_from(0) {
            if let Some(combinator) = component.as_combinator() {
                builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
                builder.push_combinator(combinator);
            } else {
                builder.push_simple_selector(
                    convert_to_is_component(
                        &substr_to_classes,
                        component,
                    )
                );
            }
        }
        builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
        builder.build_selector(selectors::parser::ParseRelative::No)
    }).collect()
}

pub fn stylist_from_selectors(selectors: &[Selector]) -> (Stylist, SharedRwLock) {
    let stylesheet_lock = SharedRwLock::new();
    let css = selectors
        .iter()
        .map(|selector| format!("{} {{}}", selector.to_css_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let stylesheet = parse::parse_stylesheet(
        &css,
        UrlExtraData::from(url::Url::parse("about:blank").unwrap()),
        &stylesheet_lock,
    )
    .expect("synthetic selector stylesheet should parse");
    let stylist = stylist_from_stylesheets(
        std::iter::once(&stylesheet),
        &stylesheet_lock.read(),
    );
    (stylist, stylesheet_lock)
}

pub fn stylist_from_stylesheets<'a>(
    stylesheets: impl Iterator<Item = &'a DocumentStyleSheet>,
    author_guard: &SharedRwLockReadGuard
) -> Stylist {
    let mut stylist = Stylist::new(
        stylo_interface::mock_device(),
        selectors::matching::QuirksMode::NoQuirks,
    );
    for sheet in stylesheets {
        stylist.append_stylesheet(sheet.clone(), &author_guard);
    }
    let ua_or_user_lock = SharedRwLock::new();
    let ua_or_user_guard = ua_or_user_lock.read();
    stylist.flush_without_invalidation(&StylesheetGuards {
        author: &author_guard,
        ua_or_user: &ua_or_user_guard,
    });
    stylist
}

pub fn selectors_from_stylist(stylist: &Stylist) -> Vec<Selector> {
    let mut selectors = BTreeMap::new();
    let cascade_data = stylist.cascade_data().borrow_for_origin(Origin::Author);
    if let Some(map) = cascade_data.normal_rules(&[]) {
        collect_selectors_from_map(map, &mut selectors);
    }
    selectors.into_values().collect()
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

pub fn match_selectors_with_style_sharing(
    document: &Html,
    stylist: &style::stylist::Stylist,
    stylesheet_lock: &SharedRwLock,
    selector_stats: Option<&mut SmallVec<[(MatchPair, SelectorStats); 16]>>,
) -> (OwnedDocumentMatches, Statistics) {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>,
        element_depth: usize,
        context: &mut StyleContext<ElementRef<'a>>,
        matches: &mut Vec<OwnedElementMatches>,
        mut selector_stats: Option<&mut SmallVec<[(MatchPair, SelectorStats); 16]>>,
        selector_map: &SelectorMap<Rule>,
        cascade_data: &CascadeData,
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
        let start = Instant::now();
        context.thread_local.bloom_filter.insert_parents_recovering(element, element_depth);
        stats.times.updating_bloom_filter += start.elapsed();
        // 1.3: Check if we can share styles
        let mut target = StyleSharingTarget::new(element);
        let start = Instant::now();
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
                let element = Element::from(element);
                let other_element = Element::from(other_element);
                matches.push(OwnedElementMatches{ element, selectors: OwnedSelectorsOrSharedStyles::SharedWithElement(other_element.id) });
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
                    OwnedElementMatches{
                        element: Element::from(element),
                        selectors: OwnedSelectorsOrSharedStyles::Selectors(matched_selectors)
                    }
                );
                if let Some(selector_stats) = selector_stats.as_deref_mut() {
                    selector_stats.extend(
                        sel_stats.unwrap().into_iter().map(|(sel, stats)| {
                            let match_pair = (Element::from(element), sel);
                            (match_pair, stats)
                        })
                    )
                }
                // 1.3.4: insert the element into the style sharing cache
                let start = Instant::now();
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
                stats
            );
        }
    }
    let author_guard = stylesheet_lock.read();
    let ua_or_user_lock = SharedRwLock::new();
    let ua_or_user_guard = ua_or_user_lock.read();
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
        thread_local: &mut ThreadLocalStyleContext::new(),
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
        &mut stats
    );
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
    use crate::result::Result;
    use crate::parse::{get_document_and_selectors, websites_path};
    use crate::structs::Selector;
    use crate::{convert_to_is_selectors, do_website};
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
            convert_to_is_selectors(&website.document, website.selectors())
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
}
