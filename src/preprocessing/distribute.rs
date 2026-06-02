use crate::Selector;
use cssparser::ToCss as _;
use log::trace;
use selectors::parser::Component;
use smallvec::SmallVec;
use std::iter::Iterator;
use style::selector_parser::SelectorImpl;

#[derive(Clone, Debug)]
struct InnerIter<'selector> {
    parent_iter_snapshot: std::iter::Rev<std::slice::Iter<'selector, Component<SelectorImpl>>>,
    selector_iter: std::slice::Iter<'selector, Selector>,
}

#[derive(Clone, Debug)]
enum ComponentOrInner<'selector> {
    Component(&'selector Component<SelectorImpl>),
    Inner(InnerIter<'selector>),
}

#[derive(Clone, Debug)]
enum StackOrNoOp<'selector> {
    Stack(SmallVec<[ComponentOrInner<'selector>; 8]>),
    NoOp(std::iter::Once<Selector>), // Variant to avoid allocating a Vec if the selector has no :is() components to distribute (common)
}

#[derive(Clone, Debug)]
pub struct DistributedSelectors<'selector> {
    stack: StackOrNoOp<'selector>,
}

impl<'selector> Iterator for DistributedSelectors<'selector> {
    type Item = Selector;
    fn next(&mut self) -> Option<Selector> {
        match &mut self.stack {
            StackOrNoOp::NoOp(once) => once.next(),
            StackOrNoOp::Stack(stack) => {
                trace!("distribute::next: start stack={stack:#?}");
                // realize the current stack state
                let Some(current) = Self::realize_buffer(stack) else {
                    trace!("distribute::next: stack realized to None");
                    return None;
                };
                trace!(
                    "distribute::next: realized current={}",
                    current.to_css_string()
                );
                // Prepare the stack for the next call.
                // Pop items from our buffer until we find an Inner.
                // Call next on it. If None, pop that Inner and continue searching.
                let (next_sel, parent_iter) = loop {
                    let last_inner = loop {
                        match stack.last_mut() {
                            Some(ComponentOrInner::Component(component)) => {
                                trace!("distribute::next: popping component={component:?}");
                                stack.pop();
                            },
                            Some(ComponentOrInner::Inner(i)) => {
                                trace!("distribute::next: found inner={i:#?}");
                                break i
                            },
                            None => {
                                trace!("distribute::next: exhausted stack after returning current");
                                return None
                            },
                        }
                    };
                    match last_inner.selector_iter.next() {
                        Some(next_sel) => {
                            trace!(
                                "distribute::next: advancing inner to next_sel={}",
                                next_sel.to_css_string()
                            );
                            break (next_sel, last_inner.parent_iter_snapshot.clone())
                        },
                        None => {
                            trace!("distribute::next: inner exhausted, popping it");
                            stack.pop();
                            continue; // unneeded but makes this more readable for me
                        },
                    }
                };
                // recursively push the components of next_sel onto the stack;
                // then, recursively push the components of parent_iter.
                trace!(
                    "distribute::next: pushing next_sel={}, then parent_iter={parent_iter:?}",
                    next_sel.to_css_string()
                );
                Self::recursively_push(stack, next_sel.iter_raw_parse_order_from(0));
                Self::recursively_push(stack, parent_iter);
                trace!("distribute::next: end stack={stack:#?}");
                Some(current)
            },
        }
    }
}

impl<'selector> DistributedSelectors<'selector> {
    pub fn from_selector(selector: &'selector Selector) -> Self {
        let components = selector.iter_raw_parse_order_from(0);
        let stack = if components.clone().all(|component| !matches!(component, Component::Is(_))) {
            // Common/fast case: no :is() components, no distributing. Just clone this selector.
            StackOrNoOp::NoOp(std::iter::once(selector.clone()))
        } else {
            // There are :is() components. Seed the stack.
            let mut stack: SmallVec<[ComponentOrInner<'selector>; 8]> = SmallVec::new();
            Self::recursively_push(&mut stack, components);
            StackOrNoOp::Stack(stack)
        };
        Self { stack }
    }

    fn recursively_push(stack: &mut SmallVec<[ComponentOrInner<'selector>; 8]>, mut components: std::iter::Rev<std::slice::Iter<'selector, Component<SelectorImpl>>>) {
        trace!("distribute::recursively_push: start stack={stack:#?}, components={components:?}");
        while let Some(component) = components.next() {
            trace!("distribute::recursively_push: next component={component:?}");
            match component {
                // upon encountering :is, push the Inner variant, then recurse inside the first selector.
                Component::Is(sel_list) => {
                    let inner_iter = InnerIter {
                        parent_iter_snapshot: components.clone(),
                        selector_iter: sel_list.slice().iter(),
                    };
                    trace!("distribute::recursively_push: pushing inner={inner_iter:#?}");
                    stack.push(ComponentOrInner::Inner(inner_iter));
                    let Some(ComponentOrInner::Inner(inner_iter)) = stack.last_mut() else {
                        panic!()
                    };
                    let first_sel = inner_iter.selector_iter.next().unwrap(); // assuming all :is()s have at least one selector
                    trace!(
                        "distribute::recursively_push: descending into first_sel={} with deferred parent_iter={:?}",
                        first_sel.to_css_string(),
                        inner_iter.parent_iter_snapshot
                    );
                    // TODO: this actually wasn't true, since I was passing selectors directly from my concretization pass to this pass, and that was leaving empty :is() selectors. I've patched that to now put Component::Invalid inside if the list is empty. However, this pass probably ought to be able to handle empty :is() selectors.
                    Self::recursively_push(stack, first_sel.iter_raw_parse_order_from(0));
                },
                // TODO: add :where, and all other selectors I want to distribute
                other => {
                    trace!("distribute::recursively_push: pushing component={other:?}");
                    stack.push(ComponentOrInner::Component(other))
                },
            }
        }
        trace!("distribute::recursively_push: end stack={stack:#?}");
    }

    fn realize_buffer(stack: &SmallVec<[ComponentOrInner<'selector>; 8]>) -> Option<Selector> {
        // empty stack means no more selectors
        if stack.is_empty() {
            return None;
        }
        let components = stack.iter().filter_map(|item| match item {
            ComponentOrInner::Component(c) => Some((*c).clone()),
            ComponentOrInner::Inner(_) => None, // Skip over Inner.
            // This is because Self::recursively_push pushes an Inner, then pushes the
            // correct components from inside the Inner immediately after.
        });
        Some(super::selector_from_iter(components))
    }
}
