use style::selector_map::SelectorMapElement;
use crate::ElementRef;

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
        self.node.parent().map(|n| ElementRef::wrap(n).unwrap()) // TODO: only Element parents, or could there be text/other node type parents as well? Does this only return Element parents? Should it?
    }

    fn borrow_data(&self) -> Option<atomic_refcell::AtomicRef<'_, style::data::ElementData>> {
        None // TODO: ElementData probably unnecessary?
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