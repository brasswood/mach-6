use style::selector_map::SelectorMapElement;
use crate::ElementRef;

impl SelectorMapElement for ElementRef<'_> {
    fn id(&self) -> Option<&style::Atom> {
        self.value().id_atom()
    }

    fn each_class<F>(&self, callback: F)
    where
        F: FnMut(&style::values::AtomIdent) {
        todo!()
    }

    fn each_attr_name<F>(&self, callback: F)
    where
        F: FnMut(&style::LocalName) {
        todo!()
    }

    fn local_name(&self) -> &<style::selector_parser::SelectorImpl as selectors::parser::SelectorImpl>::BorrowedLocalName {
        todo!()
    }

    fn state(&self) -> stylo_dom::ElementState {
        todo!()
    }

    fn namespace(&self)
        -> &<style::selector_parser::SelectorImpl as selectors::parser::SelectorImpl>::BorrowedNamespaceUrl {
        todo!()
    }

    fn traversal_parent(&self) -> Option<Self> {
        todo!()
    }

    fn borrow_data(&self) -> Option<atomic_refcell::AtomicRef<'_, style::data::ElementData>> {
        todo!()
    }

    fn query_container_size(
        &self,
        display: &style::values::computed::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
        todo!()
    }
}