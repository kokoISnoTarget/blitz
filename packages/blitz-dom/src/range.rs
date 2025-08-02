use std::collections::HashSet;

use crate::{BaseDocument, Node, node::NodeKind};

pub(crate) struct Range {
    pub start_id: usize,
    pub end_id: usize,
}

impl BaseDocument {
    fn get_node_index(&self, node_id: usize) -> usize {
        self.index_map
            .get(node_id)
            .copied()
            .expect("IndexMap should be updated before using it and node_id should exist.")
    }

    fn get_nodes_of_range(&self, range: Range) -> &[usize] {
        let mut start_index = self.get_node_index(range.start_id);
        let mut end_index = self.get_node_index(range.end_id);

        if start_index > end_index {
            std::mem::swap(&mut start_index, &mut end_index);
        }

        &self.index_map[start_index..=end_index]
    }
}

#[derive(Debug, Default)]
pub(crate) struct Selection {
    /// The id of the node where the selection starts.
    anchor_id: usize,
    /// The id of the node where the selection ends.
    focus_id: usize,
    /// The offset of the selection start within the start node.
    /// In text nodes, this is the index of the character within the text node.
    /// In element nodes, this is the index of the child node within the element.
    anchor_offset: usize,
    /// The offset of the selection end within the end node.
    /// In text nodes, this is the index of the character within the text node.
    /// In element nodes, this is the index of the child node within the element.
    focus_offset: usize,

    direction: Direction,

    true_anchor_id: Option<usize>,
    true_focus_id: Option<usize>,
}

#[derive(Debug, Default)]
enum Direction {
    Forward,
    Backward,
    #[default]
    None,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_anchor(&mut self, anchor_id: usize, anchor_offset: usize) {
        self.start_id = anchor_id;
        self.start_offset = anchor_offset;
        self.true_anchor_id = anchor_id;
    }
    pub fn set_focus(&mut self, focus_id: usize, focus_offset: usize) {
        self.end_id = focus_id;
        self.end_offset = focus_offset;
        self.true_focus_id = focus_id;
    }
    pub fn is_collapsed(&self) -> bool {
        self.start_id == self.end_id && self.start_offset == self.end_offset
    }
    fn start_text_must_be_offset(&self) -> bool {
        let Some(ref cache) = self.cache else {
            return false;
        };
        cache.start_id == self.start_id
    }
    fn end_text_must_be_offset(&self) -> bool {
        let Some(ref cache) = self.cache else {
            return false;
        };
        cache.end_id == self.end_id
    }
}

impl BaseDocument {
    pub fn is_node_selected(&self, node_id: usize) -> bool {
        let Selection {
            anchor_id,
            focus_id,
            anchor_offset,
            focus_offset,
            direction,
            true_anchor_id,
            true_focus_id,
        } = self.selection;
        let anchor_index = self.get_node_index(true_anchor_id.unwrap_or(anchor_id));
        let focus_index = self.get_node_index(true_focus_id.unwrap_or(focus_id));

        self.selection.self.index_map[anchor_index..=focus_index].contains(&node_id)
    }

    fn update_selection(&mut self) {
        let anchor_node = self.get_node(self.selection.anchor_id)?;
        let focus_node = self.get_node(self.selection.focus_id)?;

        let get_offset_node = |node: &Node, offset: usize| -> Option<usize> {
            match node.data.kind() {
                NodeKind::Element | NodeKind::AnonymousBlock if offset < node.children.len() => {
                    Some(node.children[offset])
                }
                _ => None,
            }
        };

        self.selection.true_anchor_id = get_offset_node(&anchor_node, self.selection.start_offset);
        self.selection.true_focus_id = get_offset_node(&focus_node, self.selection.end_offset);
    }
}
