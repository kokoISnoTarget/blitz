use super::*;
use blitz_traits::DomEvent;

pub struct EventObject(DomEvent);
impl EventObject {
    pub fn new(event: DomEvent) -> EventObject {
        EventObject(event)
    }
}
impl GarbageCollected for EventObject {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}

    fn get_name(&self) -> Option<&'static std::ffi::CStr> {
        None
    }
}
impl WrappedObject for EventObject {
    const TAG: u16 = super::EVENT;
    const CLASS_NAME: &'static str = "Event";

    fn init_template<'s>(scope: &mut HandleScope<'s>, proto: Local<ObjectTemplate>) {}
}
