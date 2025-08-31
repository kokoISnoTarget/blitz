/**
 *
 */
use std::collections::{HashMap, HashSet};

use usvg::Text;

use crate::{
    BaseDocument, Node,
    node::{self, NodeKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextPosition {
    pub node_id: usize,
    /// The offset within the node.
    /// For text nodes this is the nth character.
    /// For element nodes this is the nth child.
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// The position where the selection starts.
    pub anchor: TextPosition,
    /// The position where the selection currently ends.
    pub focus: TextPosition,
    pub direction: Direction,

    /// The index of the first node in the selection.
    pub resolved_start_index: usize,
    /// The offset to the start of the selection within the first node.
    pub resolved_start_text_offset: usize,
    /// The index of the last node in the selection.
    pub resolved_end_index: usize,
    /// The offset to the end of the selection within the last node.
    pub resolved_end_text_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    Forward,
    Backward,
    #[default]
    None,
}

impl BaseDocument {
    pub(crate) fn resolve_selection(&mut self) {
        let Selection {
            anchor,
            focus,
            direction,
            resolved_start_index,
            resolved_start_text_offset,
            resolved_end_index,
            resolved_end_text_offset,
        } = &mut self.selection;

        (*resolved_start_index, *resolved_start_text_offset) = resolve_node(document, anchor);
        (*resolved_end_index, *resolved_end_text_offset) = resolve_node(document, focus);

        if *resolved_start_index > *resolved_end_index {
            std::mem::swap(resolved_start_index, resolved_end_index);
            std::mem::swap(resolved_start_offset, resolved_end_offset);
        }

        /// Converts the text_position to an node index and text offset.
        fn resolve_node(
            document: &mut BaseDocument,
            text_position: TextPosition,
        ) -> (usize, usize) {
            let node = document.get_node(text_position.node_id).unwrap();

            match node.kind {
                NodeKind::Text => (node.layout_index, text_position.offset),
                _ => {
                    let child_node_id = node.children.get(text_position.offset).unwrap();
                    let child_node = document.get_node(child_node_id).unwrap();
                    let child_index = child_node.layout_index;
                    (child_index, 0)
                }
            }
        }
    }

    /// Checks if a node is selected based on the current selection.
    /// [resolve_selection] needs to be called before using this method.
    fn is_node_selected(&self, node_id: usize) -> bool {
        let Selection {
            resolved_start_index,
            resolved_end_index,
            ..
        } = self.selection;
        let Some(index) = self.get_node(node_id).map(|node| node.layout_index) else {
            return false;
        };
        resolved_start_index <= index && index <= resolved_end_index
    }

    fn get_nodes_in_range(&self, start_index: usize, end_index: usize) -> &[usize] {
        &self.index_to_nodeid[start_index..=end_index]
    }

    pub fn get_selected_text(&self) -> String {
        let Selection {
            resolved_start_index,
            resolved_start_text_offset,
            resolved_end_index,
            resolved_end_text_offset,
            ..
        } = self.selection;
        //FIXME: Offset of first and last node need to be accounted for.

        let nodes = self.get_nodes_in_range(*resolved_start_index, *resolved_end_index);
        nodes
            .iter()
            .filter_map(|node_id| self.get_node(*node_id))
            .filter_map(Node::text_data)
            .map(|text| text.content.as_str())
            .collect::<String>()
    }
}
