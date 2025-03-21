use blitz_dom::BaseDocument;
use html5ever::tokenizer::states::State::Doctype;
use v8::{Context, Function, FunctionCallbackArguments, HandleScope, Local, Object, ReturnValue};

use super::{add_function_to_object, element::Element};

pub fn add_document(scope: &mut HandleScope<'_>, context: &Local<'_, Context>) {
    let document_name = v8::String::new(scope, "document").unwrap();
    let document_value = v8::Object::new(scope);

    add_function_to_object(scope, &document_value, "debug", debug);
    add_function_to_object(scope, &document_value, "querySelector", query_selector);

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

    let document = scope.get_slot::<BaseDocument>().unwrap();

    match document.query_selector(&selector) {
        Ok(Some(query)) => {
            let object = Element::new(query as u32).object(scope);
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

fn debug(
    scope: &mut HandleScope<'_>,
    _args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let document = scope.get_slot::<BaseDocument>().unwrap();
    document.print_tree();
    retval.set_undefined();
}
