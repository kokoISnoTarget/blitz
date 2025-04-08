use blitz_dom::BaseDocument;

use v8::{
    self, Context, Function, FunctionCallbackArguments, Global, HandleScope, Isolate, Local,
    Object, ReturnValue, Value,
};

use crate::objects::IsolateExt;

use super::{
    add_function_to_object,
    element::{element_object, set_element_template},
};

pub fn add_document(scope: &mut HandleScope<'_>, context: &Local<'_, Context>) {
    let document_name = v8::String::new(scope, "document").unwrap();
    let document_value = v8::Object::new(scope);

    add_function_to_object(scope, &document_value, "querySelector", query_selector);
    add_function_to_object(scope, &document_value, "getElementById", get_element_by_id);
    add_function_to_object(scope, &document_value, "debugTree", debug_tree);

    let global = context.global(scope);
    global
        .set(scope, document_name.into(), document_value.into())
        .unwrap();
}

fn query_selector(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let Some(selector) = args.get(0).to_string(scope) else {
        return;
    };
    let selector = selector.to_rust_string_lossy(scope);

    let document = scope.document();

    match document.query_selector(&selector) {
        Ok(Some(query)) => {
            let object = element_object(scope, query as u32);
            retval.set(object.into());
        }
        Ok(None) => {
            retval.set_null();
        }
        Err(err) => {
            let error = v8::String::new(scope, &format!("{err:?}")).unwrap();
            let exception = v8::Exception::syntax_error(scope, error.into());
            scope.throw_exception(exception);
            retval.set_undefined();
        }
    }
}

fn get_element_by_id(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let Some(id) = args.get(0).to_string(scope) else {
        return;
    };
    let id = id.to_rust_string_lossy(scope);

    let document = scope.document();

    match document.nodes_to_id.get(&id) {
        Some(&element) => {
            let object = element_object(scope, element as u32);
            retval.set(object.into());
        }
        None => {
            retval.set_null();
        }
    }
}

fn debug_tree(
    scope: &mut HandleScope<'_>,
    _args: FunctionCallbackArguments<'_>,
    _retval: ReturnValue<'_>,
) {
    scope.document().print_tree();
}
