use atomic_refcell::AtomicRefMut;
use style::{data::ElementData, sharing::StyleSharingElement};
use crate::ElementRef;

impl StyleSharingElement for ElementRef<'_> {
    fn style_attribute(&self) -> Option<style::servo_arc::ArcBorrow<'_, style::shared_lock::Locked<style::properties::PropertyDeclarationBlock>>> {
        Some(self.value().style_block.borrow_arc())
    }

    fn synthesize_presentational_hints_for_legacy_attributes<V>(
        &self,
        _visited_handling: selectors::context::VisitedHandlingMode,
        _hints: &mut V,
    ) where
        V: selectors::sink::Push<style::applicable_declarations::ApplicableDeclarationBlock> {
        // TODO: something here?
    }

    fn has_part_attr(&self) -> bool {
        self.value().attr("part").is_some()
    }

    fn exports_any_part(&self) -> bool {
        self.value().attr("exportparts").is_some()
    }

    fn has_animations(&self, _context: &style::context::SharedStyleContext) -> bool {
        false // TODO: something here?
    }
    
    fn mutate_data(&self) -> Option<AtomicRefMut<'_, ElementData>> {
        Some(self.value().mutate_data())
    }
}
