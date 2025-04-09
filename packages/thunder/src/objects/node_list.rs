use v8::{
    Context, Exception, Function, FunctionCallbackArguments, FunctionTemplate, Global, HandleScope,
    IndexedPropertyHandlerConfiguration, Intercepted, Number, Object, PropertyAttribute,
    PropertyCallbackArguments, ReturnValue, Symbol, Uint32, cppgc::GarbageCollected, null,
};

use crate::{fast_str, util::OneByteConstExt};

use super::{HandleScopeExt, NODE_LIST, Tag, element::element_object, empty};

struct NodeList {
    array: Vec<u32>,
}
impl GarbageCollected for NodeList {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}

    fn get_name(&self) -> Option<&'static std::ffi::CStr> {
        None
    }
}
impl Tag for NodeList {
    const TAG: u16 = NODE_LIST;
}
impl NodeList {
    fn length(&self) -> u32 {
        self.array.len() as u32
    }
}

pub fn set_node_list_template(scope: &mut HandleScope) {
    let template = FunctionTemplate::new(scope, empty);
    let proto = template.prototype_template(scope);

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

    scope.set_fn_template::<NodeList>(template);
}

fn length_getter(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let obj = scope
        .unwrap_element_object::<NodeList>(args.this())
        .unwrap();
    retval.set_uint32(obj.length());
}

fn item(
    scope: &mut HandleScope<'_>,
    args: FunctionCallbackArguments<'_>,
    mut retval: ReturnValue<'_>,
) {
    let obj = scope
        .unwrap_element_object::<NodeList>(args.this())
        .unwrap();
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
        let obj = element_object(scope, index as u32);
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
    let context = scope.remove_slot::<Global<Context>>().unwrap();
    let global = context.open(scope).global(scope);
    let init_fn = fast_str!("__internal_nodeListIterator").to_v8(scope);
    let null = null(scope);
    let iter = global
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
    let node_list = scope
        .unwrap_element_object::<NodeList>(args.this())
        .unwrap();
    let element_id = node_list.array.get(index as usize);

    if let Some(&element_id) = element_id {
        ret.set(element_object(scope, element_id).cast());
    } else {
        ret.set_null();
    };

    Intercepted::No // TODO: wot?
}
