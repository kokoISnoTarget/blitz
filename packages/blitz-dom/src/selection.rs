/**
 *
 */
use std::collections::{HashMap, HashSet};

use parley::{Cluster, Cursor, Layout, Rect};
use skrifa::raw::tables::base::Base;
use usvg::Text;

use crate::{
    BaseDocument, Node,
    node::{self, NodeKind, TextBrush},
    traversal::AncestorTraverser,
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
    pub fn selection_view(&self) -> SelectionView {
        SelectionView(self)
    }
    pub fn selection_driver(&mut self) -> SelectionDriver {
        SelectionDriver(self)
    }
}

struct SelectionDriver<'a>(&'a mut BaseDocument);
impl SelectionDriver<'a> {
    pub(crate) fn mouse_down(&mut self, node_id: usize, x: f64, y: f64) {}

    pub(crate) fn mouse_move(&self, node_id: usize, x: f64, y: f64) {}

    fn resolve_mouse_position(&self, node_id: usize, x: f64, y: f64) -> TextPosition {
        let layout = AncestorTraverser::new(self.0, node_id)
            .filter_map(|node_id| self.0.get_node(node_id))
            .find(|node| node.flags.is_inline_root())
            .unwrap()
            .element_data()
            .unwrap()
            .inline_layout_data
            .as_deref()
            .unwrap();

        layout.layout.lines().
        let layout_cursor = Cursor::from_point(&layout.layout, x as f32, y as f32).visual_clusters(&layout.layout);
    }

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
}
impl Deref for SelectionDriver<'_> {
    type Target = SelectionView;

    fn deref(&self) -> &Self::Target {
        &SelectionView(self.0)
    }
}
struct SelectionView<'a>(&'a BaseDocument);
impl SelectionView<'a> {
    /// Checks if a node is selected based on the current selection.
    /// [resolve_selection] needs to be called before using this method.
    fn is_node_selected(&self, node_id: usize) -> bool {
        let Selection {
            resolved_start_index,
            resolved_end_index,
            ..
        } = self.0.selection;
        let Some(index) = self.0.get_node(node_id).map(|node| node.layout_index) else {
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
        } = self.0.selection;
        //FIXME: Offset of first and last node need to be accounted for.

        let nodes = self.get_nodes_in_range(*resolved_start_index, *resolved_end_index);
        nodes
            .iter()
            .filter_map(|node_id| self.get_node(*node_id))
            .filter_map(Node::text_data)
            .map(|text| text.content.as_str())
            .collect::<String>()
    }

    pub fn selection_geometry(&self, layout: &Layout<TextBrush>, mut f: impl FnMut(Rect)) {
        for line in layout.lines() {
            // FIXME: Dont just select the whole line if any cluster in the line is selected
            if line
                .runs()
                .flat_map(|run| run.clusters())
                .any(|cluster| self.is_node_selected(cluster.first_style().brush.id))
            {
                let metrics = line.metrics();
                let line_min = metrics.min_coord as f64;
                let line_max = metrics.max_coord as f64;

                let x = metrics.offset as f64;
                let width = metrics.advance as f64;
                f(Rect::new(x, line_min, x + width, line_max), line_ix);
            }
        }
    }
}

/*
// This is partially from parley
// https://github.com/linebender/parley/blob/4240aebba14eed5f7f8c21880a1b7a877eac76eb/parley/src/layout/cursor.rs#L912C1-L1008C6

/// Invokes `f` with the sequence of rectangles which represent the visual
/// geometry of this selection for the given layout, and the indices of the
/// lines to which they belong.
///
/// This avoids allocation if the intent is to render the rectangles
/// immediately.
fn geometry_with<B: Brush>(&self, layout: &Layout<B>, mut f: impl FnMut(Rect, usize)) {
    const NEWLINE_WHITESPACE_WIDTH_RATIO: f64 = 0.25;
    if self.is_collapsed() {
        return;
    }
    let mut start = self.anchor;
    let mut end = self.focus;
    if start.index > end.index {
        core::mem::swap(&mut start, &mut end);
    }
    let text_range = start.index..end.index;
    let line_start_ix = start.line(layout).map(|(ix, _)| ix).unwrap_or(0);
    let line_end_ix = end
        .line(layout)
        .map(|(ix, _)| ix)
        .unwrap_or(layout.len() + 1);
    for line_ix in line_start_ix..=line_end_ix {
        let Some(line) = layout.get(line_ix) else {
            continue;
        };
        let metrics = line.metrics();
        let line_min = metrics.min_coord as f64;
        let line_max = metrics.max_coord as f64;
        // Trailing whitespace to indicate that the newline character at the
        // end of this line is selected. It's based on the ascent and
        // descent so it doesn't change with the line height.
        //
        // TODO: the width of this whitespace should be the width of a space
        // (U+0020) character.
        let newline_whitespace = if line.break_reason() == BreakReason::Explicit {
            (metrics.ascent as f64 + metrics.descent as f64) * NEWLINE_WHITESPACE_WIDTH_RATIO
        } else {
            0.0
        };
        if line_ix == line_start_ix || line_ix == line_end_ix {
            // We only need to run the expensive logic on the first and
            // last lines
            let mut start_x = metrics.offset as f64;
            let mut cur_x = start_x;
            let mut cluster_count = 0;
            let mut box_advance = 0.0;
            let mut have_seen_any_runs = false;
            for item in line.items_nonpositioned() {
                match item {
                    LineItem::Run(run) => {
                        have_seen_any_runs = true;
                        for cluster in run.visual_clusters() {
                            let advance = cluster.advance() as f64 + box_advance;
                            box_advance = 0.0;
                            if text_range.contains(&cluster.text_range().start) {
                                cluster_count += 1;
                                cur_x += advance;
                            } else {
                                if cur_x != start_x {
                                    f(Rect::new(start_x, line_min, cur_x, line_max), line_ix);
                                }
                                cur_x += advance;
                                start_x = cur_x;
                            }
                        }
                    }
                    LineItem::InlineBox(inline_box) => {
                        box_advance += inline_box.width as f64;
                        // HACK: Don't display selections for inline boxes
                        // if they're the first thing in the line. This
                        // makes the selection match the cursor position.
                        if !have_seen_any_runs {
                            cur_x += box_advance;
                            box_advance = 0.0;
                            start_x = cur_x;
                        }
                    }
                }
            }
            let mut end_x = cur_x;
            if line_ix != line_end_ix || (cluster_count != 0 && metrics.advance == 0.0) {
                end_x += newline_whitespace;
            }
            if end_x != start_x {
                f(Rect::new(start_x, line_min, end_x, line_max), line_ix);
            }
        } else {
            let x = metrics.offset as f64;
            let width = metrics.advance as f64;
            f(
                Rect::new(x, line_min, x + width + newline_whitespace, line_max),
                line_ix,
            );
        }
    }
}*/
