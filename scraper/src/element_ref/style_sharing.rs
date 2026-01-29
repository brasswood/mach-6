use style::sharing::StyleSharingElement;
use crate::ElementRef;

impl StyleSharingElement for ElementRef<'_> {
    fn style_attribute(&self) -> Option<style::servo_arc::ArcBorrow<'_, style::shared_lock::Locked<style::properties::PropertyDeclarationBlock>>> {
        todo!()
    }

    fn synthesize_presentational_hints_for_legacy_attributes<V>(
        &self,
        visited_handling: selectors::context::VisitedHandlingMode,
        hints: &mut V,
    ) where
        V: selectors::sink::Push<style::applicable_declarations::ApplicableDeclarationBlock> {
        todo!()
    }

    fn has_part_attr(&self) -> bool {
        todo!()
    }

    fn exports_any_part(&self) -> bool {
        todo!()
    }

    fn has_animations(&self, context: &style::context::SharedStyleContext) -> bool {
        todo!()
    }
}