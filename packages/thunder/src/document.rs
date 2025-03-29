use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    pin::Pin,
    rc::Rc,
};

use crate::{net::ThunderProvider, objects::{self, add_console, add_document, IsolateExt}};
use blitz_dom::BaseDocument;
use blitz_traits::{Document, Viewport, net::Bytes};
use v8::{
    cppgc::{make_garbage_collected, shutdown_process, Heap}, Context, ContextOptions, ContextScope, CreateParams, Exception, FunctionCallbackArguments, FunctionTemplate, Global, HandleScope, Isolate, Local, NewStringType, ObjectTemplate, OwnedIsolate, ReturnValue
};

pub struct JsDocument {
    context: Global<Context>,
    isolate: OwnedIsolate,
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
impl<'a> From<JsDocument> for BaseDocument {
    fn from(mut js_doc: JsDocument<'a>) -> BaseDocument {
        js_doc.isolate.clear_document()
    }
}
impl<'a> AsRef<BaseDocument> for JsDocument<'a> {
    fn as_ref(&self) -> &BaseDocument {
        self.isolate.document()
    }
}
impl<'a> AsMut<BaseDocument> for JsDocument<'a> {
    fn as_mut(&mut self) -> &mut BaseDocument {
        self.isolate.document_mut()
    }
}

impl JsDocument {
    pub fn new(mut isolate: OwnedIsolate) -> JsDocument {
        let document = BaseDocument::new(Viewport::default());

        isolate.set_document(document);
        isolate.setup_templates();

        let mut scope = HandleScope::new(&mut isolate);
        let context = Context::new(&mut scope, ContextOptions::default());

        Self::initialize(&mut scope, context);
        let context = Global::new(&mut scope, context);

        drop(scope);

        JsDocument { context, isolate }
    }

    // Setup global
    pub fn initialize(scope: &mut HandleScope<'_, ()>, context: Local<Context>) {
        let mut scope = ContextScope::new(scope, context);

        add_console(&mut scope, &context);
        add_document(&mut scope, &context);
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
            console.log(body.remove());
            "#,
        )
        .unwrap();
        execute_script(scope, source);
    }

    pub(crate) fn add_script(
        &mut self,
        script: Bytes,
        is_module: bool,
        executed_after_fetch: bool,
    ) {
        let scope = &mut v8::HandleScope::new(&mut self.isolate);
        let string = v8::String::new_from_utf8(scope, &script, NewStringType::Normal).unwrap();

        if is_module {
            v8::Script::compile(scope, source, origin)
            v8::Module::create_synthetic_module(scope, module_name, export_names, evaluation_steps)
        }
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

impl Drop for JsDocument {
    fn drop(&mut self) {
        let isolate = &mut self.isolate;
        isolate.clear_document();
        isolate.clear_templates();
    }
}
