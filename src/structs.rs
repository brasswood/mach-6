/* Copyright 2026 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use scraper::ElementRef;
use std::fmt::Write as _;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher as _;

#[derive(Debug, Clone, Eq, Ord)]
pub struct Element {
    pub id: u64,
    pub html: String,
}

pub type Selector = selectors::parser::Selector<style::selector_parser::SelectorImpl>;

impl From<scraper::ElementRef<'_>> for Element {
    fn from(value: scraper::ElementRef) -> Self {
        fn get_start_tag(el: ElementRef<'_>) -> String {
            let name = el.value().name();
            let mut out = String::new();
            write!(&mut out, "<{name}").unwrap();
            for (k, v) in el.value().attrs() {
                write!(&mut out, " {k}=\"{v}\"").unwrap();
            }
            out.push('>');
            out
        } // thanks, ChatGPT

        let mut hasher = DefaultHasher::new();
        value.id().hash(&mut hasher);
        let id = hasher.finish();
 
        Self{
            id,
            html: get_start_tag(value),
        }
    }
}

impl PartialOrd for Element {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Hash for Element {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Borrowed forms of result structs
pub mod borrowed {
    use super::{Element, Selector};
    use smallvec::SmallVec;

    #[derive(Debug, Clone)]
    pub struct DocumentMatches<'a>(pub Vec<ElementMatches<'a>>);

    #[derive(Debug, Clone)]
    pub struct ElementMatches<'a> {
        pub element: Element,
        pub selectors: SelectorsOrSharedStyles<'a>, 
    }

    /// When we try to share styles, we circumvent selector matching entirely. The
    /// algorithm does not give us the selectors that matched, but rather, the
    /// element which this one is similar to. This enum can report this information,
    /// so we can trace back to the element we are sharing styles with and therefore
    /// the selectors that would have matched.
    #[derive(Clone, Debug)]
    pub enum SelectorsOrSharedStyles<'a> {
        Selectors(SmallVec<[&'a Selector; 16]>),
        SharedWithElement(u64),
    }
}

/// Owned forms of result structs
pub mod owned {
    use super::{Element, Selector};
    use super::borrowed::{DocumentMatches, ElementMatches, SelectorsOrSharedStyles};
    use smallvec::SmallVec;

    #[derive(Debug, Clone)]
    pub struct OwnedDocumentMatches(pub Vec<OwnedElementMatches>);

    impl From<DocumentMatches<'_>> for OwnedDocumentMatches {
        fn from(value: DocumentMatches<'_>) -> Self {
            Self(value.0.into_iter().map(OwnedElementMatches::from).collect())
        }
    }

    #[derive(Debug, Clone)]
    pub struct OwnedElementMatches {
        pub element: Element,
        pub selectors: OwnedSelectorsOrSharedStyles,
    }

    impl From<ElementMatches<'_>> for OwnedElementMatches {
        fn from(value: ElementMatches<'_>) -> Self {
            Self {
                element: value.element,
                selectors: value.selectors.into(),
            }
        }
    }

    #[derive(Clone, Debug)]
    pub enum OwnedSelectorsOrSharedStyles {
        Selectors(SmallVec<[Selector; 16]>),
        SharedWithElement(u64),
    }

    impl From<SelectorsOrSharedStyles<'_>> for OwnedSelectorsOrSharedStyles {
        fn from(value: SelectorsOrSharedStyles<'_>) -> Self {
            match value {
                SelectorsOrSharedStyles::Selectors(selectors) => Self::Selectors(selectors.into_iter().cloned().collect()),
                SelectorsOrSharedStyles::SharedWithElement(id) => Self::SharedWithElement(id),
            }
        }
    }
}

pub mod set {
    use std::collections::{HashMap, HashSet};

    use ::cssparser::ToCss as _;
    use serde::Serialize;

    use super::{Element, Selector};
    use super::owned::{OwnedDocumentMatches, OwnedElementMatches, OwnedSelectorsOrSharedStyles};
    use super::ser::{SerDocumentMatches, SerSelectorsOrSharedStyles};

    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    #[serde(into = "SerDocumentMatches")]
    pub struct SetDocumentMatches(pub HashMap<Element, SetSelectorsOrSharedStyles>);

    impl From<OwnedDocumentMatches> for SetDocumentMatches {
        fn from(OwnedDocumentMatches(v): OwnedDocumentMatches) -> Self {
            SetDocumentMatches(v.into_iter().map(|oem| {
                let OwnedElementMatches{ element, selectors } = oem;
                let set = selectors.into();
                (element, set)
            }).collect())
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    #[serde(into = "SerSelectorsOrSharedStyles")]
    pub enum SetSelectorsOrSharedStyles {
        Selectors(HashSet<String>),
        SharedWithElement(u64),
    }

    impl From<OwnedSelectorsOrSharedStyles> for SetSelectorsOrSharedStyles {
        fn from(value: OwnedSelectorsOrSharedStyles) -> Self {
            match value {
                OwnedSelectorsOrSharedStyles::Selectors(selectors) => SetSelectorsOrSharedStyles::Selectors(selectors.iter().map(Selector::to_css_string).collect()),
                OwnedSelectorsOrSharedStyles::SharedWithElement(id) => SetSelectorsOrSharedStyles::SharedWithElement(id),
            }
        }
    }
}

pub mod ser {
    use std::collections::{BTreeMap, BTreeSet};

    use serde::Serialize;

    use super::set::{SetDocumentMatches, SetSelectorsOrSharedStyles};

    #[derive(Clone, Debug, Serialize)]
    pub struct SerDocumentMatches(pub BTreeMap<SerElementKey, SerElementMatches>);

    impl From<SetDocumentMatches> for SerDocumentMatches {
        fn from(value: SetDocumentMatches) -> Self {
            SerDocumentMatches(
                value.0.into_iter().map(|(k, v)| {
                    (SerElementKey(k.id), SerElementMatches{ html: k.html, selectors: v.into() })
                }).collect()
            )
        }
    }

    #[derive(Clone, Debug, Serialize)]
    pub struct SerElementMatches {
        pub html: String,
        pub selectors: SerSelectorsOrSharedStyles,
    }

    #[derive(Clone, Debug, Serialize)]
    #[serde(untagged)]
    pub enum SerSelectorsOrSharedStyles {
        Selectors(BTreeSet<String>),
        SharedWithElement(u64),
    }

    impl From<SetSelectorsOrSharedStyles> for SerSelectorsOrSharedStyles {
        fn from(value: SetSelectorsOrSharedStyles) -> Self {
            match value {
                SetSelectorsOrSharedStyles::Selectors(selectors) => SerSelectorsOrSharedStyles::Selectors(BTreeSet::from_iter(selectors.into_iter())),
                SetSelectorsOrSharedStyles::SharedWithElement(id) => SerSelectorsOrSharedStyles::SharedWithElement(id),
            }
        }
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct SerElementKey(pub u64);

    impl Serialize for SerElementKey {
        fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: serde::Serializer
        {
            serializer.serialize_str(&format!("element_{}", self.0))
        }
    }
}
