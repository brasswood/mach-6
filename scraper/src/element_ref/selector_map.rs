use style::selector_map::SelectorMapElement;
use crate::ElementRef;
use log_once::warn_once;

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

    fn each_attr_name<F>(&self, callback: F)
    where
        F: FnMut(&style::LocalName) {
        warn_once!("WARNING: <ElementRef as SelectorMapElement>::each_attr_name unimplemented.");
    }

    fn local_name(&self) -> &web_atoms::LocalName {
        warn_once!("WARNING: <ElementRef as SelectorMapElement>::local_name unimplemented.");
        Box::leak(Box::new(web_atoms::LocalName::from("")))
    }

    fn state(&self) -> stylo_dom::ElementState {
        warn_once!("WARNING: <ElementRef as SelectorMapElement>::state unimplemented.");
        stylo_dom::ElementState::from_bits(0).unwrap()
    }

    fn namespace(&self) -> &web_atoms::Namespace {
        warn_once!("WARNING: <ElementRef as SelectorMapElement>::namespace unimplemented.");
        Box::leak(Box::new(web_atoms::Namespace::from("")))
    }

    fn traversal_parent(&self) -> Option<Self> {
        warn_once!("WARNING: <ElementRef as SelectorMapElement>::traversal_parent unimplemented.");
        None
    }

    fn borrow_data(&self) -> Option<atomic_refcell::AtomicRef<'_, style::data::ElementData>> {
        warn_once!("WARNING: <ElementRef as SelectorMapElement>::borrow_data unimplemented.");
        None
    }

    fn query_container_size(
        &self,
        display: &style::values::computed::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
        warn_once!("WARNING: <ElementRef as SelectorMapElement>::query_container_size unimplemented.");
        euclid::Size2D::new(None, None)
    }
}