use crate::{
    objects::{HandleScopeExt, init_js_files},
    util::OneByteConstExt,
};
use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    pin::Pin,
    rc::Rc,
};

use crate::objects::element::element_object;
use crate::objects::event_object;
use crate::{
    HtmlParser, fast_str,
    fetch_thread::init_fetch_thread,
    objects::{self, IsolateExt, add_console, add_document, add_window},
};
use blitz_dom::BaseDocument;
use blitz_traits::{Document, DomEvent, Viewport, net::Bytes};
use tokio::runtime::Runtime;
use v8::{
    self, Context, ContextOptions, ContextScope, CreateParams, Exception, Function,
    FunctionCallbackArguments, FunctionTemplate, Global, HandleScope, Isolate, Local,
    NewStringType, Object, ObjectTemplate, OwnedIsolate, ReturnValue, Value,
    cppgc::{Heap, make_garbage_collected, shutdown_process},
    inspector::{ChannelImpl, V8Inspector, V8InspectorClientBase, V8InspectorClientImpl},
    undefined,
};
use xml5ever::tendril::{Tendril, TendrilSink};

pub struct JsDocument {
    pub(crate) isolate: OwnedIsolate,
}

impl Document for JsDocument {
    type Doc = BaseDocument;

    fn handle_event(&mut self, event: &mut blitz_traits::DomEvent) -> bool {
        let mut chain = if event.bubbles {
            self.isolate.document().node_chain(event.target).into_iter()
        } else {
            vec![event.target].into_iter()
        };

        while let Some(next) = chain.next() {
            if let Some(node_event_listeners) = self.isolate.event_listeners().get(&(next as u32)) {
                if let Some(event_listener) = node_event_listeners.get(&event.name().to_string()) {
                    if self.handle_js_event_listener(event, event_listener.clone()) {
                        return true;
                    }
                }
            }
            event.target = next;
            if self.isolate.document_mut().handle_event(event) {
                return true;
            }
        }
        false
    }

    fn id(&self) -> usize {
        self.as_ref().id()
    }

    fn poll(&mut self, _cx: std::task::Context) -> bool {
        // Default implementation does nothing
        false
    }
}
impl From<JsDocument> for BaseDocument {
    fn from(mut js_doc: JsDocument) -> BaseDocument {
        js_doc.isolate.clear_document()
    }
}
impl AsRef<BaseDocument> for JsDocument {
    fn as_ref(&self) -> &BaseDocument {
        self.isolate.document()
    }
}
impl AsMut<BaseDocument> for JsDocument {
    fn as_mut(&mut self) -> &mut BaseDocument {
        self.isolate.document_mut()
    }
}

impl JsDocument {
    pub async fn parse(&mut self, source: &str) {
        let parser = self.isolate.parser();
        parser.input_buffer.push_back(source.into());
        parser.finish_async().await;
    }
    pub fn new(mut isolate: OwnedIsolate) -> JsDocument {
        let mut document = BaseDocument::new(Viewport::default());

        document.add_user_agent_stylesheet(blitz_dom::DEFAULT_CSS);

        isolate.set_document(document);
        isolate.setup_templates();
        isolate.setup_listeners();

        let mut scope = HandleScope::new(&mut isolate);
        let context = Context::new(&mut scope, ContextOptions::default());
        Self::initialize(&mut scope, context);

        let context = Global::new(&mut scope, context);
        scope.set_slot(context);
        drop(scope);

        let parser = HtmlParser::new(isolate.as_mut());
        isolate.set_parser(parser);

        init_fetch_thread(&mut isolate);

        JsDocument { isolate }
    }

    // Setup global
    pub fn initialize(scope: &mut HandleScope<'_, ()>, context: Local<Context>) {
        let mut scope = ContextScope::new(scope, context);
        scope.init_templates();
        add_document(&mut scope, &context);
        add_console(&mut scope, &context);
        add_window(&mut scope, &context);

        init_js_files(&mut scope);
    }

    pub(crate) fn handle_js_event_listener(
        &mut self,
        event: &mut DomEvent,
        listener: Global<Value>,
    ) -> bool {
        let mut handled = false;
        #[cfg(feature = "tracing")]
        tracing::info!("using event listener {:?}", event);
        let context = self.isolate.remove_slot::<Global<Context>>().unwrap();
        let mut scope = HandleScope::with_context(&mut self.isolate, &context);

        let even_object = event_object(&mut scope, event.clone());
        let receiver = element_object(&mut scope, event.target as u32);

        let listener = Local::new(&mut scope, listener);

        if listener.is_function() {
            let function: Local<Function> = listener.cast();

            function
                .call(&mut scope, receiver.cast(), &[even_object.cast()])
                .unwrap();
            handled = true;
        } else if listener.is_object() {
            let object: Local<Object> = listener.cast();
            let func_name = fast_str!("handleEvent").to_v8(&mut scope);
            let func = object.get(&mut scope, func_name.cast());
            if let Some(func) = func
                && func.is_function()
            {
                let function: Local<Function> = func.cast();
                dbg!(function.call(&mut scope, receiver.cast(), &[even_object.cast()]));
                handled = true;
            }
        }

        scope.set_slot(context);
        handled
    }
}
impl Deref for JsDocument {
    type Target = BaseDocument;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}
impl DerefMut for JsDocument {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl Drop for JsDocument {
    fn drop(&mut self) {
        let isolate = &mut self.isolate;
        isolate.clear_document();
        isolate.clear_templates();
        isolate.clear_listeners();
    }
}
