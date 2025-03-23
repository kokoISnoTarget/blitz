use std::ffi::c_void;

use blitz_dom::BaseDocument;
use v8::{
    External, FunctionCallbackArguments, HandleScope, Integer, Local, Object, ObjectTemplate,
    ReturnValue, Uint32,
};

use super::util::{add_rust_element_to_object, get_rust_element_from_object};
use super::*;

pub fn element_object<'a>(scope: &mut HandleScope<'a>, id: u32) -> Local<'a, Object> {
    let obj_template = ObjectTemplate::new(scope);
    obj_template.set_internal_field_count(1);

    let object = obj_template.new_instance(scope).unwrap();

    add_node_id(scope, &object, id);
    //add_rust_element_to_object(scope, &object, self);

    add_function_to_object(scope, &object, "debug", debug);
    add_function_to_object(scope, &object, "remove", remove);

    object
}

pub fn debug(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let this = args.this();
    let id = get_node_id(scope, &this);

    #[cfg(feature = "tracing")]
    tracing::info!("Element ID: {}", id);

    retval.set_uint32(id);
}

pub fn remove(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let this = args.this();
    let node_id = get_node_id(scope, &this);
    #[cfg(feature = "tracing")]
    tracing::info!("Removing element with ID: {}", node_id);

    let document = scope.get_slot_mut::<BaseDocument>().unwrap();
    document.remove_node(node_id as usize);
}

fn add_node_id(scope: &mut HandleScope<'_>, obj: &Object, id: u32) {
    let data = Integer::new_from_unsigned(scope, id);
    obj.set_internal_field(0, data.into());
}

fn get_node_id(scope: &mut HandleScope<'_>, obj: &Object) -> u32 {
    let data = obj.get_internal_field(scope, 0).unwrap();
    let id = data.cast::<Uint32>();
    id.value()
}
