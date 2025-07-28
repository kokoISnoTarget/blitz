use crate::{BaseDocument, Node, node::NodeKind};

pub(crate) struct Range {
    pub start_id: usize,
    pub end_id: usize,
}

impl BaseDocument {
    fn get_node_index(&self, node_id: usize) -> usize {
        self.index_map
            .get(&node_id)
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

pub(crate) struct Selection {
    /// The id of the node where the selection starts.
    pub start_id: usize,
    /// The id of the node where the selection ends.
    pub end_id: usize,
    /// The offset of the selection start within the start node.
    /// In text nodes, this is the index of the character within the text node.
    /// In element nodes, this is the index of the child node within the element.
    pub start_offset: usize,
    /// The offset of the selection end within the end node.
    /// In text nodes, this is the index of the character within the text node.
    /// In element nodes, this is the index of the child node within the element.
    pub end_offset: usize,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            start_id: 0,
            end_id: 0,
            start_offset: 0,
            end_offset: 0,
        }
    }
    pub fn is_collapsed(&self) -> bool {
        self.start_id == self.end_id && self.start_offset == self.end_offset
    }
}

impl BaseDocument {
    fn is_node_selected(&self, node_id: usize) -> bool {
        let start_node = self.get_node(self.selection.start_id).unwrap();
        let end_node = self.get_node(self.selection.end_id).unwrap();

        let get_offset_node = |node: &Node, offset: usize| -> usize {
            match node.data.kind() {
                NodeKind::Element | NodeKind::AnonymousBlock => node.children[offset],
                _ => node.id,
            }
        };

        let start_id = get_offset_node(&start_node, self.selection.start_offset);
        let end_id = get_offset_node(&end_node, self.selection.end_offset);

        let mut start_index = self.get_node_index(start_id);
        let mut end_index = self.get_node_index(end_id);

        if start_index > end_index {
            std::mem::swap(&mut start_index, &mut end_index);
        }

        let target_index = self.get_node_index(node_id);
        start_index <= target_index && target_index <= end_index
    }
}

use parley::Selection as ParleySelection;
struct S2 {
    range: Range,
    selection: ParleySelection,
}
