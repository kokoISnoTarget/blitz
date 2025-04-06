use super::*;
use crate::util::todo;
use blitz_traits::DomEvent;
use v8::{HandleScope, Local, Object, cppgc::GarbageCollected};

pub struct EventObject(DomEvent);
impl GarbageCollected for EventObject {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}

    fn get_name(&self) -> Option<&'static std::ffi::CStr> {
        None
    }
}
pub fn event_object<'a>(scope: &mut HandleScope<'a>, event: DomEvent) -> Local<'a, Object> {
    let templ = v8::FunctionTemplate::new(scope, empty);
    let func = templ.get_function(scope).unwrap();
    let obj = func.new_instance(scope, &[]).unwrap();

    assert!(obj.is_api_wrapper());

    let member = unsafe {
        v8::cppgc::make_garbage_collected(scope.get_cpp_heap().unwrap(), EventObject(event))
    };

    unsafe {
        v8::Object::wrap::<EVENT, EventObject>(scope, obj, &member);
    }

    obj
}
