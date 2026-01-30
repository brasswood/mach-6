/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use ::cssparser::ToCss as _;
use derive_more::Display;
use parse::HtmlFile;
use rustc_hash::FxBuildHasher;
use selectors::Element as _;
use style::Atom;
use style::animation::DocumentAnimationSet;
use style::bloom::StyleBloom;
use style::context::RegisteredSpeculativePainter;
use style::context::SharedStyleContext;
use style::context::StyleSystemOptions;
use style::context::ThreadLocalStyleContext;
use style::selector_parser::SnapshotMap;
use style::shared_lock::StylesheetGuards;
use style::traversal_flags::TraversalFlags;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::hash::DefaultHasher;
use std::hash::Hasher as _;
use std::path::PathBuf;
use std::io;
use std::path::Path;
use scraper::ElementRef;
use scraper::Html;
use selectors::context::SelectorCaches;
use selectors::matching;
use style::context::{StyleContext, RegisteredSpeculativePainters};
use style::media_queries::Device;
use style::media_queries::MediaType;
use style::properties::ComputedValues;
use style::properties::style_structs::Font;
use style::queries::values::PrefersColorScheme;
use style::rule_tree::CascadeLevel;
use style::selector_map::SelectorMapElement as _;
use style::servo::media_queries::FontMetricsProvider;
use style::servo_arc::Arc;
use style::shared_lock::SharedRwLock;
use style::stylist::CascadeData;
use style::stylist::Rule;
use style::stylist::Stylist;
use style::values::AtomIdent;
use style::values::computed::font::GenericFontFamily;
use style::values::computed::{Length, CSSPixelLength, font::QueryFontMetricsFlags};
use std::hash::Hash;
use std::result;
use thiserror::Error;
use serde::Serialize;
use style::selector_map::SelectorMap;
use style::sharing::StyleSharingTarget;
use smallvec::SmallVec;

mod parse;

pub use parse::get_documents_and_selectors;

#[derive(Debug, Display, Clone, Copy)]
pub enum Algorithm {
    Naive,
    WithSelectorMap,
    WithSelectorMapAndBloomFilter,
}

pub fn do_all_websites(websites: &Path, algorithm: Algorithm) -> Result<impl Iterator<Item = Result<(String, SetDocumentMatches)>>> {
    Ok(get_documents_and_selectors(websites)?
        .map(move |r| {
            r.map(|(w, h, s)| {
                let matches = match algorithm {
                    Algorithm::Naive => OwnedDocumentMatches::from(match_selectors(&h, &s)),
                    Algorithm::WithSelectorMap => {
                        let selector_map = build_selector_map(&s);
                        match_selectors_with_selector_map(&h, &selector_map)
                    }
                    Algorithm::WithSelectorMapAndBloomFilter => {
                        let selector_map = build_selector_map(&s);
                        match_selectors_with_bloom_filter(&h, &selector_map)
                    }
                };
                (w, SetDocumentMatches::from(matches))
            })
        })
    )
}

#[derive(Error, Debug)]
pub struct Error {
    pub path: Option<PathBuf>,
    pub error: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    Io(io::Error),
    MultipleHtmlFiles(Vec<HtmlFile>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.error {
            ErrorKind::Io(io) => {
                write!(f, "io error: {io}")?;
                if let Some(path) = &self.path {
                    write!(f, " path: {}", path.display())?;
                }
                Ok(())
            },
            ErrorKind::MultipleHtmlFiles(v) => {
                writeln!(f, "website {} has more than one html file:", self.path.as_ref().unwrap().display())?;
                for HtmlFile(h) in v {
                    writeln!(f, "{}", h.display())?;
                }
                Ok(())
            }
        }
    }
}

impl Error {
    pub fn is_io_and(&self, f: impl FnOnce(&io::Error) -> bool) -> bool {
        match &self.error {
            ErrorKind::Io(e) => f(&e),
            _ => false
        }
    }

    pub fn is_html_and(&self, f: impl FnOnce(&Vec<HtmlFile>) -> bool) -> bool {
        match &self.error {
            ErrorKind::MultipleHtmlFiles(v) => f(&v),
            _ => false,
        }
    }
}

pub trait IntoErrorExt<T> {
    fn into_error(self, path: Option<PathBuf>) -> Error;
}

impl<T> IntoErrorExt<T> for io::Error {
    fn into_error(self, path: Option<PathBuf>) -> Error {
        Error {
            path,
            error: ErrorKind::Io(self),
        }
    }
}

pub type Result<T> = result::Result<T, Error>;

pub trait IntoResultExt<T> {
    fn into_result(self, path: Option<PathBuf>) -> Result<T>;
}

impl<T> IntoResultExt<T> for io::Result<T> {
    fn into_result(self, path: Option<PathBuf>) -> Result<T> {
        self.map_err(|e| <io::Error as IntoErrorExt<T>>::into_error(e, path))
    }
}

#[derive(Debug)]
struct TestFontMetricsProvider;

impl FontMetricsProvider for TestFontMetricsProvider {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &Font,
        _base_size: CSSPixelLength,
        _flags: QueryFontMetricsFlags,
    ) -> style::font_metrics::FontMetrics {
        style::font_metrics::FontMetrics {
            x_height: Some(CSSPixelLength::new(1.0)),
            zero_advance_measure: Some(CSSPixelLength::new(1.0)),
            cap_height: Some(CSSPixelLength::new(1.0)),
            ic_width: Some(CSSPixelLength::new(1.0)),
            ascent: CSSPixelLength::new(1.0),
            script_percent_scale_down: None,
            script_script_percent_scale_down: None,
        } // TODO: Idk
    }

    fn base_size_for_generic(&self, _generic: GenericFontFamily) -> Length {
        CSSPixelLength::new(1.0)
    }
}

fn mock_device() -> Device {
    let default_font = Font::initial_values();
    Device::new(
        MediaType::screen(),
        matching::QuirksMode::NoQuirks,
        euclid::Size2D::new(1200.0, 800.0),
        euclid::Scale::new(1.0),
        Box::new(TestFontMetricsProvider),
        ComputedValues::initial_values_with_font_override(default_font),
        PrefersColorScheme::Light,
    )
}

#[derive(Debug, Clone, Eq, Ord)]
pub struct Element {
    id: u64,
    html: String,
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

#[derive(Debug, Clone)]
pub struct ElementMatches<'a> {
    element: Element,
    selectors: SelectorsOrSharedStyles<'a>, 
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

#[derive(Debug, Clone)]
pub struct OwnedElementMatches {
    element: Element,
    selectors: OwnedSelectorsOrSharedStyles,
}

impl From<ElementMatches<'_>> for OwnedElementMatches {
    fn from(value: ElementMatches<'_>) -> Self {
        Self {
            element: value.element,
            selectors: value.selectors.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentMatches<'a>(Vec<ElementMatches<'a>>);

#[derive(Debug, Clone)]
pub struct OwnedDocumentMatches(Vec<OwnedElementMatches>);

impl From<DocumentMatches<'_>> for OwnedDocumentMatches {
    fn from(value: DocumentMatches<'_>) -> Self {
        Self(value.0.into_iter().map(OwnedElementMatches::from).collect())
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(into = "SerDocumentMatches")]
pub struct SetDocumentMatches(HashMap<Element, SetSelectorsOrSharedStyles>);

impl From<OwnedDocumentMatches> for SetDocumentMatches {
    fn from(OwnedDocumentMatches(v): OwnedDocumentMatches) -> Self {
        SetDocumentMatches(v.into_iter().map(|oem| {
            let OwnedElementMatches{ element, selectors } = oem;
            let set = selectors.into();
            (element, set)
        }).collect())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SerElementKey(u64);

impl Serialize for SerElementKey {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        serializer.serialize_str(&format!("element_{}", self.0))
    }
}

#[derive(Clone, Debug, Serialize)]
struct SerDocumentMatches(BTreeMap<SerElementKey, SerElementMatches>);

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
#[serde(untagged)]
enum SerSelectorsOrSharedStyles {
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

#[derive(Clone, Debug, Serialize)]
struct SerElementMatches {
    html: String,
    selectors: SerSelectorsOrSharedStyles,
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
        let matched_selectors = selectors.iter().filter(|s| matching::matches_selector(s, 0, None, &element, &mut context)).collect();
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

pub fn match_selectors_with_selector_map(document: &Html, selector_map: &SelectorMap<Rule>) -> OwnedDocumentMatches {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>, 
        matches: &mut Vec<OwnedElementMatches>,
        selector_map: &SelectorMap<Rule>,
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
        // 1.2: Use the selector map to get matching rules
        let mut matched_selectors = SmallVec::new();
        selector_map.get_all_matching_rules(
            element,
            element, // TODO: ????
            &mut SmallVec::new(),
            &mut Some(&mut matched_selectors),
            &mut context,
            CascadeLevel::UANormal, // TODO: ??????
            &CascadeData::new(),
            &Stylist::new(mock_device(), matching::QuirksMode::NoQuirks)
        );
        matches.push(OwnedElementMatches{ element: Element::from(element), selectors: OwnedSelectorsOrSharedStyles::Selectors(matched_selectors) });
        // 2. traverse children
        for child in element.child_elements() {
            preorder_traversal(child, matches, selector_map, caches);
        }
    }

    let mut caches = SelectorCaches::default();
    let mut result = Vec::new();
    preorder_traversal(document.root_element(), &mut result, selector_map, &mut caches);
    OwnedDocumentMatches(result)
}

pub fn match_selectors_with_bloom_filter(document: &Html, selector_map: &SelectorMap<Rule>) -> OwnedDocumentMatches {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>, 
        element_depth: usize,
        matches: &mut Vec<OwnedElementMatches>,
        selector_map: &SelectorMap<Rule>,
        style_bloom: &mut StyleBloom<ElementRef<'a>>,
        caches: &mut SelectorCaches,
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
        selector_map.get_all_matching_rules(
            element,
            element, // TODO: ????
            &mut SmallVec::new(),
            &mut Some(&mut matched_selectors),
            &mut context,
            CascadeLevel::UANormal, // TODO: ??????
            &CascadeData::new(),
            &Stylist::new(mock_device(), matching::QuirksMode::NoQuirks)
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
            preorder_traversal(child, element_depth+1, matches, selector_map, style_bloom, caches);
        }
    }
    let mut bloom_filter = StyleBloom::new();
    let mut caches = SelectorCaches::default();
    let mut result = Vec::new();
    preorder_traversal(document.root_element(), 0, &mut result, selector_map, &mut bloom_filter, &mut caches);
    OwnedDocumentMatches(result)
}

#[derive(Debug, Clone, Copy)]
struct MyRegisteredSpeculativePainters;
impl RegisteredSpeculativePainters for MyRegisteredSpeculativePainters {
    /// Look up a speculative painter
    fn get(&self, _name: &Atom) -> Option<&dyn RegisteredSpeculativePainter> {
        panic!("Oh, WOW. We actually used RegisteredSpeculativePainters and I have to do something.");
    }
}

pub fn match_selectors_with_style_sharing(document: &Html, selector_map: &SelectorMap<Rule>) -> OwnedDocumentMatches {
    fn preorder_traversal<'a>(
        element: ElementRef<'a>, 
        element_depth: usize,
        context: &mut StyleContext<ElementRef<'a>>,
        matches: &mut Vec<OwnedElementMatches>,
        selector_map: &SelectorMap<Rule>,
        style_bloom: &mut StyleBloom<ElementRef<'a>>,
        caches: &mut SelectorCaches,
    ) {
        // 1. do thing
        // 1.1: update the bloom filter with the current element
        style_bloom.insert_parents_recovering(element, element_depth);
        // 1.2: Check if we can share styles
        let mut target = StyleSharingTarget::new(element);
        match target.share_style_if_possible(context) {
            Some((other_element, _shared_styles)) => {
                // If we can share styles, do that.
                let element = Element::from(element);
                let other_element = Element::from(other_element);
                matches.push(OwnedElementMatches{ element, selectors: OwnedSelectorsOrSharedStyles::SharedWithElement(other_element.id) })
            },
            None => {
                // If we can't share styles, go through the selector map and bloom filter.
                // 1.2.1: create a MatchingContext (after updating style_bloom to avoid borrow check error)
                let mut matching_context = matching::MatchingContext::new(
                    matching::MatchingMode::Normal,
                    Some(style_bloom.filter()),
                    caches,
                    matching::QuirksMode::NoQuirks,
                    matching::NeedsSelectorFlags::No,
                    matching::MatchingForInvalidation::No,
                );
                // 1.2.2: Use the selector map to get matching rules
                let mut matched_selectors = SmallVec::new();
                selector_map.get_all_matching_rules(
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
            preorder_traversal(child, element_depth+1, context, matches, selector_map, style_bloom, caches);
        }
    }
    let mut bloom_filter = StyleBloom::new();
    let stylist = Stylist::new(mock_device(), matching::QuirksMode::NoQuirks);
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
        registered_speculative_painters: &MyRegisteredSpeculativePainters,
    };
    let mut style_context = StyleContext {
        shared: &shared_style_context,
        thread_local: &mut ThreadLocalStyleContext::new(),
    };
    let mut caches = SelectorCaches::default();
    let mut result = Vec::new();
    preorder_traversal(document.root_element(), 0, &mut style_context, &mut result, selector_map, &mut bloom_filter, &mut caches);
    OwnedDocumentMatches(result)
}