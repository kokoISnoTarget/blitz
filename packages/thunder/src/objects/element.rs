use super::*;

pub fn debug(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let node_id = scope.unwrap_object::<Element>(args.this()).unwrap().id;

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
    let node_id = scope.unwrap_object::<Element>(args.this()).unwrap().id;

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
    let node_id = scope.unwrap_object::<Element>(args.this()).unwrap().id;

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

pub struct Element {
    id: u32,
}
impl WrappedObject for Element {
    const TAG: u16 = super::ELEMENT;

    fn init_template<'s>(scope: &mut HandleScope<'s>, proto: Local<ObjectTemplate>) {
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
    }
}
impl GarbageCollected for Element {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}

    fn get_name(&self) -> Option<&'static std::ffi::CStr> {
        None
    }
}
impl Element {
    pub fn new(id: u32) -> Self {
        Element { id }
    }
}
