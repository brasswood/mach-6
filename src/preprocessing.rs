/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use indexmap::IndexSet;
use scraper::{ElementRef, Html};
use selectors::attr::AttrSelectorOperator;
use selectors::attr::ParsedAttrSelectorOperation;
use selectors::builder::SelectorBuilder;
use selectors::parser::Component;
use selectors::SelectorList;
use std::collections::HashMap;
use style::values::AtomIdent;
use style::values::AtomString;

use crate::Selector;

pub fn build_substr_selector_index<'substr, 'class>(
    document: &'class Html,
    substrings: impl Iterator<Item = &'substr AtomString>,
) -> HashMap<&'substr AtomString, IndexSet<&'class AtomIdent>> {
    // instead of taking a &[&AtomString] from the getgo, we will
    // memoize it here so that we can change within here to see if
    // it actually speeds things up.
    let substrings: Vec<&AtomString> = substrings.collect();
    // build the aho-corasick automaton
    let mut ah_builder = AhoCorasickBuilder::new();
    ah_builder.kind(None);
    let ac = ah_builder.build(substrings.iter().map(AsRef::as_ref)).unwrap();
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

fn may_be_optimizable_attr_selector(
    component: &Component<style::selector_parser::SelectorImpl>,
) -> bool {
    matches!(
        component,
        Component::AttributeInNoNamespace {
            local_name: _,
            operator: AttrSelectorOperator::Substring,
            value: _,
            ..
        } | Component::AttributeOther(_)
    )
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
    // `is()` selector. Otherwise, return None.
    fn convert_to_is_component(
        map: &HashMap<&AtomString, IndexSet<&AtomIdent>>, // mapping from substrings to lists of classes which match
        component: &Component<style::selector_parser::SelectorImpl>,
    ) -> Option<Component<style::selector_parser::SelectorImpl>>{
        optimizable_substring_from_component(component).map(|substring| {
            Component::Is(
                match map.get(substring) {
                    Some(set) =>
                        create_class_selector_list(set.iter().copied().cloned()),
                    None => create_class_selector_list(std::iter::empty()),
                }
            )
        })
    }

    let substr_to_classes  =
        build_substr_selector_index(document, substrings_from_selectors(selectors.iter()));
    selectors.into_iter().map(|selector| {
        // Pass 1: check if we will need to create a SelectorBuilder, because it's expensive.
        let maybe_should_convert_selector = selector.iter_raw_match_order().any(may_be_optimizable_attr_selector);
        if !maybe_should_convert_selector {
            // Fast path: the selector doesn't need to be converted. Skip the expensive SelectorBuilder and just clone.
            selector.clone()
        }
        else {
            // Pass 2: feed all the components into a SelectorBuilder
            let mut builder = SelectorBuilder::default();
            for component in selector.iter_raw_parse_order_from(0) {
                if let Some(combinator) = component.as_combinator() {
                    builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
                    builder.push_combinator(combinator);
                } else {
                    if let Some(new_component) = convert_to_is_component(&substr_to_classes, component) {
                        builder.push_simple_selector(new_component);
                    } else {
                        builder.push_simple_selector(component.clone());
                    }
                }
            }
            builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
            builder.build_selector(selectors::parser::ParseRelative::No)
        }
    }).collect()
}

