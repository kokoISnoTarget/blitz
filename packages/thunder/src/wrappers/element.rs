use std::{cell::RefCell, rc::Rc};

use blitz_dom::BaseDocument;
use v8::{HandleScope, Local, Object, cppgc::GarbageCollected};

pub struct Element {
    pub id: usize,
}
impl Element {
    pub fn new(id: usize) -> Self {
        Element { id }
    }
    pub fn object<'a>(self, scope: &'a mut HandleScope) -> Local<'a, Object> {
        todo!()
    }
}
impl GarbageCollected for Element {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}
}
