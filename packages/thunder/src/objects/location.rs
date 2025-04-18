use crate::v8intergration::IsolateExt as _;

use super::*;

fn host_getter(scope: &mut HandleScope, args: FunctionCallbackArguments, mut ret: ReturnValue) {
    let host = scope
        .document()
        .base_url()
        .and_then(|url| url.host_str())
        .map(|host_str| host_str.to_string());

    let str = if let Some(host) = host {
        v8::String::new(scope, &host).unwrap()
    } else {
        v8::String::empty(scope)
    };

    ret.set(str.cast());
}

fn href_getter(scope: &mut HandleScope, args: FunctionCallbackArguments, mut ret: ReturnValue) {
    let url = scope.document().base_url().map(|url| url.to_string());

    let str = if let Some(url) = url {
        v8::String::new(scope, &url).unwrap()
    } else {
        v8::String::empty(scope)
    };

    ret.set(str.cast());
}

pub struct Location;
impl GarbageCollected for Location {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}
}

impl WrappedObject for Location {
    const TAG: u16 = 0;
    const CLASS_NAME: &'static str = "Location";
    fn init_template<'s>(scope: &mut HandleScope<'s>, proto: Local<ObjectTemplate>) {
        let host_name = fast_str!("host").to_v8(scope);
        let host_getter = FunctionTemplate::new(scope, host_getter);
        proto.set_accessor_property(
            host_name.cast(),
            Some(host_getter),
            None,
            PropertyAttribute::NONE,
        );

        let href_name = fast_str!("href").to_v8(scope);
        let href_getter = FunctionTemplate::new(scope, href_getter);
        proto.set_accessor_property(
            href_name.cast(),
            Some(href_getter),
            None,
            PropertyAttribute::NONE,
        );
    }
    fn init<'s>(scope: &mut HandleScope<'s>)
    where
        Self: Sized + 'static,
    {
        let template = FunctionTemplate::new(scope, Self::init_function);
        let proto = template.prototype_template(scope);

        Self::init_template(scope, proto);

        scope.set_fn_template::<Self>(template);
    }

    fn object<'s>(self, scope: &mut HandleScope<'s>) -> Local<'s, Object>
    where
        Self: Sized + 'static,
        [(); { Self::TAG } as usize]:,
    {
        let template = scope
            .get_fn_template::<Self>()
            .expect("Objects should get initialized before first creation");
        let template = Local::new(scope, template);
        let func = template.get_function(scope).unwrap();
        let obj = func.new_instance(scope, &[]).unwrap();
        obj
    }
}

pub fn add_location(scope: &mut HandleScope<'_>, global: Local<Object>) {
    let location_name = v8::String::new(scope, "location").unwrap();
    let location_value = Location.object(scope);

    global
        .set(scope, location_name.into(), location_value.into())
        .unwrap();
}
