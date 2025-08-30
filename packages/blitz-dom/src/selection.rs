use std::collections::{HashMap, HashSet};

use crate::{BaseDocument, Node, node::NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextPosition {
    pub node_id: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: TextPosition,
    pub focus: TextPosition,
    pub direction: Direction,

    pub resolved_start_index: usize,
    pub resolved_start_offset: usize,
    pub resolved_end_index: usize,
    pub resolved_end_offset: usize,
}
impl Selection {
    fn match_position(&self, position: usize) -> Option<SelectionMatch> {
        let Selection {
            resolved_start_index,
            resolved_end_index,
            ..
        } = self;
        if resolved_start_index <= position && position <= resolved_end_index {
            Some(SelectionMatch::from)
        } else {
            None
        }
    }
}
struct SelectionResolver<'a>(&'a Selection);
impl SelectionResolver<'_> {
    fn new(selection: &Selection) -> Self {
        Self(selection)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    Forward,
    Backward,
    #[default]
    None,
}

impl BaseDocument {
    fn resolve_selection(&mut self) {
        let Selection {
            anchor,
            focus,
            direction,
            resolved_start_index,
            resolved_start_offset,
            resolved_end_index,
            resolved_end_offset,
        } = &mut self.selection;

        *resolved_start_index = self.nodeid_to_index[&anchor.node_id];
        *resolved_end_index = self.nodeid_to_index[&focus.node_id];
        *resolved_start_offset = anchor.offset;
        *resolved_end_offset = focus.offset;

        if *resolved_start_index > *resolved_end_index {
            std::mem::swap(resolved_start_index, resolved_end_index);
            std::mem::swap(resolved_start_offset, resolved_end_offset);
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
        let index = self.get_node(node_id).map(|node| node.order())
        resolved_start_index <= index && index <= resolved_end_index
    }

}

pub(crate) enum SelectionMatch {
    Open(usize),
    Close(usize),
    Both(usize, usize),
    Full,
    None,
}

impl SelectionMatcher {
    pub(crate) fn match_node(&self, node_id: usize) -> SelectionMatch {
        let Selection {
            resolved_start_index,
            resolved_end_index,
            resolved_start_offset,
            resolved_end_offset,
            ..
        } = self.selection;
        let index = self.nodeid_to_index[&node_id];

        if index < resolved_start_index || index > resolved_end_index {
            SelectionMatch::None
        } else if index == resolved_start_index && index == resolved_end_index {
            SelectionMatch::Both(resolved_start_offset, resolved_end_offset)
        } else if index == resolved_start_index {
            SelectionMatch::Open(resolved_start_offset)
        } else if index == resolved_end_index {
            SelectionMatch::Close(resolved_end_offset)
        } else {
            SelectionMatch::Full
        }
    }
}
