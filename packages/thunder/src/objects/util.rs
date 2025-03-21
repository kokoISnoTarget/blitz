use std::{
    ffi::c_void,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use v8::{External, FunctionCallback, HandleScope, Local, MapFnTo, Object};

pub fn add_rust_element_to_object<T>(scope: &mut HandleScope<'_>, obj: &Local<Object>, element: T) {
    let boxed_element = Box::new(element);
    let ptr = Box::into_raw(boxed_element);

    let external = External::new(scope, ptr as *mut c_void);
    obj.set_internal_field(0, external.into());
}

pub fn get_rust_element_from_object<'a, T>(
    scope: &'a mut HandleScope<'_>,
    obj: &'a Local<Object>,
) -> Option<RustElement<'a, T>> {
    let external = obj.get_internal_field(scope, 0)?;
    let ptr = external.try_cast::<External>().ok()?.value() as *mut T;

    Some(RustElement::new(ptr))
}
pub fn remove_rust_element_from_object<T>(
    scope: &mut HandleScope<'_>,
    obj: &Local<Object>,
) -> Option<T> {
    let external = obj.get_internal_field(scope, 0)?;
    let ptr = external.try_cast::<External>().ok()?.value() as *mut T;
    let element = unsafe { Box::from_raw(ptr) };
    Some(*element)
}

pub struct RustElement<'a, T> {
    element: *mut T,
    _marker: std::marker::PhantomData<&'a T>,
}
impl<'a, T> RustElement<'a, T> {
    pub fn new(element: *mut T) -> Self {
        Self {
            element,
            _marker: std::marker::PhantomData,
        }
    }
}
impl<'a, T> Deref for RustElement<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.element }
    }
}
impl<'a, T> DerefMut for RustElement<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.element }
    }
}

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
