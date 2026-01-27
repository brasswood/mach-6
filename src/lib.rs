/* Copyright 2025 Andrew Riachi
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use ::cssparser::ToCss as _;
use derive_more::Display;
use serde::de;
use selectors::Element as _;
use style::bloom::StyleBloom;
use style::context::SharedStyleContext;
use style::context::ThreadLocalStyleContext;
use style::traversal_flags::TraversalFlags;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs::DirEntry;
use std::fs::ReadDir;
use std::hash::DefaultHasher;
use std::hash::Hasher as _;
use std::path::PathBuf;
use std::io;
use std::fs;
use std::path::Path;
use std::time::Instant;
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
use style::sharing::StyleSharingElement;
use smallvec::SmallVec;

pub mod cssparser;

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

pub fn get_documents_and_selectors(websites_path: &Path) -> Result<impl Iterator<Item = Result<(String, Html, Vec<Selector>)>>> {
    let websites_dir = fs::read_dir(&websites_path).into_result(Some(websites_path.to_path_buf()))?; 
    let websites = get_websites_dirs(websites_dir);
    let documents = websites.filter_map(|r: io::Result<PathBuf>| {
        r.into_result(Some(websites_path.to_path_buf()))
            .and_then(|d: PathBuf| parse_website(&d).map(|html: Option<Html>| html.map(|html| (d, html)))).transpose()
    });
    let documents_selectors = documents.map(|r: Result<(PathBuf, Html)>| {
        r.map(|(base, document): (PathBuf, Html)| {
            let style_tag_selector = scraper::Selector::parse("style").unwrap();
            let style_tags = document.select(&style_tag_selector);
            let selectors_from_style_tags = style_tags.filter_map(|elt| {
                match parse_stylesheet(&elt.inner_html()) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        eprintln!("WARNING: error parsing a style tag from website {}: {}. Skipping.", base.display(), e);
                        None
                    }
                }
            })
            .flatten();

            let stylesheets: Vec<CssFile> = get_stylesheet_paths(&document); // TODO: also get from <style> in main html
            let selectors_from_stylesheets = stylesheets.into_iter()
                .filter_map(|f| {
                    match parse_css_file(&base, &f) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            eprintln!("WARNING: error parsing CSS file {}: {}. Skipping.", f.0.display(), e);
                            None
                        },
                    }
                })
                .flatten();
            let selectors = selectors_from_style_tags.chain(selectors_from_stylesheets).collect();
            (base.file_name().unwrap().to_str().unwrap().to_owned(), document, selectors)
        })
    });
    Ok(documents_selectors)
}

fn get_websites_dirs(websites: ReadDir) -> impl Iterator<Item = io::Result<PathBuf>> {
    websites.filter_map(|website| {
        website.map(|website| {
            let website_path = website.path();
            if website_path.is_dir() {
                Some(website_path)
            } else {
                eprintln!("WARNING: ignoring {} because it is not a directory", website_path.display());
                None
            }
        }).transpose()
    })
}

fn parse_website(website: &Path)-> Result<Option<Html>> {
    let main = get_main_html(website)?;
    main.map(parse_main_html).transpose()
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
pub struct HtmlFile(PathBuf);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
pub struct CssFile(PathBuf);

fn get_main_html(website: &Path) -> Result<Option<HtmlFile>> {
    let website_path = Some(website.to_path_buf());
    let files = fs::read_dir(website).into_result(website_path.clone())?;
    let f = |entry: DirEntry| {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("html") {
            Some(HtmlFile(PathBuf::from(path)))
        } else {
            None
        }
    };
    let mut found: Vec<HtmlFile> = files.collect::<io::Result<Vec<_>>>().into_result(website_path)?.into_iter().filter_map(f).collect();
    match found.len() {
        0 => {
            eprintln!("WARNING: ignoring {}, no html file found", website.display());
            Ok(None)
        },
        1 => Ok(Some(found.pop().unwrap())),
        _ => Err(Error{path: Some(website.to_path_buf()), error: ErrorKind::MultipleHtmlFiles(found) })  
    }
}

fn parse_main_html(HtmlFile(website): HtmlFile) -> Result<Html> {
    let contents = fs::read_to_string(&website).into_result(Some(website))?;
    Ok(Html::parse_document(&contents))
}

/// Returns the relative paths of stylesheets referenced by the given document.
fn get_stylesheet_paths(document: &Html) -> Vec<CssFile> {
    let selector = scraper::Selector::parse(r#"link[rel="stylesheet"]"#).unwrap();
    document.select(&selector).filter_map(|elt| {
        let Some(path) = elt.attr("href") else {
            eprintln!("WARNING: Found no href attribute in link element: {}. Skipping.", elt.html());
            return None;
        };
        Some(CssFile(PathBuf::from(path)))
    }).collect()
}

pub type Selector = selectors::parser::Selector<style::selector_parser::SelectorImpl>;

// TODO: returning iterator from these would probably be ideal.
fn parse_css_file(base: &Path, CssFile(stylesheet_path): &CssFile) -> Result<Vec<Selector>> {
    let full_path = base.join(stylesheet_path);
    let css = fs::read_to_string(&full_path).into_result(Some(full_path))?;
    parse_stylesheet(&css)
}

fn parse_stylesheet(css: &str) -> Result<Vec<Selector>> {
    let res = cssparser::get_all_selectors(&css)
        .into_iter()
        .filter_map(|r| {
            r.ok().flatten().map(|sel_list| sel_list.selectors.slice().iter().cloned().collect::<Vec<_>>().into_iter())
        })
        .flatten()
        .collect();
    Ok(res)
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

#[derive(Debug, Clone)]
pub struct ElementMatches<'a> {
    element: Element,
    selectors: SmallVec<[&'a Selector; 16]>,
}

#[derive(Debug, Clone)]
pub struct OwnedElementMatches {
    element: Element,
    selectors: SmallVec<[Selector; 16]>,
}

impl From<ElementMatches<'_>> for OwnedElementMatches {
    fn from(value: ElementMatches<'_>) -> Self {
        Self {
            element: value.element,
            selectors: value.selectors.into_iter().cloned().collect(),
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
#[serde(into = "SerDocumentMatches")]
pub struct SetDocumentMatches(HashMap<Element, HashSet<String>>);

impl From<OwnedDocumentMatches> for SetDocumentMatches {
    fn from(OwnedDocumentMatches(v): OwnedDocumentMatches) -> Self {
        SetDocumentMatches(v.into_iter().map(|oem| {
            let OwnedElementMatches{ element, selectors } = oem;
            let set = selectors.iter().map(Selector::to_css_string).collect();
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
                let selectors = BTreeSet::from_iter(v.into_iter());
                (SerElementKey(k.id), SerElementMatches{ html: k.html, selectors })
            }).collect()
        )
    }
}

#[derive(Clone, Debug, Serialize)]
struct SerElementMatches {
    html: String,
    selectors: BTreeSet<String>,
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
        matches.push(ElementMatches{ element: Element::from(element), selectors: matched_selectors });
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
        matches.push(OwnedElementMatches{ element: Element::from(element), selectors: matched_selectors });
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
        matches.push(OwnedElementMatches{ element: Element::from(element), selectors: matched_selectors });
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
    fn get(&self, name: &Atom) -> Option<&dyn RegisteredSpeculativePainter> {
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
        let mut target = StyleSharingElement::new(element);
        // 1.3: create a MatchingContext (after updating style_bloom to avoid borrow check error)
        let mut matching_context = matching::MatchingContext::new(
            matching::MatchingMode::Normal,
            Some(style_bloom.filter()),
            caches,
            matching::QuirksMode::NoQuirks,
            matching::NeedsSelectorFlags::No,
            matching::MatchingForInvalidation::No,
        );
        // 1.4: Use the selector map to get matching rules
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
        matches.push(OwnedElementMatches{ element: Element::from(element), selectors: matched_selectors });
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
    let shared_style_context = SharedStyleContext {
        stylist: &stylist,
        visited_styles_enabled: true,
        options: StyleSystemOptions {
            disable_style_sharing_cache: false,
            dump_style_statistics: false, // TODO: maybe change this later
            style_statistics_threshold: 0, // TODO: maybe change this later
        },
        guards: StyleSheetGuards {
            author: SharedRwLockReadGuard(SharedRwLock::new()),
            ua_or_user: SharedRwLockReadGuard(SharedRwLock::new()),
        },
        current_time_for_animations: 0.0,
        traversal_flags: TraversalFlags::empty(),
        snapshot_map: SnapshotMap::new(),
        animations: DocumentAnimationSet {
            sets: Arc::new(RwLock::new(FxHashMap::new())),
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

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};
    use crate::{CssFile, IntoResultExt, get_main_html, get_stylesheet_paths, parse_main_html};

    /// In all of these tests:
    ///   - Err() represents an unexpected error occurring during the test
    ///   - panic represents a test failure


    #[test]
    fn ensures_main_html_exists() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        let is_err = get_main_html(website_path).is_ok_and(|h| h.is_none());
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn ensures_not_multiple_main_html() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        for i in 1..=2 {
            let html_path = website_path.join(format!("{i}.html"));
            fs::File::create_new(&html_path).into_result(Some(html_path))?;
        }
        let is_err = get_main_html(website_path).is_err_and(|e| e.is_html_and(|_| true));
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn ensures_main_html_is_file() -> std::io::Result<()> {
        let website_dir = tempfile::tempdir()?;
        let website_path = website_dir.path();
        fs::create_dir(website_path.join("index.html"))?;
        let main_res = get_main_html(website_path);
        let is_err = matches!(main_res, Ok(None));
        assert!(is_err);
        Ok(())
    }

    #[test]
    fn parses_main_html() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        fs::write(website_path.join("index.html"), "<html><body><h1>Hello, World!</h1></body></html>").into_result(Some(website_path.to_path_buf()))?;
        println!("{:?}", website_path);
        let main_html = get_main_html(website_path)?.unwrap();
        parse_main_html(main_html).unwrap();
        Ok(())
    }

    #[test]
    fn gets_stylesheet_paths() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        fs::write(website_path.join("index.html"), r#"<html><head><link rel="stylesheet" href="style1.css"><link rel="stylesheet" href="style2.css"></head><body><h1>Hello, World!</h1></body></html>"#)
            .into_result(Some(website_path.to_path_buf()))?;
        let main_html = get_main_html(website_path)?.unwrap();
        let document = parse_main_html(main_html)?;
        let mut stylesheets = get_stylesheet_paths(&document);
        let mut expected: Vec<_> = vec!["style1.css", "style2.css"]
            .into_iter()
            .map(|s| CssFile(PathBuf::from(s)))
            .collect();
        stylesheets.sort();
        expected.sort();
        assert_eq!(stylesheets, expected);
        Ok(())
    }

    #[test]
    fn excludes_non_stylesheet_paths() -> super::Result<()> {
        let website_dir = tempfile::tempdir().into_result(None)?;
        let website_path = website_dir.path();
        let index_html_path = website_path.join("index.html");
        fs::write(&index_html_path, r#"<html><head><link rel="stylesheet" href="style1.css"><link rel="stylesheet" href="style2.css"><link rel="prerender" href="boogeyman"></head><body><h1>Hello, World!</h1></body></html>"#).into_result(Some(index_html_path))?;
        let main_html = get_main_html(website_path)?.unwrap();
        let document = parse_main_html(main_html)?;
        let mut stylesheets = get_stylesheet_paths(&document);
        let mut expected: Vec<_> = vec!["style1.css", "style2.css"]
            .into_iter()
            .map(|s| CssFile(PathBuf::from(s)))
            .collect();
        stylesheets.sort();
        expected.sort();
        assert_eq!(stylesheets, expected);
        Ok(())
    }
}