use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    pin::Pin,
    rc::Rc,
};

use crate::objects::{self, add_console, add_document};
use blitz_dom::BaseDocument;
use blitz_traits::{Document, Viewport};
use v8::{
    Context, ContextOptions, ContextScope, CreateParams, Exception, FunctionCallbackArguments,
    FunctionTemplate, HandleScope, Isolate, ObjectTemplate, OwnedIsolate, ReturnValue,
    cppgc::{Heap, make_garbage_collected, shutdown_process},
};

pub struct JsDocument {
    pub isolate: OwnedIsolate,
}

impl Document for JsDocument {
    type Doc = BaseDocument;

    fn handle_event(&mut self, _event: &mut blitz_traits::DomEvent) {}

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
        js_doc.isolate.remove_slot::<BaseDocument>().unwrap()
    }
}
impl AsRef<BaseDocument> for JsDocument {
    fn as_ref(&self) -> &BaseDocument {
        self.isolate.get_slot::<BaseDocument>().unwrap()
    }
}
impl AsMut<BaseDocument> for JsDocument {
    fn as_mut(&mut self) -> &mut BaseDocument {
        self.isolate.get_slot_mut::<BaseDocument>().unwrap()
    }
}

impl JsDocument {
    pub fn new(mut isolate: OwnedIsolate) -> Self {
        let document = BaseDocument::new(Viewport::default());

        isolate.set_slot(document);
        Self { isolate }
    }

    pub fn setup(&mut self) {
        let handle_scope = &mut HandleScope::new(&mut self.isolate);

        let context = Context::new(handle_scope, ContextOptions::default());
        let scope = &mut ContextScope::new(handle_scope, context);

        add_console(scope, &context);
        add_document(scope, &context);

        #[cfg(feature = "tracing")]
        tracing::info!("Set global scope");

        let source = v8::String::new(
            scope,
            r#"
            let body = document.querySelector('body');
            body.remove();
            "#,
        )
        .unwrap();
        execute_script(scope, source);
    }
}

fn execute_script(
    context_scope: &mut v8::ContextScope<v8::HandleScope>,
    script: v8::Local<v8::String>,
) {
    let scope = &mut v8::HandleScope::new(context_scope);
    let mut try_catch = v8::TryCatch::new(scope);

    let script =
        v8::Script::compile(&mut try_catch, script, None).expect("failed to compile script");

    let result = script.run(&mut try_catch);
    let Some(result) = result else {
        let exception_string = try_catch
            .stack_trace()
            .or_else(|| try_catch.exception())
            .map_or_else(
                || "no stack trace".into(),
                |value| value.to_rust_string_lossy(&mut try_catch),
            );

        panic!("{exception_string}");
    };
    #[cfg(feature = "tracing")]
    tracing::info!("Executed script");
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
