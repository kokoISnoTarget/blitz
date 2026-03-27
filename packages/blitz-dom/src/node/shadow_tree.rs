use std::{cell::RefCell, collections::HashMap, ptr::NonNull};

use selectors::OpaqueElement;
use slab::Slab;

use crate::Node;

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

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowRootData {
    pub host: usize,
    pub mode: ShadowRootMode,
    pub slot_assignment: SlotAssignment,
}
impl ShadowRootData {
    pub(crate) fn style_data(&self) -> &style::stylist::CascadeData {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotAssignment {
    Manual,
    Named,
}
