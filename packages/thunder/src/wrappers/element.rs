use v8::{Handle, Number, Uint32, Value};

use super::*;

#[derive(Debug)]
pub struct Element {
    pub id: usize,
}
impl Element {
    pub fn new(id: usize) -> Self {
        Element { id }
    }
    pub fn object<'a>(self, scope: &'a mut HandleScope) -> Local<'a, Object> {
        let template = scope.get_template::<Element, _>(|scope: &mut HandleScope| {
            let obj = ObjectTemplate::new(scope);
            obj.set_internal_field_count(1);

            let fn_name = v8::String::new(scope, "remove").unwrap();
            let fn_value = v8::FunctionTemplate::new(scope, Self::remove);
            obj.set(fn_name.into(), fn_value.into());
            obj
        });

        let obj = template.new_instance(scope).unwrap();

        let id = v8::Integer::new_from_unsigned(scope, self.id as u32);
        if !obj.set_internal_field(0, id.into()) {
            #[cfg(feature = "tracing")]
            tracing::info!("Element::object {} failed to set internal field", self.id);
        };

        obj
    }

    fn remove(scope: &mut HandleScope, args: FunctionCallbackArguments, mut rv: ReturnValue) {
        #[cfg(feature = "tracing")]
        tracing::info!("Element::remove");

        let this = args.this();
        let Some(index) = this.get_internal_field(scope, 0) else {
            #[cfg(feature = "tracing")]
            tracing::error!("Element::remove: no index");
            return;
        };
        let val: Local<v8::Integer> = index.try_into().unwrap();
        let index = val.value() as usize;

        #[cfg(feature = "tracing")]
        tracing::info!("Element::remove {}", index);

        //let this = unsafe { v8::Object::unwrap::<{ ELEMENT }, Element>(scope, this) }.unwrap();
        //let node_id = this.id;

        let document = scope.get_slot_mut::<BaseDocument>().unwrap();

        let parent_id = document.nodes[index].parent.take();
        let Some(parent_id) = parent_id else {
            return;
        };
        document.nodes[parent_id]
            .children
            .retain(|child_id| child_id != &index);
    }
}
impl GarbageCollected for Element {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}
}
