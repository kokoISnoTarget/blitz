use std::{
    any::Any,
    borrow::Cow,
    cell::{Ref, RefCell, RefMut},
    collections::HashSet,
    sync::Arc,
};

use crate::{
    document::JsDocument,
    net::{OuterJsHandler, ThunderProvider},
};
use blitz_dom::{
    ElementNodeData, Node, NodeData, local_name,
    net::{CssHandler, ImageHandler, Resource},
    node::Attribute,
    util::ImageType,
};
use blitz_traits::net::{NetProvider, Request, SharedProvider};
use html5ever::{
    ParseOpts, QualName,
    interface::{NodeOrText, QuirksMode},
    tendril::TendrilSink,
    tokenizer::{BufferQueue, TokenSink, Tokenizer, TokenizerResult},
    tree_builder::TreeBuilder,
};
use html5ever::{
    interface::{ElementFlags, TreeSink},
    tendril::StrTendril,
};
use tokio::{runtime::Handle, task::block_in_place};
use url::Url;
use xml5ever::tendril::{fmt::UTF8, stream::Utf8LossyDecoder};

/// Convert an html5ever Attribute which uses tendril for its value to a blitz Attribute
/// which uses String.
fn html5ever_to_blitz_attr(attr: html5ever::Attribute) -> Attribute {
    Attribute {
        name: attr.name,
        value: attr.value.to_string(),
    }
}

pub struct HtmlParser<'a> {
    pub tokenizer: Tokenizer<TreeBuilder<usize, HtmlSink<'a>>>,
    pub input_buffer: BufferQueue,
}
impl TendrilSink<UTF8> for HtmlParser<'_> {
    fn process(&mut self, t: StrTendril) {
        self.input_buffer.push_back(t);
        self.drive_parser();
    }

    // FIXME: Is it too noisy to report every character decoding error?
    fn error(&mut self, desc: Cow<'static, str>) {
        self.tokenizer.sink.sink.parse_error(desc)
    }

    type Output = ();

    fn finish(mut self) -> Self::Output {
        self.drive_parser();
        assert!(self.input_buffer.is_empty());
        self.tokenizer.end();
        self.tokenizer.sink.sink.finish()
    }
}
impl HtmlParser<'_> {
    pub fn parse(doc: &mut JsDocument, net_provider: Arc<ThunderProvider>, html: &str) {
        let sink = HtmlSink::new(doc, net_provider);

        let opts = ParseOpts::default();

        let tb = TreeBuilder::new(sink, opts.tree_builder);
        let tok = Tokenizer::new(tb, opts.tokenizer);

        let parser = HtmlParser {
            tokenizer: tok,
            input_buffer: BufferQueue::default(),
        };

        Utf8LossyDecoder::new(parser)
            .read_from(&mut html.as_bytes())
            .unwrap()
    }

    fn drive_parser(&mut self) {
        while let TokenizerResult::Script(script_node_id) = self.tokenizer.feed(&self.input_buffer)
        {
            self.tokenizer.sink.sink.add_script(script_node_id);
        }
    }
}
pub struct HtmlSink<'a> {
    doc: RefCell<&'a mut JsDocument>,
    doc_id: usize,
    net_provider: Arc<ThunderProvider>,
}

impl<'a> HtmlSink<'a> {
    fn new(doc: &'a mut JsDocument, net_provider: Arc<ThunderProvider>) -> Self {
        let doc_id = doc.id();

        HtmlSink {
            doc: RefCell::new(doc),
            doc_id,
            net_provider,
        }
    }
    fn add_script(&mut self, node_id: usize) {
        let mut allows_parsing = false;
        let mut execute_after_fetch = true;
        let script_node = self.node(node_id);
        let attrs = script_node.attrs().unwrap();

        let is_async = attrs
            .iter()
            .any(|attr| matches!(attr.name.local, local_name!("async")));
        let is_module = attrs
            .iter()
            .any(|attr| matches!(attr.name.local, local_name!("type") if attr.value.to_lowercase() == "module"));
        let is_deferred = attrs
            .iter()
            .any(|attr| matches!(attr.name.local, local_name!("defer")));
        if is_deferred || is_module || is_async {
            allows_parsing = true;
        }
        if is_async {
            execute_after_fetch = true;
        } else if is_deferred || is_module {
            execute_after_fetch = false;
        }

        // let Some(src) = attrs
        // .iter()
        // .find(|attr| matches!(attr.name.local, local_name!("src")))
        // .map(|attr| attr.value.clone())
        // else {
        // let text_content = script_node.text_content();
        // return;
        // };

        // let str = if !allows_parsing {
        // let result = block_in_place(self.net_provider.fetch_imediate(Request::get(src)));

        // Some(result)
        // } else  if {
        // };

        let src = attrs
            .iter()
            .find(|attr| matches!(attr.name.local, local_name!("src")))
            .map(|attr| attr.value.clone());
        if let Some(src) = src {
            let url = self.doc.borrow().resolve_url(&src);
            if is_async {
                self.net_provider.fetch(
                    self.doc_id,
                    Request::get(url),
                    Box::new(OuterJsHandler {
                        node_id,
                        defer: !execute_after_fetch,
                    }),
                );
            } else {
                let bytes =
                    Handle::current().block_on(self.net_provider.fetch_imediate(Request::get(url)));

                let mut doc = self.doc.borrow_mut();
                doc.add_script(bytes, is_module, execute_after_fetch);
            }
        }
    }

    #[track_caller]
    fn create_node(&self, node_data: NodeData) -> usize {
        self.doc.borrow_mut().create_node(node_data)
    }

    #[track_caller]
    fn create_text_node(&self, text: &str) -> usize {
        self.doc.borrow_mut().create_text_node(text)
    }

    #[track_caller]
    fn node(&self, id: usize) -> Ref<Node> {
        Ref::map(self.doc.borrow(), |doc| &doc.nodes[id])
    }

    #[track_caller]
    fn node_mut(&self, id: usize) -> RefMut<Node> {
        RefMut::map(self.doc.borrow_mut(), |doc| &mut doc.nodes[id])
    }

    fn try_append_text_to_text_node(&self, node_id: Option<usize>, text: &str) -> bool {
        let Some(node_id) = node_id else {
            return false;
        };
        let mut node = self.node_mut(node_id);

        match node.text_data_mut() {
            Some(data) => {
                data.content += text;
                true
            }
            None => false,
        }
    }

    fn last_child(&self, parent_id: usize) -> Option<usize> {
        self.node(parent_id).children.last().copied()
    }

    fn load_linked_stylesheet(&self, target_id: usize) {
        let node = self.node(target_id);

        let rel_attr = node.attr(local_name!("rel"));
        let href_attr = node.attr(local_name!("href"));

        if let (Some("stylesheet"), Some(href)) = (rel_attr, href_attr) {
            let url = self.doc.borrow().resolve_url(href);
            self.net_provider.fetch(
                self.doc_id,
                Request::get(url.clone()),
                Box::new(CssHandler {
                    node: target_id,
                    source_url: url,
                    guard: self.doc.borrow().guard.clone(),
                    provider: self.net_provider.clone(),
                }),
            );
        }
    }

    fn load_image(&self, target_id: usize) {
        let node = self.node(target_id);
        if let Some(raw_src) = node.attr(local_name!("src")) {
            if !raw_src.is_empty() {
                let src = self.doc.borrow().resolve_url(raw_src);
                self.doc.borrow().net_provider.fetch(
                    self.doc.borrow().id(),
                    Request::get(src),
                    Box::new(ImageHandler::new(target_id, ImageType::Image)),
                );
            }
        }
    }

    fn process_button_input(&self, target_id: usize) {
        let node = self.node(target_id);
        let Some(data) = node.element_data() else {
            return;
        };

        let tagname = data.name.local.as_ref();
        let type_attr = data.attr(local_name!("type"));
        let value = data.attr(local_name!("value"));

        // Add content of "value" attribute as a text node child if:
        //   - Tag name is
        if let ("input", Some("button" | "submit" | "reset"), Some(value)) =
            (tagname, type_attr, value)
        {
            let value = value.to_string();
            drop(node);
            let id = self.create_text_node(&value);
            self.append(&target_id, NodeOrText::AppendNode(id));
        }
    }
}

// This is from https://github.com/DioxusLabs/blitz/blob/36369ba285d7291b449d9d7770427fc895dc5221/packages/blitz-html/src/html_sink.rs
impl<'b> TreeSink for HtmlSink<'b> {
    type Output = ();

    // we use the ID of the nodes in the tree as the handle
    type Handle = usize;

    type ElemName<'a>
        = Ref<'a, QualName>
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        // Add inline stylesheets (<style> elements)
        //for id in self.style_nodes.borrow().iter() {
        //    doc.process_style_element(*id);
        //}
        // TODO: Implement style processing

        // for error in self.errors.borrow().iter() {
        //     println!("ERROR: {}", error);
        // }
    }

    fn parse_error(&self, msg: Cow<'static, str>) {
        #[cfg(feature = "tracing")]
        tracing::error!("Parse error: {}", msg);
    }

    fn get_document(&self) -> Self::Handle {
        0
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        Ref::map(self.doc.borrow(), |doc| {
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
            "style" => {
                todo!();
                //self.style_nodes.borrow_mut().push(id)
            }
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
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        #[cfg(feature = "tracing")]
        tracing::warn!(
            "Trying to append DOCTYPE to document, which is not supported yet. {name}, {public_id}, {system_id}."
        );
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        // TODO: implement templates properly. This should allow to function like regular elements.
        *target
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        #[cfg(feature = "tracing")]
        tracing::warn!("Trying to set quirks mode to {mode:?}, which is not supported yet.");
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
