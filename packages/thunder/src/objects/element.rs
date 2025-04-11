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

fn query_selector(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let element = scope.unwrap_object::<Element>(args.this()).unwrap();
    let node_id = element.id as usize;

    let Some(selector) = args.get(0).to_string(scope) else {
        return;
    };
    let selector = selector.to_rust_string_lossy(scope);

    let document = scope.document();

    match document.query_selector_from(node_id, &selector) {
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

fn query_selector_all(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let element = scope.unwrap_object::<Element>(args.this()).unwrap();
    let node_id = element.id as usize;

    let Some(selector) = args.get(0).to_string(scope) else {
        return;
    };
    let selector = selector.to_rust_string_lossy(scope);

    let document = scope.document();

    match document.query_selector_all_from(node_id, &selector) {
        Ok(nodes) => {
            let node_list = NodeList::new(nodes).object(scope);
            retval.set(node_list.cast());
        }
        Err(err) => {
            let error = v8::String::new(scope, &format!("{err:?}")).unwrap();
            let exception = v8::Exception::syntax_error(scope, error.into());
            scope.throw_exception(exception);
            retval.set_undefined();
        }
    }
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

        let query_selector_name = fast_str!("querySelector").to_v8(scope);
        let query_selector_function = FunctionTemplate::new(scope, query_selector);
        proto.set(query_selector_name.cast(), query_selector_function.cast());

        let query_selector_all_name = fast_str!("querySelectorAll").to_v8(scope);
        let query_selector_all_function = FunctionTemplate::new(scope, query_selector_all);
        proto.set(
            query_selector_all_name.cast(),
            query_selector_all_function.cast(),
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
