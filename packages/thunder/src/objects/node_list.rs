use smallvec::SmallVec;

use crate::v8intergration::HandleScopeExt;

use super::*;

pub struct NodeList {
    array: SmallVec<[usize; 32]>,
}
impl GarbageCollected for NodeList {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}

    fn get_name(&self) -> Option<&'static std::ffi::CStr> {
        None
    }
}
impl WrappedObject for NodeList {
    const TAG: u16 = NODE_LIST;

    fn init_template<'s>(scope: &mut HandleScope<'s>, proto: Local<ObjectTemplate>) {
        let indexed_config = IndexedPropertyHandlerConfiguration::new().getter(index);
        proto.set_indexed_property_handler(indexed_config);

        let lenth_name = fast_str!("length").to_v8(scope);
        let lenth_getter = FunctionTemplate::new(scope, length_getter);
        proto.set_accessor_property(
            lenth_name.cast(),
            Some(lenth_getter.cast()),
            None,
            PropertyAttribute::READ_ONLY,
        );

        let item_name = fast_str!("item").to_v8(scope);
        let item_function = FunctionTemplate::new(scope, item);
        proto.set(item_name.cast(), item_function.cast());

        let entries_name = fast_str!("entries").to_v8(scope);
        let entries_function = FunctionTemplate::new(scope, entries);
        proto.set(entries_name.cast(), entries_function.cast());
    }
}
impl NodeList {
    pub fn new(array: SmallVec<[usize; 32]>) -> NodeList {
        NodeList { array }
    }
    fn length(&self) -> u32 {
        self.array.len() as u32
    }
}

fn length_getter(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let obj = args.this().unwrap_as::<NodeList>(scope);
    retval.set_uint32(obj.length());
}

fn item(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let obj = args.this().unwrap_as::<NodeList>(scope);
    let Ok(index) = args.get(0).try_cast::<Number>() else {
        let msg = fast_str!("No index provided").to_v8(scope);
        let exception = Exception::type_error(scope, msg);
        scope.throw_exception(exception);
        return;
    };
    let Some(index) = index.int32_value(scope) else {
        retval.set_null();
        return;
    };

    if index < obj.length() as i32 && index >= 0 {
        let obj = Element::new(index as u32).object(scope);
        retval.set(obj.cast());
    } else {
        retval.set_null();
    }
}

fn entries(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let init_fn = fast_str!("__internal_nodeListIterator").to_v8(scope);
    let null = null(scope);
    let iter = scope
        .global_this()
        .get(scope, init_fn.cast())
        .unwrap()
        .cast::<Function>()
        .call(scope, null.cast(), &[args.this().cast()])
        .unwrap();
    retval.set(iter);
}

fn index<'s>(
    scope: &mut HandleScope<'s>,
    index: u32,
    args: PropertyCallbackArguments<'s>,
    mut ret: ReturnValue<'_>,
) -> Intercepted {
    let node_list = args.this().unwrap_as::<NodeList>(scope);
    let element_id = node_list.array.get(index as usize);

    if let Some(&element_id) = element_id {
        ret.set(Element::new(element_id as u32).object(scope).cast());
    } else {
        ret.set_null();
    };
    Intercepted::Yes
}
