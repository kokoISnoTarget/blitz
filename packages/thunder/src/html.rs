use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashSet,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::Waker,
};

use crate::{
    fetch_thread::ScriptOptions,
    objects::{IsolateExt, IsolatePtr},
};
use blitz_dom::{
    ElementNodeData, Node, NodeData, local_name,
    net::{CssHandler, ImageHandler},
    node::Attribute,
    util::ImageType,
};
use blitz_traits::net::Request;
use html5ever::{
    ParseOpts, QualName,
    interface::{NodeOrText, QuirksMode},
    tokenizer::{BufferQueue, Tokenizer, TokenizerResult},
    tree_builder::TreeBuilder,
};
use html5ever::{
    interface::{ElementFlags, TreeSink},
    tendril::StrTendril,
};
use v8::{Isolate, script_compiler::CompileOptions};

/// Convert an html5ever Attribute which uses tendril for its value to a blitz Attribute
/// which uses String.
fn html5ever_to_blitz_attr(attr: html5ever::Attribute) -> Attribute {
    Attribute {
        name: attr.name,
        value: attr.value.to_string(),
    }
}

pub struct HtmlParser {
    pub tokenizer: Tokenizer<TreeBuilder<usize, HtmlSink>>,
    pub input_buffer: BufferQueue,
    pub should_parse: ShouldParse,
    pub finished: bool,
}

impl HtmlParser {
    pub fn new(isolate: &mut Isolate) -> HtmlParser {
        let sink = HtmlSink::new(isolate);

        let opts = ParseOpts::default();

        let tb = TreeBuilder::new(sink, opts.tree_builder);
        let tok = Tokenizer::new(tb, opts.tokenizer);

        HtmlParser {
            tokenizer: tok,
            input_buffer: BufferQueue::default(),
            should_parse: ShouldParse::new(),
            finished: false,
        }
    }

    pub async fn finish_async(&mut self) {
        self.drive_parser().await;
        assert!(self.input_buffer.is_empty());
        self.tokenizer.end();
        self.tokenizer.sink.sink.finish_async().await;
    }
    pub async fn drive_parser(&mut self) {
        loop {
            // Wait for running scripts and some resources to be fetched
            self.should_parse.iref().await;
            let result = self.tokenizer.feed(&self.input_buffer);
            if let TokenizerResult::Script(script_node_id) = result {
                self.tokenizer.sink.sink.add_script(script_node_id);
            } else {
                break;
            }
        }
    }
}
pub struct HtmlSink {
    isolate: IsolatePtr,
    doc_id: usize,
    style_nodes: RefCell<Vec<usize>>,
}

impl HtmlSink {
    fn new(isolate: &mut Isolate) -> HtmlSink {
        let doc_id = isolate.document().id();

        HtmlSink {
            isolate: isolate.ptr(),
            doc_id,
            style_nodes: RefCell::new(Vec::new()),
        }
    }
    async fn finish_async(&mut self) {
        let doc = self.isolate.document_mut();
        for id in self.style_nodes.borrow().iter() {
            doc.process_style_element(*id);
        }
    }
    fn add_script(&mut self, node_id: usize) {
        let script_node = self.node(node_id);
        let attrs = script_node.attrs().unwrap();

        let is_async = attrs
            .iter()
            .any(|attr| matches!(attr.name.local, local_name!("async")));
        let is_module = attrs
            .iter()
            .any(|attr| matches!(attr.name.local, local_name!("type") if attr.value.to_lowercase() == "module"));
        let is_defer = attrs
            .iter()
            .any(|attr| matches!(attr.name.local, local_name!("defer")));

        let src = attrs
            .iter()
            .find(|attr| matches!(attr.name.local, local_name!("src")))
            .map(|attr| attr.value.clone());

        if let Some(src) = src {
            let url = self.isolate.document().resolve_url(&src);
            self.isolate.fetch_thread().fetch(ScriptOptions {
                url,
                is_module,
                is_defer,
                is_async,
            });
        } else {
            let script = script_node.text_content();
            if is_defer {
                todo!();
            }

            let mut scope = self.isolate.context_scope();

            let script = v8::String::new(&mut scope, &script).unwrap();
            let mut source = v8::script_compiler::Source::new(script, None);

            let mut try_catch = v8::TryCatch::new(&mut scope);
            let failed = v8::script_compiler::compile(
                &mut try_catch,
                &mut source,
                CompileOptions::EagerCompile,
                v8::script_compiler::NoCacheReason::NoReason,
            )
            .unwrap()
            .run(&mut try_catch)
            .is_none();

            if failed {
                let stack_trace = try_catch
                    .stack_trace()
                    .or_else(|| try_catch.exception())
                    .map_or_else(
                        || "no stack trace".into(),
                        |value| value.to_rust_string_lossy(&mut try_catch),
                    );
                #[cfg(feature = "tracing")]
                tracing::error!("Running script failed: \n{}", stack_trace);
            }
        }
    }

    #[track_caller]
    fn create_node(&self, node_data: NodeData) -> usize {
        self.isolate.document_mut_from_ref().create_node(node_data)
    }

    #[track_caller]
    fn create_text_node(&self, text: &str) -> usize {
        self.isolate.document_mut_from_ref().create_text_node(text)
    }

    #[track_caller]
    fn node(&self, id: usize) -> &Node {
        &self.isolate.document().nodes[id]
    }

    #[track_caller]
    fn node_mut(&self, id: usize) -> &mut Node {
        &mut self.isolate.document_mut_from_ref().nodes[id]
    }

    fn try_append_text_to_text_node(&self, node_id: Option<usize>, text: &str) -> bool {
        let Some(node_id) = node_id else {
            return false;
        };
        let node = self.node_mut(node_id);

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
            let url = self.isolate.document().resolve_url(href);
            self.isolate.document().net_provider.fetch(
                self.doc_id,
                Request::get(url.clone()),
                Box::new(CssHandler {
                    node: target_id,
                    source_url: url,
                    guard: self.isolate.document().guard.clone(),
                    provider: self.isolate.document().net_provider.clone(),
                }),
            );
        }
    }

    fn load_image(&self, target_id: usize) {
        let node = self.node(target_id);
        if let Some(raw_src) = node.attr(local_name!("src")) {
            if !raw_src.is_empty() {
                let src = self.isolate.document().resolve_url(raw_src);
                self.isolate.document().net_provider.fetch(
                    self.isolate.document().id(),
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
            _ = node;
            let id = self.create_text_node(&value);
            self.append(&target_id, NodeOrText::AppendNode(id));
        }
    }
}

// This is from https://github.com/DioxusLabs/blitz/blob/36369ba285d7291b449d9d7770427fc895dc5221/packages/blitz-html/src/html_sink.rs
impl TreeSink for HtmlSink {
    type Output = ();

    // we use the ID of the nodes in the tree as the handle
    type Handle = usize;

    type ElemName<'a>
        = &'a QualName
    where
        Self: 'a;

    fn finish(mut self) -> Self::Output {
        // Add inline stylesheets (<style> elements)
        let doc = self.isolate.document_mut();
        for id in self.style_nodes.borrow().iter() {
            doc.process_style_element(*id);
        }
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
        &self.isolate.document().nodes[*target]
            .element_data()
            .expect("TreeSink::elem_name called on a node which is not an element!")
            .name
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<html5ever::Attribute>,
        _flags: ElementFlags,
    ) -> Self::Handle {
        let attrs = attrs.into_iter().map(html5ever_to_blitz_attr).collect();
        let mut data = ElementNodeData::new(name.clone(), attrs);
        data.flush_style_attribute(&self.isolate.document().guard);

        let id = self.create_node(NodeData::Element(data));
        let node = self.node(id);

        // Initialise style data
        *node.stylo_element_data.borrow_mut() = Some(Default::default());

        let id_attr = node.attr(local_name!("id")).map(|id| id.to_string());
        _ = node;

        // If the node has an "id" attribute, store it in the ID map.
        if let Some(id_attr) = id_attr {
            self.isolate
                .document_mut_from_ref()
                .nodes_to_id
                .insert(id_attr, id);
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
        let node = self.node_mut(*target);
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
        let node = self.node_mut(*target);
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

#[derive(Debug, Clone)]
pub struct ShouldParse(pub(crate) Arc<ShouldParseInner>);
impl ShouldParse {
    pub fn new() -> Self {
        Self(Arc::new(ShouldParseInner {
            waker: Mutex::default(),
            state: AtomicBool::new(true),
        }))
    }
    pub fn iref(&self) -> ShouldParseRef {
        ShouldParseRef(&self.0)
    }
}

#[derive(Debug)]
pub struct ShouldParseInner {
    pub waker: Mutex<Option<Waker>>,
    pub state: AtomicBool,
}

pub struct ShouldParseRef<'a>(&'a ShouldParseInner);

impl Future for ShouldParseRef<'_> {
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        let inner = this.0;

        if inner.state.load(Ordering::Relaxed) {
            std::task::Poll::Ready(())
        } else {
            *inner.waker.lock().unwrap() = Some(cx.waker().clone());
            std::task::Poll::Pending
        }
    }
}
