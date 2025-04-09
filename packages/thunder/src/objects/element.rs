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
    let node_id = scope
        .unwrap_element_object::<Element>(args.this())
        .unwrap()
        .id;

    #[cfg(feature = "tracing")]
    tracing::info!("Element ID: {}", node_id);

    retval.set_uint32(node_id);
}

pub fn remove(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    _retval: ReturnValue<'_>,
) {
    //let obj = args.this();
    //let node_id = get_node_id(scope, &obj);
    let node_id = scope
        .unwrap_element_object::<Element>(args.this())
        .unwrap()
        .id;

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
    //let obj = args.this();
    //let node_id = get_node_id(scope, &obj);
    let node_id = scope
        .unwrap_element_object::<Element>(args.this())
        .unwrap()
        .id;

    #[cfg(feature = "tracing")]
    tracing::warn!(
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

pub fn set_element_template<'a>(scope: &mut HandleScope<'a>) {
    let template = FunctionTemplate::new(scope, empty);
    let proto = template.prototype_template(scope);
    proto.set_internal_field_count(1);

    let remove_name = fast_str!("remove").to_v8(scope);
    let remove_function = FunctionTemplate::new(scope, remove);
    proto.set(remove_name.cast(), remove_function.cast());

    let debug_name = fast_str!("debug").to_v8(scope);
    let debug_function = FunctionTemplate::new(scope, debug);
    proto.set(debug_name.cast(), debug_function.cast());

    let add_event_listener_name = fast_str!("addEventListener").to_v8(scope);
    let add_event_listener_function = FunctionTemplate::new(scope, add_event_listener);
    proto.set(
        add_event_listener_name.cast(),
        add_event_listener_function.cast(),
    );

    scope.set_fn_template::<Element>(template);
}

pub fn element_object<'a>(scope: &mut HandleScope<'a>, id: u32) -> Local<'a, Object> {
    scope.create_wrapped_object(Element::new(id))
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
