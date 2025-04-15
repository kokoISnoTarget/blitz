use crate::{
    application::{EventProxy, ThunderEventType},
    fast_str,
    module::{host_import_module_dynamically_callback, initialize_import_meta_object_callback},
    objects::{
        Element, EventObject, WrappedObject, add_console, add_document, add_window, init_js_files,
        init_templates,
    },
    rusty_v8_ext::HostInitializeImportMetaObjectCallback,
    util::OneByteConstExt,
    v8intergration::{GlobalState, HandleScopeExt, IsolateExt},
};
use blitz_dom::BaseDocument;
use blitz_shell::BlitzShellEvent;
use blitz_traits::{Document, DomEvent, Viewport};
use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use url::Url;
use v8::{
    Context, ContextOptions, ContextScope, Function, Global, HandleScope, Isolate, Local, Object,
    OwnedIsolate, Value,
};
use winit::{event_loop::EventLoopProxy, window::WindowId};
use xml5ever::tendril::StrTendril;

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

    fn poll(&mut self, cx: std::task::Context) -> bool {
        self.run_script_queue();

        let parser = self.isolate.parser();
        if parser.finished {
            return false;
        }
        parser.feed();
        parser.try_finish();
        true
    }
}
impl From<JsDocument> for BaseDocument {
    fn from(mut js_doc: JsDocument) -> BaseDocument {
        js_doc.isolate.remove_global_state().unwrap().document
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
    pub(crate) fn thunder_event(&mut self, event: &ThunderEventType) {
        match event {
            ThunderEventType::ScriptFetched { script_id, content } => todo!(),
            ThunderEventType::ModuleFetched {
                parent_id,
                module_id,
                content,
            } => todo!(),
            ThunderEventType::DocumentFetched { url, bytes } => {
                self.set_base_url(&url);
                self.add_source(String::from_utf8(bytes.to_vec()).unwrap());
            }
        }
    }
    pub(crate) fn resume(&self, window_id: &WindowId) {
        self.isolate.fetch_thread().set_window(window_id.clone());
    }
    fn run_script_queue(&mut self) {
        //let len = self.script_queue.len();
        //let mut buf = Vec::with_capacity(len);
        //self.script_queue.blocking_recv_many(&mut buf, len);
        //for script in buf.drain(..) {}
    }

    pub fn add_source(&mut self, source: impl Into<StrTendril>) {
        let parser = self.isolate.parser();
        parser.input_buffer.push_back(source.into());
    }
    pub fn new(mut isolate: OwnedIsolate, proxy: EventLoopProxy<BlitzShellEvent>) -> JsDocument {
        let mut document = BaseDocument::new(Viewport::default());
        document.add_user_agent_stylesheet(blitz_dom::DEFAULT_CSS);

        let event_proxy = EventProxy::new(proxy);
        let global_state = GlobalState::new(&mut isolate, document, event_proxy);
        isolate.set_global_state(global_state);

        Self::initialize(&mut isolate);

        #[cfg(feature = "tracing")]
        tracing::info!("Initialized JsDocument");

        JsDocument { isolate }
    }

    // Setup global
    pub fn initialize(isolate: &mut Isolate) {
        isolate.set_host_initialize_import_meta_object_callback(
            initialize_import_meta_object_callback.to_c_fn(),
        );
        isolate
            .set_host_import_module_dynamically_callback(host_import_module_dynamically_callback);

        let mut scope = isolate.context_scope();
        init_templates(&mut scope);
        let global = scope.global_this();
        add_document(&mut scope, global);
        add_console(&mut scope, global);
        add_window(&mut scope, global);

        init_js_files(&mut scope);
    }

    pub(crate) fn handle_js_event_listener(
        &mut self,
        event: &mut DomEvent,
        listener: Global<Value>,
    ) -> bool {
        #[cfg(feature = "tracing")]
        tracing::info!("using event listener {:?}", event);
        let mut scope = self.isolate.context_scope();

        let even_object = EventObject::new(event.clone()).object(&mut scope);
        let receiver = Element::new(event.target as u32).object(&mut scope);

        let listener = Local::new(&mut scope, listener);

        if listener.is_function() {
            let function: Local<Function> = listener.cast();

            function
                .call(&mut scope, receiver.cast(), &[even_object.cast()])
                .unwrap();
            return true;
        } else if listener.is_object() {
            let object: Local<Object> = listener.cast();
            let func_name = fast_str!("handleEvent").to_v8(&mut scope);
            let func = object.get(&mut scope, func_name.cast());
            if let Some(func) = func
                && func.is_function()
            {
                let function: Local<Function> = func.cast();
                dbg!(function.call(&mut scope, receiver.cast(), &[even_object.cast()]));
                return true;
            }
        }
        false
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
        if let Some(global_state) = self.isolate.remove_global_state() {
            drop(global_state);
        }
    }
}
