use crate::util::todo;
use blitz_traits::DomEvent;
use v8::{HandleScope, Local, Object};

pub struct EventObject(DomEvent);

pub fn event_object<'a>(scope: &mut HandleScope<'a>, event: DomEvent) -> Local<'a, Object> {
    todo()
}
