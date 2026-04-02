use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use markup5ever::{Attribute, LocalName, local_name};
use selectors::OpaqueElement;
use slab::Slab;
use style::{
    author_styles::AuthorStyles,
    stylesheets::{DocumentStyleSheet, StylesheetInDocument, scope_rule::ImplicitScopeRoot},
};

use crate::BaseDocument;
use crate::traversal::TreeTraverser;
use crate::{Node, local_names, traversal::AncestorTraverser};

thread_local! {
    static GLOBAL_OPAQUE_ELEMENT_MAP: RefCell<GlobalOpaqueElementMap> = RefCell::new(GlobalOpaqueElementMap {
        doc_id_to_document_nodes: HashMap::new(),
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueElementId(usize);

impl OpaqueElementId {
    // 64-bit: doc_id (16 bits) | node_id (48 bits)
    #[cfg(target_pointer_width = "64")]
    const SHIFT: usize = 48;
    #[cfg(target_pointer_width = "64")]
    const MASK: usize = 0xFFFFFFFFFFFF;

    // 32-bit: doc_id (8 bits) | node_id (24 bits)
    #[cfg(target_pointer_width = "32")]
    const SHIFT: usize = 24;
    #[cfg(target_pointer_width = "32")]
    const MASK: usize = 0xFFFFFF;

    #[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
    compile_error!(
        "Opaque elements are not implemented/configured for the current target pointer width"
    );

    pub fn new(doc_id: usize, node_id: usize) -> Self {
        debug_assert!(node_id <= Self::MASK, "node_id exceeds available bits");

        let left = doc_id << Self::SHIFT;
        let right = node_id & Self::MASK;

        Self(left | right)
    }

    pub fn doc_id(&self) -> usize {
        self.0 >> Self::SHIFT
    }
    pub fn node_id(&self) -> usize {
        self.0 & Self::MASK
    }

    pub fn from_node(node: &Node) -> Self {
        Self::new(node.doc_id, node.id)
    }
    pub fn to_node(&self) -> Option<&Node> {
        GlobalOpaqueElementMap::get_node(self.doc_id(), self.node_id())
    }

    pub fn to_opaque_element(&self) -> OpaqueElement {
        OpaqueElement::from_non_null_ptr(NonNull::new((self.0 + 1) as *mut ()).unwrap())
    }
    pub fn from_opaque_element(opaque_element: OpaqueElement) -> Self {
        Self(unsafe { opaque_element.as_const_ptr::<()>().addr() } - 1)
    }
}

pub(crate) struct GlobalOpaqueElementMap {
    doc_id_to_document_nodes: HashMap<usize, *const Slab<Node>>,
}
impl GlobalOpaqueElementMap {
    pub fn insert(doc_id: usize, slab: *const Slab<Node>) {
        GLOBAL_OPAQUE_ELEMENT_MAP
            .with_borrow_mut(|map| map.doc_id_to_document_nodes.insert(doc_id, slab));
    }
    pub fn get(doc_id: usize) -> Option<*const Slab<Node>> {
        GLOBAL_OPAQUE_ELEMENT_MAP
            .with_borrow(|map| map.doc_id_to_document_nodes.get(&doc_id).copied())
    }
    pub fn remove(doc_id: usize) -> Option<*const Slab<Node>> {
        GLOBAL_OPAQUE_ELEMENT_MAP
            .with_borrow_mut(|map| map.doc_id_to_document_nodes.remove(&doc_id))
    }
    pub fn get_node<'a>(doc_id: usize, node_id: usize) -> Option<&'a Node> {
        let doc = Self::get(doc_id)?;
        unsafe { (&*doc).get(node_id) }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeSlottableData {
    pub name: String,
    pub assigned_slot: Option<usize>,
    pub manual_slot_assignment: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentFragmentData {
    pub host: Option<usize>,
    pub shadow_root: Option<ShadowRootData>,
}

#[derive(Debug, Clone, Default)]
pub struct ShadowRootData {
    pub mode: ShadowRootMode,
    pub slot_assignment: SlotAssignment,
    pub cloneable: bool,
    pub delegates_focus: bool,
    pub serializable: bool,
    pub declarative: bool,

    pub styles: StylesWrapper,
}

impl ShadowRootData {
    pub fn style_data(&self) -> &style::stylist::CascadeData {
        &self.styles.data
    }
    pub fn implicit_scope_for_sheet(&self, sheet_index: usize) -> Option<ImplicitScopeRoot> {
        self.styles
            .stylesheets
            .get(sheet_index)?
            .implicit_scope_root()
    }
}

pub struct StylesWrapper(AuthorStyles<DocumentStyleSheet>);
impl Debug for StylesWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthorStylesWrapper")
            .finish_non_exhaustive()
    }
}
impl Deref for StylesWrapper {
    type Target = AuthorStyles<DocumentStyleSheet>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for StylesWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Clone for StylesWrapper {
    fn clone(&self) -> Self {
        StylesWrapper(AuthorStyles::new()) // TODO: actually clone
    }
}
impl Default for StylesWrapper {
    fn default() -> Self {
        Self(AuthorStyles::new())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ShadowRootMode {
    #[default]
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SlotAssignment {
    Manual,
    #[default]
    Named,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShadowRootInit {
    pub mode: ShadowRootMode,
    pub assignment: SlotAssignment,
    pub cloneable: bool,
    pub delegates_focus: bool,
    pub serializable: bool,
}
impl From<&[Attribute]> for ShadowRootInit {
    fn from(attrs: &[Attribute]) -> Self {
        let mut init = Self::default();
        for Attribute { name, value } in attrs {
            match name.local {
                local_name!(shadowrootmode) if value == "closed" => {
                    init.mode = ShadowRootMode::Closed;
                }
                local_name!(shadowrootclonable) => {
                    init.cloneable = true;
                }
                local_name!(shadowrootdelegatesfocus) => {
                    init.delegates_focus = true;
                }
                local_name!(shadowrootserializable) => {
                    init.serializable = true;
                }
                _ => {}
            }
        }
        init
    }
}

pub fn valid_shadow_host_name(name: LocalName) -> bool {
    name.contains('-')
        || matches!(
            name,
            local_names!(
                "article", "aside",
                "blockquote", "body",
                "div", "footer",
                "h1", "h2",
                "h3", "h4",
                "h5", "h6",
                "header", "main",
                "nav", "p",
                "section", "span"; |)
        )
}

// To find a slot for a given slottable slottable and an optional boolean open (default false):
//     If slottable’s parent is null, then return null.
//     Let shadow be slottable’s parent’s shadow root.
//     If shadow is null, then return null.
//     If open is true and shadow’s mode i not "open", then return null.
//     If shadow’s slot assignment is "manual", then return the slot in shadow’s descendants whose manually assigned nodes contains slottable, if any; otherwise null.
//     Return the first slot in tree order in shadow’s descendants whose name is slottable’s name, if any; otherwise null.
pub fn find_a_slot(doc: &BaseDocument, slottable_id: usize, open: bool) -> Option<usize> {
    let slottable = doc.get_node(slottable_id)?;
    // If slottable’s parent is null, then return null.
    let parent_id = slottable.parent?;
    // Let shadow be slottable’s parent’s shadow root.
    let parent = doc.get_node(parent_id)?;
    let shadow_root_id = parent.element_data()?.shadow_root?;
    let shadow_root = doc.get_node(shadow_root_id)?;
    let shadow_data = shadow_root.shadow_root_data()?;

    // If open is true and shadow’s mode is not "open", then return null.
    if open && shadow_data.mode != ShadowRootMode::Open {
        return None;
    }

    // If shadow’s slot assignment is "manual", then return the slot in shadow’s descendants whose manually assigned nodes contains slottable, if any; otherwise null.
    if shadow_data.slot_assignment == SlotAssignment::Manual {
        return find_manual_slot(doc, shadow_root_id, slottable_id);
    }

    // Return the first slot in tree order in shadow’s descendants whose name is slottable’s name, if any; otherwise null.
    find_named_slot(doc, shadow_root_id, &slottable.slottable.name)
}

fn find_manual_slot(
    doc: &BaseDocument,
    shadow_root_id: usize,
    slottable_id: usize,
) -> Option<usize> {
    let slottable = doc.get_node(slottable_id)?;
    let slot_id = slottable.slottable.manual_slot_assignment?;

    // Check if slot_id is a descendant of shadow_root_id
    if AncestorTraverser::new(doc, shadow_root_id).any(|id| id == slot_id) {
        return Some(slot_id);
    }
    None
}

fn find_named_slot(doc: &BaseDocument, shadow_root_id: usize, name: &str) -> Option<usize> {
    // Return the first slot in tree order in shadow’s descendants whose name is slottable’s name.
    // skip(1) to avoid checking the root itself.
    for desc_id in TreeTraverser::new_with_root(doc, shadow_root_id).skip(1) {
        if let Some(node) = doc.get_node(desc_id) {
            if node.data.is_element_with_tag_name(&local_name!("slot")) {
                let slot_name = node
                    .element_data()
                    .and_then(|el| el.attr(local_name!("name")))
                    .unwrap_or("");
                if slot_name == name {
                    return Some(desc_id);
                }
            }
        }
    }
    None
}

// To find slottables for a given slot slot:
//     Let result be « ».
//     Let root be slot’s root.
//     If root is not a shadow root, then return result.
//     Let host be root’s host.
//     If root’s slot assignment is "manual":
//         For each slottable slottable of slot’s manually assigned nodes, if slottable’s parent is host, append slottable to result.
//     Otherwise, for each slottable child slottable of host, in tree order:
//         Let foundSlot be the result of finding a slot given slottable.
//         If foundSlot is slot, then append slottable to result.
//     Return result.
pub fn find_slottables(doc: &BaseDocument, slot_id: usize) -> Vec<usize> {
    let mut result = Vec::new();
    // Let root be slot’s root.
    let root_id = find_root(doc, slot_id);
    let root = match doc.get_node(root_id) {
        Some(node) => node,
        None => return result,
    };
    // If root is not a shadow root, then return result.
    let shadow_data = match root.shadow_root_data() {
        Some(data) => data,
        None => return result,
    };
    // Let host be root’s host.
    let host_id = match root.document_fragment_data().and_then(|d| d.host) {
        Some(id) => id,
        None => return result,
    };
    let host = match doc.get_node(host_id) {
        Some(node) => node,
        None => return result,
    };

    // If root’s slot assignment is "manual":
    if shadow_data.slot_assignment == SlotAssignment::Manual {
        // For each slottable child slottable of host, in tree order:
        // if slottable’s manually assigned slot is slot, append slottable to result.
        for &child_id in &host.children {
            if let Some(child) = doc.get_node(child_id) {
                if child.slottable.manual_slot_assignment == Some(slot_id) {
                    result.push(child_id);
                }
            }
        }
    } else {
        // Otherwise, for each slottable child slottable of host, in tree order:
        // Let foundSlot be the result of finding a slot given slottable.
        // If foundSlot is slot, then append slottable to result.
        for &child_id in &host.children {
            if find_a_slot(doc, child_id, false) == Some(slot_id) {
                result.push(child_id);
            }
        }
    }
    result
}

fn find_root(doc: &BaseDocument, node_id: usize) -> usize {
    AncestorTraverser::new(doc, node_id).last().unwrap()
}

// To find flattened slottables for a given slot slot:
//     Let result be « ».
//     If slot’s root is not a shadow root, then return result.
//     Let slottables be the result of finding slottables given slot.
//     If slottables is the empty list, then append each slottable child of slot, in tree order, to slottables.
//     For each node of slottables:
//         If node is a slot whose root is a shadow root:
//             Let temporaryResult be the result of finding flattened slottables given node.
//             Append each slottable in temporaryResult, in order, to result.
//         Otherwise, append node to result.
//     Return result.
pub fn find_flattened_slottables(doc: &BaseDocument, slot_id: usize) -> Vec<usize> {
    let mut result = Vec::new();
    // If slot’s root is not a shadow root, then return result.
    let root_id = find_root(doc, slot_id);
    if doc
        .get_node(root_id)
        .and_then(|n| n.shadow_root_data())
        .is_none()
    {
        return result;
    }
    // Let slottables be the result of finding slottables given slot.
    let mut slottables = find_slottables(doc, slot_id);
    // If slottables is the empty list, then append each slottable child of slot, in tree order, to slottables.
    if slottables.is_empty() {
        if let Some(slot) = doc.get_node(slot_id) {
            slottables = slot.children.clone();
        }
    }
    // For each node of slottables:
    for node_id in slottables {
        if let Some(node) = doc.get_node(node_id) {
            // If node is a slot whose root is a shadow root:
            let is_slot = node.data.is_element_with_tag_name(&local_name!("slot"));
            let node_root_id = find_root(doc, node_id);
            if is_slot
                && doc
                    .get_node(node_root_id)
                    .and_then(|n| n.shadow_root_data())
                    .is_some()
            {
                // Let temporaryResult be the result of finding flattened slottables given node.
                // Append each slottable in temporaryResult, in order, to result.
                let mut temp = find_flattened_slottables(doc, node_id);
                result.append(&mut temp);
            } else {
                // Otherwise, append node to result.
                result.push(node_id);
            }
        }
    }
    result
}

// To assign slottables for a slot slot:
//     Let slottables be the result of finding slottables for slot.
//     If slottables and slot’s assigned nodes are not identical, then run signal a slot change for slot.
//     Set slot’s assigned nodes to slottables.
//     For each slottable of slottables: set slottable’s assigned slot to slot.
pub fn assign_slottables(doc: &mut BaseDocument, slot_id: usize) {
    // Let slottables be the result of finding slottables for slot.
    let slottables = find_slottables(doc, slot_id);

    // Set slot’s assigned nodes to slottables.
    if let Some(slot_data) = doc
        .get_node_mut(slot_id)
        .and_then(|n| n.element_data_mut())
        .and_then(|el| el.slot_data_mut())
    {
        slot_data.assigned_nodes = slottables.clone();
    }

    // For each slottable of slottables: set slottable’s assigned slot to slot.
    for &slottable_id in &slottables {
        if let Some(slottable) = doc.get_node_mut(slottable_id) {
            slottable.slottable.assigned_slot = Some(slot_id);
        }
    }
}

// To assign slottables for a tree, given a node root, run assign slottables for each slot of root’s inclusive descendants, in tree order.
pub fn assign_slottables_for_a_tree(doc: &mut BaseDocument, root_id: usize) {
    let mut slots = Vec::new();
    for id in TreeTraverser::new_with_root(doc, root_id) {
        if let Some(node) = doc.get_node(id) {
            if node.data.is_element_with_tag_name(&local_name!("slot")) {
                slots.push(id);
            }
        }
    }

    for slot_id in slots {
        assign_slottables(doc, slot_id);
    }
}

// To assign a slot, given a slottable slottable:
//     Let slot be the result of finding a slot with slottable.
//     If slot is non-null, then run assign slottables for slot.
pub fn assign_a_slot_given_slottable(doc: &mut BaseDocument, slottable_id: usize) {
    // Let slot be the result of finding a slot with slottable.
    let slot_id = find_a_slot(doc, slottable_id, false);
    // If slot is non-null, then run assign slottables for slot.
    if let Some(slot_id) = slot_id {
        assign_slottables(doc, slot_id);
    }
}
