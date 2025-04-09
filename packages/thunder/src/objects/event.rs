use super::*;
use blitz_traits::DomEvent;

pub struct EventObject(DomEvent);
impl GarbageCollected for EventObject {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}

    fn get_name(&self) -> Option<&'static std::ffi::CStr> {
        None
    }
}
impl Tag for EventObject {
    const TAG: u16 = super::EVENT;
}
pub fn set_event_template<'a>(scope: &mut HandleScope<'a>) {
    let template = FunctionTemplate::new(scope, empty);
    let proto = template.prototype_template(scope);
    proto.set_internal_field_count(1);

    scope.set_fn_template::<EventObject>(template);
}
pub fn event_object<'a>(scope: &mut HandleScope<'a>, event: DomEvent) -> Local<'a, Object> {
    scope.create_wrapped_object(EventObject(event))
}
