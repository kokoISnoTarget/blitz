use std::ffi::c_void;

use blitz_dom::BaseDocument;
use v8::{
    External, FunctionCallbackArguments, HandleScope, Local, Object, ObjectTemplate, ReturnValue,
};

use super::util::{add_rust_element_to_object, get_rust_element_from_object};
use super::*;

pub struct Element {
    id: u32,
}

impl Element {
    pub fn new(id: u32) -> Self {
        Element { id }
    }

    pub fn object<'a>(self, scope: &mut HandleScope<'a>) -> Local<'a, Object> {
        let obj_template = ObjectTemplate::new(scope);
        obj_template.set_internal_field_count(1);

        let object = obj_template.new_instance(scope).unwrap();

        add_rust_element_to_object(scope, &object, self);

        add_function_to_object(scope, &object, "debug", Self::debug);
        add_function_to_object(scope, &object, "remove", Self::remove);

        object
    }

    pub fn debug(
        scope: &mut HandleScope<'_>,
        args: FunctionCallbackArguments<'_>,
        mut retval: ReturnValue<'_>,
    ) {
        let this = args.this();
        let element = get_rust_element_from_object::<Element>(scope, &this).unwrap();

        #[cfg(feature = "tracing")]
        tracing::info!("Element ID: {}", element.id);

        retval.set_uint32(element.id);
    }

    pub fn remove(
        scope: &mut HandleScope<'_>,
        args: FunctionCallbackArguments<'_>,
        mut retval: ReturnValue<'_>,
    ) {
        let this = args.this();
        let element = get_rust_element_from_object::<Element>(scope, &this).unwrap();
        let id = element.id;
        #[cfg(feature = "tracing")]
        tracing::info!("Removing element with ID: {}", element.id);

        let document = scope.get_slot_mut::<BaseDocument>().unwrap();
        document.remove_node(id as usize);
    }
}
