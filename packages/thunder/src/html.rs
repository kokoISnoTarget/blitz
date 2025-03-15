use std::{
    borrow::Cow,
    cell::{Ref, RefCell},
};

use crate::document::JsDocument;
use blitz_dom::node::Attribute;
use html5ever::QualName;
use html5ever::interface::{ElementFlags, TreeSink};
use url::Url;

/// Convert an html5ever Attribute which uses tendril for its value to a blitz Attribute
/// which uses String.
fn html5ever_to_blitz_attr(attr: html5ever::Attribute) -> Attribute {
    Attribute {
        name: attr.name,
        value: attr.value.to_string(),
    }
}

pub struct HtmlParser<'a> {
    js_doc: RefCell<&'a mut JsDocument>,
}

impl<'a> HtmlParser<'a> {
    pub fn new(js_doc: &'a mut JsDocument) -> Self {
        HtmlParser {
            js_doc: RefCell::new(js_doc),
        }
    }
}

impl<'a> HtmlParser<'a> {
    pub fn parse(&self, html: &str) {
        // Parse HTML and populate the document
    }
}

// This is from https://github.com/DioxusLabs/blitz/blob/36369ba285d7291b449d9d7770427fc895dc5221/packages/blitz-html/src/html_sink.rs
impl<'b> TreeSink for HtmlParser<'b> {
    type Output = &'b mut JsDocument;

    // we use the ID of the nodes in the tree as the handle
    type Handle = usize;

    type ElemName<'a>
        = Ref<'a, QualName>
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        let doc = self.js_doc.into_inner();

        // Add inline stylesheets (<style> elements)
        for id in self.style_nodes.borrow().iter() {
            doc.process_style_element(*id);
        }

        for error in self.errors.borrow().iter() {
            println!("ERROR: {}", error);
        }

        doc
    }

    fn parse_error(&self, msg: Cow<'static, str>) {
        dbg!(msg);
    }

    fn get_document(&self) -> Self::Handle {
        0
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        Ref::map(self.js_doc.borrow(), |doc| {
            &doc.as_ref().nodes[*target]
                .element_data()
                .expect("TreeSink::elem_name called on a node which is not an element!")
                .name
        })
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<html5ever::Attribute>,
        _flags: ElementFlags,
    ) -> Self::Handle {
        let attrs = attrs.into_iter().map(html5ever_to_blitz_attr).collect();
        let mut data = ElementNodeData::new(name.clone(), attrs);
        data.flush_style_attribute(&self.doc.borrow().guard);

        let id = self.create_node(NodeData::Element(data));
        let node = self.node(id);

        // Initialise style data
        *node.stylo_element_data.borrow_mut() = Some(Default::default());

        let id_attr = node.attr(local_name!("id")).map(|id| id.to_string());
        drop(node);

        // If the node has an "id" attribute, store it in the ID map.
        if let Some(id_attr) = id_attr {
            self.doc.borrow_mut().nodes_to_id.insert(id_attr, id);
        }

        // Custom post-processing by element tag name
        match name.local.as_ref() {
            "link" => self.load_linked_stylesheet(id),
            "img" => self.load_image(id),
            "input" => self.process_button_input(id),
            "style" => self.style_nodes.borrow_mut().push(id),
            _ => {}
        }

        id
    }

    fn create_comment(&self, _text: StrTendril) -> Self::Handle {
        self.create_node(NodeData::Comment)
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> Self::Handle {
        self.create_node(NodeData::Comment)
    }

    fn append(&self, parent_id: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(child_id) => {
                self.node_mut(*parent_id).children.push(child_id);
                self.node_mut(child_id).parent = Some(*parent_id);
            }
            NodeOrText::AppendText(text) => {
                let last_child_id = self.last_child(*parent_id);
                let has_appended = self.try_append_text_to_text_node(last_child_id, &text);
                if !has_appended {
                    let id = self.create_text_node(&text);
                    self.append(parent_id, NodeOrText::AppendNode(id));
                }
            }
        }
    }

    // Note: The tree builder promises we won't have a text node after the insertion point.
    // https://github.com/servo/html5ever/blob/main/rcdom/lib.rs#L338
    fn append_before_sibling(&self, sibling_id: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        let sibling = self.node(*sibling_id);
        let parent_id = sibling.parent.expect("Sibling has not parent");
        let parent = self.node(parent_id);
        let sibling_pos = parent
            .children
            .iter()
            .position(|cid| cid == sibling_id)
            .expect("Sibling is not a child of parent");

        // If node to append is a text node, first attempt to
        let new_child_id = match new_node {
            NodeOrText::AppendText(text) => {
                let previous_sibling_id = match sibling_pos {
                    0 => None,
                    other => Some(parent.children[other - 1]),
                };
                let has_appended = self.try_append_text_to_text_node(previous_sibling_id, &text);
                if has_appended {
                    return;
                } else {
                    self.create_text_node(&text)
                }
            }
            NodeOrText::AppendNode(id) => id,
        };

        // TODO: Should remove from existing parent?
        assert_eq!(self.node_mut(new_child_id).parent, None);

        self.node_mut(new_child_id).parent = Some(parent_id);
        self.node_mut(parent_id)
            .children
            .insert(sibling_pos, new_child_id);
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let has_parent = self.node(*element).parent.is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // Ignore. We don't care about the DOCTYPE for now.
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        // TODO: implement templates properly. This should allow to function like regular elements.
        *target
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<html5ever::Attribute>) {
        let mut node = self.node_mut(*target);
        let element_data = node.element_data_mut().expect("Not an element");

        let existing_names = element_data
            .attrs
            .iter()
            .map(|e| e.name.clone())
            .collect::<HashSet<_>>();

        element_data.attrs.extend(
            attrs
                .into_iter()
                .map(html5ever_to_blitz_attr)
                .filter(|attr| !existing_names.contains(&attr.name)),
        );
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        let mut node = self.node_mut(*target);
        let parent_id = node.parent.take().expect("Node has no parent");
        self.node_mut(parent_id)
            .children
            .retain(|child_id| child_id != target);
    }

    fn reparent_children(&self, node_id: &Self::Handle, new_parent_id: &Self::Handle) {
        // Take children array from old parent
        let children = std::mem::take(&mut self.node_mut(*node_id).children);

        // Update parent reference of children
        for child_id in children.iter() {
            self.node_mut(*child_id).parent = Some(*new_parent_id);
        }

        // Add children to new parent
        self.node_mut(*new_parent_id).children.extend(&children);
    }
}
