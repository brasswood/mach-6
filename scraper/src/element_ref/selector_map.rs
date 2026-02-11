use style::selector_map::SelectorMapElement;
use crate::{ElementRef, Node};

impl SelectorMapElement for ElementRef<'_> {
    fn id(&self) -> Option<&style::Atom> {
        self.value().id_atom()
    }

    fn each_class<F>(&self, mut callback: F)
    where
        F: FnMut(&style::values::AtomIdent) {
        for class in self.value().classes_atom() {
            callback(class)
        }
    }

    fn each_attr_name<F>(&self, mut callback: F)
    where
        F: FnMut(&style::LocalName) {
        for (attr, _) in self.value().attrs_atom() {
            callback(&style::values::GenericAtomIdent(attr.local.clone()))
        }
    }

    fn local_name(&self) -> &web_atoms::LocalName {
        &self.value().name.local
    }

    fn state(&self) -> stylo_dom::ElementState {
        stylo_dom::ElementState::from_bits(0).unwrap() // TODO: This is probably right?
    }

    fn namespace(&self) -> &web_atoms::Namespace {
        &self.value().name.ns
    }

    fn traversal_parent(&self) -> Option<Self> {
        let parent_node = self.node.parent()?;
        match parent_node.value() {
            Node::Document => None,
            Node::Element(_) => Some(ElementRef::new(parent_node)),
            other => panic!("Did not expect parent of element to be {:?}.", other),
        }
    }

    fn borrow_data(&self) -> Option<atomic_refcell::AtomicRef<'_, style::data::ElementData>> {
        use std::sync::OnceLock;
        use style::data::{ElementData, ElementDataFlags, ElementStyles};
        use style::properties::style_structs::Font;
        use style::properties::ComputedValues;
        use style::style_resolver::{PrimaryStyle, ResolvedStyle};

        static DEFAULT_DATA: OnceLock<atomic_refcell::AtomicRefCell<ElementData>> = OnceLock::new();

        let cell = DEFAULT_DATA.get_or_init(|| {
            let default_font = Font::initial_values();
            let style = ComputedValues::initial_values_with_font_override(default_font);
            let primary = PrimaryStyle {
                style: ResolvedStyle(style),
                reused_via_rule_node: false,
                may_have_starting_style: false,
            };
            let mut data = ElementData {
                styles: ElementStyles {
                    primary: Some(primary.style.0),
                    ..Default::default()
                },
                ..Default::default()
            };
            data.flags.set(
                ElementDataFlags::PRIMARY_STYLE_REUSED_VIA_RULE_NODE,
                primary.reused_via_rule_node,
            );
            data.flags.set(
                ElementDataFlags::MAY_HAVE_STARTING_STYLE,
                primary.may_have_starting_style,
            );
            atomic_refcell::AtomicRefCell::new(data)
        });

        Some(cell.borrow())
    }

    fn query_container_size(
        &self,
        _display: &style::values::computed::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
        // TODO: I don't see anything to do with container queries
        // in trait Element that I can reference. Is what I have here OK?
        euclid::Size2D::new(None, None) 
    }
}
