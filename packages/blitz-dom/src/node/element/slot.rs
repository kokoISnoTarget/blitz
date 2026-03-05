use crate::node::{ShadowRootMode, SlotAssignmentMode, SpecialElementData};
use crate::traversal::{AncestorTraverser, TreeTraverser};
use crate::{BaseDocument, DocumentMutator, local_name};
use style::dom::TElement;

#[derive(Debug, Clone)]
pub struct SlotElement {
    /// https://dom.spec.whatwg.org/#slot-assigned-nodes
    pub assigned_nodes: Vec<usize>,
    /// https://html.spec.whatwg.org/multipage/scripting.html#manually-assigned-nodes
    pub manual_assigned_nodes: Vec<usize>,
}
impl SlotElement {
    pub const fn new() -> Self {
        Self {
            assigned_nodes: vec![],
            manual_assigned_nodes: vec![],
        }
    }
}

pub trait SlotDocumentMutatorExt {
    fn assign(&mut self, this_id: usize, nodes: impl IntoIterator<Item = usize>);
}

impl SlotDocumentMutatorExt for DocumentMutator<'_> {
    fn assign(&mut self, this_id: usize, nodes: impl IntoIterator<Item = usize>) {
        let Some(this_slot) = self.doc.nodes[this_id]
            .element_data_mut()
            .and_then(|data| data.slot_data_mut())
        else {
            #[cfg(feature = "tracing")]
            tracing::error!("Tried to assign to non slot element/node");
            return;
        };

        let mut node_set = std::mem::take(&mut this_slot.manual_assigned_nodes);

        node_set
            .drain(..)
            .for_each(|node_id| self.doc.nodes[node_id].manual_slot_assignment = None);

        for node_id in nodes.into_iter() {
            if let Some(slot_id) = self.doc.nodes[node_id].manual_slot_assignment {
                self.doc.nodes[slot_id]
                    .element_data_mut()
                    .expect("Should be an element")
                    .slot_data_mut()
                    .expect("Should be an slot element")
                    .manual_assigned_nodes
                    .retain(|slottable_id| *slottable_id != node_id)
            }
            self.doc.nodes[node_id].manual_slot_assignment = Some(this_id);
            node_set.push(node_id);
        }

        self.doc.nodes[this_id]
            .element_data_mut()
            .expect("Should have checked the precondition above")
            .slot_data_mut()
            .expect("Should have checked the precondition above")
            .manual_assigned_nodes = node_set;

        let root_id = AncestorTraverser::new(&self.doc, this_id)
            .last()
            .expect("Should always return at least this.");
        assign_slottables_for_tree(&mut self.doc, root_id)
    }
}

pub(crate) fn assign_slottables_for_tree(doc: &mut BaseDocument, root_id: usize) {
    let mut slots = TreeTraverser::new(doc)
        .filter(|node_id| {
            doc.nodes[*node_id]
                .element_data()
                .is_some_and(|data| data.name.local == local_name!(slot))
        })
        .collect::<Vec<_>>();

    for slot_id in slots.drain(..) {
        assign_slottables(doc, slot_id);
    }

    todo!();
}

pub(crate) fn assign_slottables(doc: &mut BaseDocument, slot_id: usize) {
    let slottables = find_slottables(doc, slot_id);
    // TODO: if slottables and slot’s assigned nodes are not identical, then run signal a slot change for slot. https://dom.spec.whatwg.org/#assign-slotables

    for &slottable_id in slottables.iter() {
        doc.nodes[slottable_id].assigned_slot = Some(slot_id);
    }

    doc.nodes[slot_id]
        .element_data_mut()
        .expect("Should be an element")
        .slot_data_mut()
        .expect("Should be an slot element")
        .assigned_nodes = slottables;
}

pub(crate) fn find_slottables(doc: &BaseDocument, slot_id: usize) -> Vec<usize> {
    let mut result = vec![];
    let root_id = AncestorTraverser::new(doc, slot_id)
        .last()
        .expect("Should always return at least this.");
    let Some(shadow_root) = doc.nodes[root_id].shadow_root_data() else {
        return result;
    };
    let host_id = shadow_root.host;
    let slot = doc.nodes[slot_id]
        .element_data()
        .expect("Should be an element")
        .slot_data()
        .expect("Should be an slot element");
    match shadow_root.slot_assignment_mode {
        SlotAssignmentMode::Manual => result.extend(
            slot.manual_assigned_nodes
                .iter()
                .copied()
                .filter(|slottable_id| doc.nodes[*slottable_id].parent == Some(host_id)),
        ),
        SlotAssignmentMode::Named => {
            for &slottable_id in &doc.nodes[host_id].children {
                if !doc.nodes[slottable_id].is_element() && !doc.nodes[slottable_id].is_text_node()
                {
                    continue;
                }
                if let Some(found_slot) = find_slot(doc, slottable_id, None)
                    && found_slot == slot_id
                {
                    result.push(slottable_id);
                }
            }
        }
    }

    result
}

pub(crate) fn find_slot(
    doc: &BaseDocument,
    slottable_id: usize,
    open: Option<bool>,
) -> Option<usize> {
    let open = open.unwrap_or(false);
    let slottable = &doc.nodes[slottable_id];
    let Some(parent_id) = slottable.parent else {
        return None;
    };
    let Some(shadow_root) = slottable.with(parent_id).containing_shadow() else {
        return None;
    };
    let shadow_root_data = shadow_root
        .shadow_root_data()
        .expect("Shadow root should have data");

    if open && shadow_root_data.mode != ShadowRootMode::Open {
        return None;
    }
    if shadow_root_data.slot_assignment_mode == SlotAssignmentMode::Manual {
        for node_id in TreeTraverser::new_with_root(doc, shadow_root.id) {
            let node = &doc.nodes[node_id];
            let Some(element_data) = node.element_data() else {
                continue;
            };
            let Some(slot_data) = element_data.slot_data() else {
                continue;
            };
            if slot_data.manual_assigned_nodes.contains(&slottable_id) {
                return Some(node_id);
            }
        }
        return None;
    }
    TreeTraverser::new_with_root(doc, shadow_root.id)
        .filter(|node_id| {
            let node = &doc.nodes[*node_id];
            let Some(element_data) = node.element_data() else {
                return false;
            };
            element_data.attr(local_name!("name")) == Some(slottable.name.as_str())
        })
        .next()
}
