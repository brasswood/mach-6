use crate::Selector;
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
                // realize the current stack state
                let Some(current) = Self::realize_buffer(stack) else {
                    return None;
                };
                // Prepare the stack for the next call.
                // Pop items from our buffer until we find an Inner.
                // Call next on it. If None, pop that Inner and continue searching.
                let (next_sel, parent_iter) = loop {
                    let last_inner = loop {
                        match stack.last_mut() {
                            Some(ComponentOrInner::Component(_)) => { stack.pop(); },
                            Some(ComponentOrInner::Inner(i)) => break i,
                            None => return None,
                        }
                    };
                    match last_inner.selector_iter.next() {
                        Some(next_sel) => break (next_sel, last_inner.parent_iter_snapshot.clone()),
                        None => {
                            stack.pop();
                            continue; // unneeded but makes this more readable for me
                        },
                    }
                };
                // recursively push the components of next_sel onto the stack;
                // then, recursively push the components of parent_iter.
                Self::recursively_push(stack, next_sel.iter_raw_parse_order_from(0));
                Self::recursively_push(stack, parent_iter);
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
        while let Some(component) = components.next() {
            match component {
                // upon encountering :is, push the Inner variant, then recurse inside the first selector.
                Component::Is(sel_list) => {
                    let inner_iter = InnerIter {
                        parent_iter_snapshot: components.clone(),
                        selector_iter: sel_list.slice().iter(),
                    };
                    stack.push(ComponentOrInner::Inner(inner_iter));
                    let Some(ComponentOrInner::Inner(inner_iter)) = stack.last_mut() else {
                        panic!()
                    };
                    let first_sel = inner_iter.selector_iter.next().unwrap(); // assuming all :is()s have at least one selector
                    Self::recursively_push(stack, first_sel.iter_raw_parse_order_from(0));
                },
                // TODO: add :where, and all other selectors I want to distribute
                other => stack.push(ComponentOrInner::Component(other)),
            }
        }
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