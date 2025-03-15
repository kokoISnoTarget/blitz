use std::ops::DerefMut;

use v8::{
    FunctionCallback, Handle, MapFnTo, Private,
    cppgc::{GarbageCollected, Ptr},
};

pub fn add_method<'a, F: MapFnTo<FunctionCallback>>(
    handle_scope: &mut v8::HandleScope<'a>,
    obj: &v8::Local<'a, v8::Object>,
    name: &str,
    func: F,
) {
    let name = v8::String::new(handle_scope, name).unwrap();
    let func = v8::Function::new(handle_scope, func).unwrap();
    obj.set(handle_scope, name.into(), func.into()).unwrap();
}
