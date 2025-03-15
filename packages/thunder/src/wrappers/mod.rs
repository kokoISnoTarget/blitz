mod document;
mod element;
mod util;

use std::{
    any::TypeId,
    cell::{LazyCell, RefCell},
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

pub use blitz_dom::BaseDocument;
pub use document::Document;
pub use element::Element;
pub use util::add_method;
pub use v8::{
    FunctionCallbackArguments, HandleScope, Local, Object, ReturnValue, cppgc::GarbageCollected,
};
use v8::{Global, Isolate, ObjectTemplate};

pub struct Templates {
    templates: HashMap<TypeId, Global<ObjectTemplate>>,
}
impl Templates {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }
}

trait HandleScopeTemplateExt {
    fn get_template<T: 'static, F: FnOnce(&mut HandleScope) -> Local<ObjectTemplate>>(
        &mut self,
        f: F,
    ) -> Local<ObjectTemplate>;
}

impl<'s> HandleScopeTemplateExt for HandleScope<'s> {
    fn get_template<
        T: 'static,
        F: for<'a> FnOnce(&mut HandleScope<'a>) -> Local<'a, ObjectTemplate>,
    >(
        &mut self,
        f: F,
    ) -> Local<ObjectTemplate> {
        let id = TypeId::of::<T>();

        let template = self
            .get_slot_mut::<Templates>()
            .expect("Templates not initialized")
            .templates
            .entry(id)
            .or_insert_with(|| {
                let template_local = f(self);
                Global::new(self, template_local)
            })
            .to_owned();

        Local::new(self, template)
    }
}
