//! HTML nodes.

use std::fmt;
use std::ops::Deref;
use std::slice::Iter as SliceIter;

use crate::{CaseSensitivity, StrTendril};
use atomic_refcell::AtomicRefCell;
use html5ever::{Attribute, LocalName, QualName};
use style::data::ElementData;
use style::servo_arc::Arc;
use style::{Atom, values::GenericAtomIdent};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use style::properties::declaration_block::parse_style_attribute;
use style::context::QuirksMode;
use style::stylesheets::{CssRuleType, UrlExtraData};
use style::shared_lock::{Locked, SharedRwLock};

/// An HTML node.
// `Element` is usally the most common variant and hence boxing it
// will most likely not improve performance overall.
#[allow(variant_size_differences)]
#[derive(PartialEq, Eq)]
pub enum Node {
    /// The document root.
    Document,

    /// The fragment root.
    Fragment,

    /// A doctype.
    Doctype(Doctype),

    /// A comment.
    Comment(Comment),

    /// Text.
    Text(Text),

    /// An element.
    Element(Element),

    /// A processing instruction.
    ProcessingInstruction(ProcessingInstruction),
}

impl Node {
    /// Returns true if node is the document root.
    pub fn is_document(&self) -> bool {
        matches!(*self, Node::Document)
    }

    /// Returns true if node is the fragment root.
    pub fn is_fragment(&self) -> bool {
        matches!(*self, Node::Fragment)
    }

    /// Returns true if node is a doctype.
    pub fn is_doctype(&self) -> bool {
        matches!(*self, Node::Doctype(_))
    }

    /// Returns true if node is a comment.
    pub fn is_comment(&self) -> bool {
        matches!(*self, Node::Comment(_))
    }

    /// Returns true if node is text.
    pub fn is_text(&self) -> bool {
        matches!(*self, Node::Text(_))
    }

    /// Returns true if node is an element.
    pub fn is_element(&self) -> bool {
        matches!(*self, Node::Element(_))
    }

    /// Returns self as a doctype.
    pub fn as_doctype(&self) -> Option<&Doctype> {
        match *self {
            Node::Doctype(ref d) => Some(d),
            _ => None,
        }
    }

    /// Returns self as a comment.
    pub fn as_comment(&self) -> Option<&Comment> {
        match *self {
            Node::Comment(ref c) => Some(c),
            _ => None,
        }
    }

    /// Returns self as text.
    pub fn as_text(&self) -> Option<&Text> {
        match *self {
            Node::Text(ref t) => Some(t),
            _ => None,
        }
    }

    /// Returns self as an element.
    pub fn as_element(&self) -> Option<&Element> {
        match *self {
            Node::Element(ref e) => Some(e),
            _ => None,
        }
    }

    /// Returns self as an element.
    pub fn as_processing_instruction(&self) -> Option<&ProcessingInstruction> {
        match *self {
            Node::ProcessingInstruction(ref pi) => Some(pi),
            _ => None,
        }
    }
}

// Always use one line.
impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            Node::Document => write!(f, "Document"),
            Node::Fragment => write!(f, "Fragment"),
            Node::Doctype(ref d) => write!(f, "Doctype({d:?})"),
            Node::Comment(ref c) => write!(f, "Comment({c:?})"),
            Node::Text(ref t) => write!(f, "Text({t:?})"),
            Node::Element(ref e) => write!(f, "Element({e:?})"),
            Node::ProcessingInstruction(ref pi) => write!(f, "ProcessingInstruction({pi:?})"),
        }
    }
}

/// A doctype.
#[derive(Clone, PartialEq, Eq)]
pub struct Doctype {
    /// The doctype name.
    pub name: StrTendril,

    /// The doctype public ID.
    pub public_id: StrTendril,

    /// The doctype system ID.
    pub system_id: StrTendril,
}

impl Doctype {
    /// Returns the doctype name.
    pub fn name(&self) -> &str {
        self.name.deref()
    }

    /// Returns the doctype public ID.
    pub fn public_id(&self) -> &str {
        self.public_id.deref()
    }

    /// Returns the doctype system ID.
    pub fn system_id(&self) -> &str {
        self.system_id.deref()
    }
}

impl fmt::Debug for Doctype {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "<!DOCTYPE {} PUBLIC {:?} {:?}>",
            self.name(),
            self.public_id(),
            self.system_id()
        )
    }
}

/// An HTML comment.
#[derive(Clone, PartialEq, Eq)]
pub struct Comment {
    /// The comment text.
    pub comment: StrTendril,
}

impl Deref for Comment {
    type Target = str;

    fn deref(&self) -> &str {
        self.comment.deref()
    }
}

impl fmt::Debug for Comment {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "<!-- {:?} -->", self.deref())
    }
}

/// HTML text.
#[derive(Clone, PartialEq, Eq)]
pub struct Text {
    /// The text.
    pub text: StrTendril,
}

impl Deref for Text {
    type Target = str;

    fn deref(&self) -> &str {
        self.text.deref()
    }
}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{:?}", self.deref())
    }
}

/// A Map of attributes that preserves the order of the attributes.
#[cfg(feature = "deterministic")]
pub type Attributes = indexmap::IndexMap<QualName, StrTendril>;

/// A Map of attributes that doesn't preserve the order of the attributes.
/// Please enable the `deterministic` feature for order-preserving
/// (de)serialization.
#[cfg(not(feature = "deterministic"))]
pub type Attributes = Vec<(QualName, Atom)>;

/// An HTML element.
pub struct Element {
    /// The element name.
    pub name: QualName,

    /// The element attributes.
    pub attrs: Attributes,

    /// The PropertyDeclarationBlock, parsed on construction from the 'style' attribute
    pub style_block: Arc<Locked<style::properties::PropertyDeclarationBlock>>,

    style_block_lock: SharedRwLock,

    element_data: Option<AtomicRefCell<ElementData>>,

    id: OnceCell<Option<Atom>>,

    classes: OnceCell<Box<[style::values::AtomIdent]>>,
}

struct InternedStyleBlock {
    lock: SharedRwLock,
    block: Arc<Locked<style::properties::PropertyDeclarationBlock>>,
}

fn intern_style_block(style_attr: &str) -> (SharedRwLock, Arc<Locked<style::properties::PropertyDeclarationBlock>>) {
    static INTERNER: OnceLock<Mutex<HashMap<String, InternedStyleBlock>>> = OnceLock::new();
    let interner = INTERNER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = interner.lock().unwrap();

    if let Some(entry) = map.get(style_attr) {
        return (entry.lock.clone(), entry.block.clone());
    }

    let style_block = parse_style_attribute(
        style_attr,
        &UrlExtraData::from(url::Url::parse("about:blank").unwrap()),
        None,
        QuirksMode::NoQuirks,
        CssRuleType::Style,
    );
    let lock = SharedRwLock::new();
    let block = Arc::new(lock.wrap(style_block));

    map.insert(
        style_attr.to_owned(),
        InternedStyleBlock {
            lock: lock.clone(),
            block: block.clone(),
        },
    );

    (lock, block)
}

impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.attrs == other.attrs && self.id == other.id && self.classes == other.classes
    }
}

impl Eq for Element {}

impl Element {
    #[doc(hidden)]
    pub fn new(name: QualName, attributes: Vec<Attribute>) -> Self {
        #[allow(unused_mut)]
        let mut attrs = attributes
            .into_iter()
            .map(|attr| (attr.name, Atom::from(&*attr.value)))
            .collect::<Attributes>();


        #[cfg(not(feature = "deterministic"))]
        attrs.sort_unstable_by(|lhs, rhs| lhs.0.cmp(&rhs.0));

        let style_attr = attrs.iter().find(|attr| &*attr.0.local == "style").map(|attr| &*attr.1).unwrap_or("");
        let (style_block_lock, style_block) = intern_style_block(style_attr);

        Element {
            attrs,
            name,
            style_block,
            style_block_lock,
            element_data: None,
            id: OnceCell::new(),
            classes: OnceCell::new(),
        }
    }

    /// Returns the element name.
    pub fn name(&self) -> &str {
        self.name.local.deref()
    }

    pub(crate) fn id_atom(&self) -> Option<&Atom> {
        self.id
            .get_or_init(|| {
                self.attrs
                    .iter()
                    .find(|(name, _)| name.local.as_ref() == "id")
                    .map(|(_, value)| value.clone())
            })
            .as_ref()
    }

    /// Returns the element ID.
    pub fn id(&self) -> Option<&str> {
        self.id_atom().map(|v| &**v)
    }

    /// Returns true if element has the class.
    pub fn has_class(&self, class: &str, case_sensitive: CaseSensitivity) -> bool {
        self.classes()
            .any(|c| case_sensitive.eq(c.as_bytes(), class.as_bytes()))
    }

    pub(crate) fn classes_atom(&self) -> ClassesAtom<'_> {
        let classes = self.classes.get_or_init(|| {
            let mut classes = self
                .attrs
                .iter()
                .filter(|(name, _)| name.local.as_ref() == "class")
                .map(|(_, value)| value)
                .collect::<Vec<_>>();

            classes.sort_unstable();
            classes.dedup();

            classes
                .into_iter()
                .map(|v| GenericAtomIdent(v.clone()))
                .collect::<Vec<_>>()
                .into_boxed_slice()
        });

        ClassesAtom {
            inner: classes.iter()
        }
    }

    /// Returns an iterator over the element's classes.
    pub fn classes(&self) -> Classes<'_> {

        Classes {
            inner: self.classes_atom(),
        }
    }
    
    pub(crate) fn attrs_atom(&self) -> AttributesIter<'_> {
        self.attrs.iter()
    }

    /// Returns the value of an attribute.
    pub fn attr(&self, attr: &str) -> Option<&str> {
        let qualname = QualName::new(None, ns!(), LocalName::from(attr));

        #[cfg(not(feature = "deterministic"))]
        let value = self
            .attrs
            .binary_search_by(|attr| attr.0.cmp(&qualname))
            .ok()
            .map(|idx| &*self.attrs[idx].1);

        #[cfg(feature = "deterministic")]
        let value = self.attrs.get(&qualname).map(Deref::deref);

        value
    }

    /// Returns an iterator over the element's attributes.
    pub fn attrs(&self) -> Attrs<'_> {
        Attrs {
            inner: self.attrs.iter(),
        }
    }
}

/// Iterator over classes.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct Classes<'a> {
    inner: ClassesAtom<'a>,
}

impl<'a> Iterator for Classes<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        self.inner.next().map(|v| &***v)
        // first deref: &GenericAtomIdent -> GenericAtomIdent
        // second deref: GenericAtomIdent -> Atom
        // third deref: Atom -> str
    }
}

#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub(crate) struct ClassesAtom<'a> {
    inner: SliceIter<'a, style::values::AtomIdent>,
}

impl<'a> Iterator for ClassesAtom<'a> {
    type Item = &'a style::values::AtomIdent;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// An iterator over a node's attributes.
#[cfg(feature = "deterministic")]
pub type AttributesIter<'a> = indexmap::map::Iter<'a, QualName, StrTendril>;

/// An iterator over a node's attributes.
#[cfg(not(feature = "deterministic"))]
pub type AttributesIter<'a> = SliceIter<'a, (QualName, Atom)>;

/// Iterator over attributes.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct Attrs<'a> {
    inner: AttributesIter<'a>,
}

impl<'a> Iterator for Attrs<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<(&'a str, &'a str)> {
        self.inner.next().map(|(k, v)| (k.local.deref(), v.deref()))
    }
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "<{}", self.name())?;
        for (key, value) in self.attrs() {
            write!(f, " {key}={value:?}")?;
        }
        write!(f, ">")
    }
}

/// HTML Processing Instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingInstruction {
    /// The PI target.
    pub target: StrTendril,
    /// The PI data.
    pub data: StrTendril,
}

impl Deref for ProcessingInstruction {
    type Target = str;

    fn deref(&self) -> &str {
        self.data.deref()
    }
}

pub(crate) mod serializable;
