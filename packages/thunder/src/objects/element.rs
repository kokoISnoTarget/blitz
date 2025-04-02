use std::ffi::c_void;

use blitz_dom::BaseDocument;
use v8::cppgc::GarbageCollected;
use v8::{
    self, Function, FunctionBuilder, FunctionCallbackArguments, FunctionTemplate, Global,
    HandleScope, Integer, Local, Object, ObjectTemplate, ReturnValue, Uint32,
};

use crate::fast_str;
use crate::util::OneByteConstExt;

use super::util::{add_rust_element_to_object, get_rust_element_from_object};
use super::*;

//pub fn element_object<'a>(scope: &mut HandleScope<'a>, id: u32) -> Local<'a, Object> {
//    let obj_template = ObjectTemplate::new(scope);
//    obj_template.set_internal_field_count(1);
//
//    let object = obj_template.new_instance(scope).unwrap();
//
//    add_node_id(scope, &object, id);
//    //add_rust_element_to_object(scope, &object, self);
//
//    add_function_to_object(scope, &object, "debug", debug);
//    add_function_to_object(scope, &object, "remove", remove);
//
//    object
//}

pub fn debug(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let obj = args.this();
    let Some(element) = scope.unwrap_element_object::<Element>(obj) else {
        #[cfg(feature = "tracing")]
        tracing::warn!("Failed to unwrap element object while debugging it");
        return;
    };
    let id = element.id;

    #[cfg(feature = "tracing")]
    tracing::info!("Element ID: {}", id);

    retval.set_uint32(id);
}

pub fn remove(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    _retval: ReturnValue<'_>,
) {
    let obj = args.this();
    let node_id = get_node_id(scope, &obj);
    #[cfg(feature = "tracing")]
    tracing::info!("Removing element with ID: {}", node_id);

    let document = scope.document_mut();
    document.remove_node(node_id as usize);
}
fn add_event_listener(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    _retval: ReturnValue<'_>,
) {
    let obj = args.this();
    let node_id = get_node_id(scope, &obj);

    #[cfg(feature = "tracing")]
    tracing::info!(
        "Adding event listener for node: {}, {:?}",
        node_id,
        scope.document().nodes[node_id as usize]
    );

    let event_type = args.get(0).to_string(scope).unwrap();
    let event_type = event_type.to_rust_string_lossy(scope);
    let event_listener = args.get(1);
    dbg!(&event_type);
    let event_listener = Global::new(scope, event_listener.cast());
    let _options = args.get(2); // TODO: Implement options
    let _use_capture = args.get(3); // TODO: Implement use_capture

    let element_listeners = scope.event_listeners_mut().entry(node_id).or_default();
    element_listeners.insert(event_type, event_listener);
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

pub fn set_element_template<'a>(scope: &mut HandleScope<'a>) {
    let template = ObjectTemplate::new(scope);
    template.set_internal_field_count(1);

    let remove_name = fast_str!("remove").to_v8(scope);
    let remove_function = Function::new(scope, remove).unwrap();
    template.set(remove_name.cast(), remove_function.cast());

    let debug_name = fast_str!("debug").to_v8(scope);
    let debug_function = Function::new(scope, debug).unwrap();
    template.set(debug_name.cast(), debug_function.cast());

    scope.set_obj_template::<Element>(template);
}

pub fn element_object<'a>(scope: &mut HandleScope<'a>, id: u32) -> Local<'a, Object> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(1);

    let object = template.new_instance(scope).unwrap();

    let remove_name = fast_str!("remove").to_v8(scope);
    let remove_function = Function::new(scope, remove).unwrap();
    object.set(scope, remove_name.cast(), remove_function.cast());

    let add_event_listener_name = fast_str!("addEventListener").to_v8(scope);
    let add_event_listener_function = Function::new(scope, add_event_listener).unwrap();
    object.set(
        scope,
        add_event_listener_name.cast(),
        add_event_listener_function.cast(),
    );

    let int = Integer::new_from_unsigned(scope, id);
    object.set_internal_field(0, int.into());
    object
}

struct Element {
    id: u32,
}
impl Tag for Element {
    const TAG: u16 = super::tag::ELEMENT;
}
impl GarbageCollected for Element {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}

    fn get_name(&self) -> Option<&'static std::ffi::CStr> {
        None
    }
}
impl Element {
    fn new(id: u32) -> Self {
        Element { id }
    }
}
