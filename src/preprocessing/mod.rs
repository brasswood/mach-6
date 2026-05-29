use selectors::{builder::SelectorBuilder, parser::Component};
use style::selector_parser::SelectorImpl;

use crate::structs::Selector;

pub mod concretize;
pub mod distribute;

fn selector_from_iter(components: impl Iterator<Item = Component<SelectorImpl>>) -> Selector {
    let mut builder = SelectorBuilder::default();
    for component in components {
        if let Some(combinator) = component.as_combinator() {
            builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
            builder.push_combinator(combinator);
        } else {
            builder.push_simple_selector(component.clone());
        }
    }
    builder.reverse_last_compound(); // TODO: This will effectively reverse twice. Get rid of this.
    builder.build_selector(selectors::parser::ParseRelative::No)
}

