use std::{
    hash::BuildHasher,
    ops::{Deref, DerefMut},
};

use crate::v8intergration::IsolateExt;

use super::*;

pub fn add_function_to_object(
    scope: &mut HandleScope<'_>,
    obj: &Local<Object>,
    name: &str,
    func: impl MapFnTo<FunctionCallback>,
) {
    let func = v8::Function::new(scope, func).unwrap();
    let name = v8::String::new(scope, name).unwrap();
    obj.set(scope, name.into(), func.into());
}

// This is from https://github.com/denoland/rusty_v8/blob/3ffe0d7de976172148939ef3c85176e2b1e44781/src/isolate.rs#L2092
/// A special hasher that is optimized for hashing `std::any::TypeId` values.
/// `TypeId` values are actually 64-bit values which themselves come out of some
/// hash function, so it's unnecessary to shuffle their bits any further.
#[derive(Clone, Default)]
pub(crate) struct TypeIdHasher {
    state: Option<u64>,
}

impl std::hash::Hasher for TypeIdHasher {
    fn write(&mut self, _bytes: &[u8]) {
        panic!("TypeIdHasher::write() called unexpectedly");
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        // The internal hash function of TypeId only takes the bottom 64-bits, even on versions
        // of Rust that use a 128-bit TypeId.
        let prev_state = self.state.replace(value);
        debug_assert_eq!(prev_state, None);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.state.unwrap()
    }
}

// This is from https://github.com/denoland/rusty_v8/blob/3ffe0d7de976172148939ef3c85176e2b1e44781/src/isolate.rs#L2115C1-L2129C1
/// Factory for instances of `TypeIdHasher`. This is the type that one would
/// pass to the constructor of some map/set type in order to make it use
/// `TypeIdHasher` instead of the default hasher implementation.
#[derive(Copy, Clone, Default)]
pub(crate) struct BuildTypeIdHasher;

impl BuildHasher for BuildTypeIdHasher {
    type Hasher = TypeIdHasher;

    #[inline]
    fn build_hasher(&self) -> Self::Hasher {
        Default::default()
    }
}

pub trait WrappedObject: GarbageCollected {
    const TAG: u16;
    fn init_template<'s>(scope: &mut HandleScope<'s>, proto: Local<ObjectTemplate>);
    fn init_function(
        _scope: &mut HandleScope<'_>,
        _args: FunctionCallbackArguments<'_>,
        _ret: ReturnValue,
    ) {
    }

    fn init<'s>(scope: &mut HandleScope<'s>)
    where
        Self: Sized + 'static,
    {
        let template = FunctionTemplate::new(scope, Self::init_function);
        let proto = template.prototype_template(scope);
        proto.set_internal_field_count(1);

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

        assert!(obj.is_api_wrapper(), "Object is not an API wrapper");

        let heap = scope.get_cpp_heap().unwrap();
        let member = unsafe { v8::cppgc::make_garbage_collected(heap, self) };
        unsafe {
            v8::Object::wrap::<{ Self::TAG }, Self>(scope, obj, &member);
        }
        obj
    }
}

pub trait ObjectExt {
    fn get_as<T: WrappedObject>(self, scope: &mut Isolate) -> Option<Ptr<T>>
    where
        [(); { T::TAG } as usize]:;
    fn unwrap_as<T: WrappedObject>(self, scope: &mut Isolate) -> Ptr<T>
    where
        [(); { T::TAG } as usize]:,
        Self: Sized,
    {
        self.get_as(scope).unwrap()
    }
}
impl<'s> ObjectExt for Local<'s, Object> {
    fn get_as<T: WrappedObject>(self, isolate: &mut Isolate) -> Option<Ptr<T>>
    where
        [(); { T::TAG } as usize]:,
    {
        unsafe { v8::Object::unwrap::<{ T::TAG }, T>(isolate, self) }
    }
}
