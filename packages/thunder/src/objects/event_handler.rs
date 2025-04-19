use v8::{
    TracedReference,
    cppgc::{Member, Visitor},
};

use super::*;

// https://html.spec.whatwg.org/multipage/webappapis.html#event-handler-attributes
pub struct EventHandler {
    value: TracedReference<Value>,
    listener: TracedReference<Value>,
}

impl GarbageCollected for EventHandler {
    fn trace(&self, visitor: &Visitor) {
        visitor.trace(&self.value);
        visitor.trace(&self.listener);
    }
}
impl EventHandler {
    fn getter(&self, )
}
