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
use style::selector_parser::SelectorImpl;
use std::collections::HashMap;
use std::iter::FlatMap;
use style::values::AtomIdent;
use style::values::AtomString;

use crate::Selector;

#[derive(Clone)]
enum InnerSelectors<'a> {
    Empty(std::iter::Empty<&'a Selector>),
    Once(std::iter::Once<&'a Selector>),
    Slice(std::slice::Iter<'a, Selector>),
}

impl<'a> Iterator for InnerSelectors<'a> {
    type Item = &'a Selector;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty(e) => e.next(),
            Self::Once(o) => o.next(),
            Self::Slice(s) => s.next(),
        }
    }
}

fn get_inner_selectors<'component>(component: &'component Component<SelectorImpl>) -> Option<InnerSelectors<'component>> {
    match component {
        Component::Negation(l) => Some(InnerSelectors::Slice(l.slice().iter())),
        Component::Slotted(s) => Some(InnerSelectors::Once(std::iter::once(s))),
        Component::Host(s) => Some(if let Some(s) = s { InnerSelectors::Once(std::iter::once(s)) } else { InnerSelectors::Empty(std::iter::empty()) }),
        Component::Where(l) | Component::Is(l) => Some(InnerSelectors::Slice(l.slice().iter())),
        _ => None,
    }
}


#[derive(Clone)]
pub struct FlattenedSelectors<'a, I: Iterator<Item = &'a Selector>> {
    iter: FlatMap< I, std::iter::Rev<std::slice::Iter<'a, Component<SelectorImpl>>>, fn(&'a Selector) -> std::iter::Rev<std::slice::Iter<'a, Component<SelectorImpl>>> >,
    stack: Vec<FlatMap< InnerSelectors<'a>, std::iter::Rev<std::slice::Iter<'a, Component<SelectorImpl>>>, fn(&'a Selector) -> std::iter::Rev<std::slice::Iter<'a, Component<SelectorImpl>>> >>,
}

impl<'a, I: Iterator<Item = &'a Selector>> FlattenedSelectors<'a, I> {
    pub fn from_iter(iter: I) -> Self {
        Self {
            iter: iter.flat_map(|sel| sel.iter_raw_parse_order_from(0)),
            stack: vec![],
        }
    }
}

impl<'a, I: Iterator<Item = &'a Selector>> Iterator for FlattenedSelectors<'a, I> {
    type Item = &'a Component<SelectorImpl>;

    fn next(&mut self) -> Option<Self::Item> {
        let next_component = match self.stack.last_mut() {
            Some(top) => top.next(),
            None => self.iter.next(),
        };
        if let Some(next_component) = next_component {
            // if the next component is :is() or similar...
            if let Some(new_iter) = get_inner_selectors(next_component) {
                // ...push a new flattened component iterator to the stack.
                self.stack.push(new_iter.flat_map(|sel| sel.iter_raw_parse_order_from(0)));
                // re-enter. This will get the first component in our new top-of-stack, or, if there wasn't a selector in the :is(), will skip over it and just get the next component in the current top-of-stack.
                return self.next();
            } else {
                // If the next component is not :is() or similar, just return it.
                return Some(next_component);
            }
        }
        // If there is no next component...
        else {
            //...pop an iterator off the stack.
            if self.stack.pop().is_some() {
                // If the stack wasn't empty, we at least have self.iter to call next() on. Re-enter.
                return self.next();
            } else {
                // If the stack was empty, we were at the end of self.iter. Return None.
                return None;
            }
        }
    }
}

pub fn substrings_from_selectors<'a>(selectors: impl Iterator<Item = &'a Selector> + Clone) -> impl Iterator<Item = &'a AtomString> + Clone {
    FlattenedSelectors::from_iter(selectors)
        .filter_map(optimizable_substring_from_component)
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

    enum OldOrNewComponent<'component> {
        Old(&'component Component<SelectorImpl>),
        New(Component<SelectorImpl>),
    }

    impl<'component> OldOrNewComponent<'component> {
        fn to_owned(self) -> Component<SelectorImpl> {
            match self {
                OldOrNewComponent::Old(c) => c.clone(),
                OldOrNewComponent::New(c) => c,
            }
        }
    }

    enum OldOrNewSelector<'selector> {
        Old(&'selector Selector),
        New(Selector),
    }
    
    impl<'selector> OldOrNewSelector<'selector> {
        fn to_owned(self) -> Selector {
            match self {
                OldOrNewSelector::Old(s) => s.clone(),
                OldOrNewSelector::New(s) => s,
            }
        }
    }

    // Helper function which takes a Component; if it's an attribute selector
    // with "class*=", look it up in the map and convert it to an equivalent
    // `is()` selector. Otherwise, return None.
    fn convert_to_is_component<'component>(
        map: &HashMap<&'component AtomString, IndexSet<&AtomIdent>>, // mapping from substrings to lists of classes which match
        component: &'component Component<SelectorImpl>,
    ) -> OldOrNewComponent<'component> {
        // Let the record show that I tried to reuse code by defining an inner function which takes a closure that wraps a cloned selector list in a new component. But it was impossible to write the type of a closure that takes a Map<Map<...>> here; I would have had to make my own custom iterator type.
        let convert_selector = |selector| convert_to_is_selector(map, selector);
        match component {
            Component::Negation(l) => {
                let converted = l.slice().iter().map(convert_selector);
                if converted.clone().all(|selector| matches!(selector, OldOrNewSelector::Old(_))) {
                    OldOrNewComponent::Old(component)
                } else {
                    let new_selectors = converted.map(OldOrNewSelector::to_owned);
                    OldOrNewComponent::New(Component::Negation(SelectorList::from_iter(new_selectors)))
                }
            },
            Component::Slotted(s) => {
                match convert_selector(s) {
                    OldOrNewSelector::Old(_) => OldOrNewComponent::Old(component),
                    OldOrNewSelector::New(s) => OldOrNewComponent::New(Component::Slotted(s)),
                }
            },
            Component::Host(s) => {
                match s {
                    Some(s) => match convert_selector(s) {
                        OldOrNewSelector::Old(_) => OldOrNewComponent::Old(component),
                        OldOrNewSelector::New(s) => OldOrNewComponent::New(Component::Host(Some(s))),
                    },
                    None => OldOrNewComponent::Old(component),
                }
            },
            Component::Where(l) => {
                let converted = l.slice().iter().map(convert_selector);
                if converted.clone().all(|selector| matches!(selector, OldOrNewSelector::Old(_))) {
                    OldOrNewComponent::Old(component)
                } else {
                    let new_selectors = converted.map(OldOrNewSelector::to_owned);
                    OldOrNewComponent::New(Component::Where(SelectorList::from_iter(new_selectors)))
                }
            },
            Component::Is(l) => {
                let converted = l.slice().iter().map(convert_selector);
                if converted.clone().all(|selector| matches!(selector, OldOrNewSelector::Old(_))) {
                    OldOrNewComponent::Old(component)
                } else {
                    let new_selectors = converted.map(OldOrNewSelector::to_owned);
                    OldOrNewComponent::New(Component::Is(SelectorList::from_iter(new_selectors)))
                }
            },
            component => {
                // fast reject
                if !may_be_optimizable_attr_selector(component) {
                    return OldOrNewComponent::Old(component);
                }
                match optimizable_substring_from_component(component) {
                    Some(substring) => OldOrNewComponent::New(Component::Is(
                        match map.get(substring) {
                            Some(set) =>
                                create_class_selector_list(set.iter().copied().cloned()),
                            None => create_class_selector_list(std::iter::empty()),
                        }
                    )),
                    None => OldOrNewComponent::Old(component),
                }
            }
        }
    }

    fn convert_to_is_selector<'sel>(
        map: &HashMap<&'sel AtomString, IndexSet<&AtomIdent>>,
        selector: &'sel Selector,
    ) -> OldOrNewSelector<'sel> { // None when no change
        let rewritten_components = selector.iter_raw_parse_order_from(0).map(|component| convert_to_is_component(map, component));
        if rewritten_components.clone().all(|component| matches!(component, OldOrNewComponent::Old(_))) {
            // Fast path: the selector doesn't need to be converted. Skip the expensive SelectorBuilder and just clone.
            OldOrNewSelector::Old(selector)
        } else {
            // slow path: feed all the components into a SelectorBuilder
            let mut builder = SelectorBuilder::default();
            for rewritten_component in rewritten_components {
                let rewritten_component = rewritten_component.to_owned();
                if let Some(combinator) = rewritten_component.as_combinator() {
                    builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
                    builder.push_combinator(combinator);
                } else {
                    builder.push_simple_selector(rewritten_component.clone());
                }
            }
            builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
            let new_selector = builder.build_selector(selectors::parser::ParseRelative::No);
            OldOrNewSelector::New(new_selector)
        }
    }

    let substr_to_classes  =
        build_substr_selector_index(document, substrings_from_selectors(selectors.iter()));
    let converted_selectors = selectors
        .into_iter()
        .map(|selector| convert_to_is_selector(&substr_to_classes, selector));
    if converted_selectors.clone().all(|selector| matches!(selector, OldOrNewSelector::Old(_))) {
        selectors.to_vec()
    } else {
        converted_selectors.map(OldOrNewSelector::to_owned).collect()
    }
}

