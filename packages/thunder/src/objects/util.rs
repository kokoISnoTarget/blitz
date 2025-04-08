use std::{
    any::TypeId,
    collections::HashMap,
    ffi::c_void,
    hash::BuildHasher,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use blitz_dom::BaseDocument;
use v8::{
    self, External, Function, FunctionCallback, FunctionCallbackArguments, FunctionTemplate,
    Global, Handle, HandleScope, Isolate, Local, MapFnTo, Object, ObjectTemplate, ReturnValue,
    Value,
    cppgc::{GarbageCollected, Ptr},
};

use crate::{HtmlParser, fetch_thread::FetchThread};

const FUNCTION_TEMPLATE_SLOT: u32 = 0;
const DOCUMENT_SLOT: u32 = 1;
const OBJECT_TEMPLATE_SLOT: u32 = 2;
const HTML_PARSER_SLOT: u32 = 3;
const FETCH_THREAD_SLOT: u32 = 4;
const EVENT_LISTENERS_SLOT: u32 = 5;

type FunctionTemplatesMap = HashMap<TypeId, Global<FunctionTemplate>, BuildTypeIdHasher>;
type ObjectTemplatesMap = HashMap<TypeId, Global<ObjectTemplate>, BuildTypeIdHasher>;

type EventListeners = HashMap<u32, HashMap<String, Global<Value>>>;

trait _IsolateExt {
    fn get_inner<T>(&self, slot: u32) -> &T;
    fn get_inner_mut<T>(&mut self, slot: u32) -> &mut T;
    fn get_inner_wrapped<T>(&mut self, slot: u32) -> SlotWrapper<T>;
    fn set_inner<T>(&mut self, slot: u32, data: T);
    fn clear_inner<T>(&mut self, slot: u32) -> T;
}
impl _IsolateExt for Isolate {
    fn get_inner<T>(&self, slot: u32) -> &T {
        let raw_ptr = self.get_data(slot);
        assert!(!raw_ptr.is_null(), "Data on slot {slot} is null");
        unsafe { &*(raw_ptr as *const T) }
    }
    fn get_inner_mut<T>(&mut self, slot: u32) -> &mut T {
        let raw_ptr = self.get_data(slot);
        assert!(!raw_ptr.is_null(), "Data on slot {slot} is null");
        unsafe { &mut *(raw_ptr as *mut T) }
    }
    fn get_inner_wrapped<T>(&mut self, slot: u32) -> SlotWrapper<T> {
        let raw_ptr = self.get_data(slot);
        assert!(!raw_ptr.is_null());
        SlotWrapper::new(raw_ptr as *mut T)
    }
    fn set_inner<T>(&mut self, slot: u32, data: T) {
        let ptr = Box::into_raw(Box::new(data));
        self.set_data(slot, ptr as *mut c_void);
    }
    fn clear_inner<T>(&mut self, slot: u32) -> T {
        let raw_ptr = self.get_data(slot);
        assert!(!raw_ptr.is_null());
        self.set_data(slot, std::ptr::null_mut() as *mut c_void);
        *unsafe { Box::from_raw(raw_ptr as *mut T) }
    }
}
pub trait IsolateExt {
    fn document(&self) -> &BaseDocument;
    fn document_mut(&mut self) -> &mut BaseDocument;
    fn document_mut_from_ref(&self) -> &mut BaseDocument;
    fn set_document(&mut self, document: BaseDocument);
    fn clear_document(&mut self) -> BaseDocument;

    fn ptr(&self) -> IsolatePtr;

    fn parser(&mut self) -> &mut HtmlParser;
    fn set_parser(&mut self, parser: HtmlParser);

    fn fetch_thread(&self) -> &FetchThread;
    fn set_fetch_thread(&mut self, fetch_thread: FetchThread);

    fn setup_templates(&mut self);
    fn clear_templates(&mut self);

    fn event_listeners(&self) -> &EventListeners;
    fn event_listeners_mut(&mut self) -> &mut EventListeners;
    fn setup_listeners(&mut self);
    fn clear_listeners(&mut self);
}
impl IsolateExt for Isolate {
    fn document(&self) -> &BaseDocument {
        self.get_inner(DOCUMENT_SLOT)
    }
    fn document_mut(&mut self) -> &mut BaseDocument {
        self.get_inner_mut(DOCUMENT_SLOT)
    }
    fn document_mut_from_ref(&self) -> &mut BaseDocument {
        let raw_ptr = self.get_data(DOCUMENT_SLOT);
        assert!(!raw_ptr.is_null());
        unsafe { &mut *(raw_ptr as *mut _) }
    }
    fn set_document(&mut self, document: BaseDocument) {
        self.set_inner(DOCUMENT_SLOT, document);
    }
    fn clear_document(&mut self) -> BaseDocument {
        self.clear_inner(DOCUMENT_SLOT)
    }

    fn setup_templates(&mut self) {
        let templates = FunctionTemplatesMap::with_hasher(Default::default());
        self.set_inner(FUNCTION_TEMPLATE_SLOT, templates);
        let templates = ObjectTemplatesMap::with_hasher(Default::default());
        self.set_inner(OBJECT_TEMPLATE_SLOT, templates);
    }
    fn clear_templates(&mut self) {
        let mut templates = self.clear_inner::<FunctionTemplatesMap>(FUNCTION_TEMPLATE_SLOT);
        templates.clear();
        let mut templates = self.clear_inner::<ObjectTemplatesMap>(OBJECT_TEMPLATE_SLOT);
        templates.clear();
    }

    fn ptr(&self) -> IsolatePtr {
        IsolatePtr::new(self as *const Isolate as *mut Isolate)
    }

    fn parser(&mut self) -> &mut HtmlParser {
        self.get_inner_mut(HTML_PARSER_SLOT)
    }
    fn set_parser(&mut self, parser: HtmlParser) {
        self.set_inner(HTML_PARSER_SLOT, parser);
    }
    fn fetch_thread(&self) -> &FetchThread {
        self.get_inner(FETCH_THREAD_SLOT)
    }
    fn set_fetch_thread(&mut self, thread: FetchThread) {
        self.set_inner(FETCH_THREAD_SLOT, thread);
    }

    fn event_listeners(&self) -> &EventListeners {
        self.get_inner(EVENT_LISTENERS_SLOT)
    }
    fn event_listeners_mut(&mut self) -> &mut EventListeners {
        self.get_inner_mut(EVENT_LISTENERS_SLOT)
    }
    fn setup_listeners(&mut self) {
        self.set_inner(EVENT_LISTENERS_SLOT, EventListeners::default());
    }
    fn clear_listeners(&mut self) {
        self.clear_inner::<EventListeners>(EVENT_LISTENERS_SLOT);
    }
}

pub trait HandleScopeExt<'a> {
    fn get_fn_template<T: 'static>(&mut self) -> Global<FunctionTemplate>;
    fn set_fn_template<T: 'static>(
        &mut self,
        template: impl Handle<Data = FunctionTemplate>,
    ) -> Option<Global<FunctionTemplate>>;
    fn get_obj_template<T: 'static>(&mut self) -> Global<ObjectTemplate>;
    fn set_obj_template<T: 'static>(
        &mut self,
        template: impl Handle<Data = ObjectTemplate>,
    ) -> Option<Global<ObjectTemplate>>;
    fn init_templates(&mut self);

    fn create_wrapped_object<T: GarbageCollected + Tag + 'static>(
        &mut self,
        object: T,
    ) -> Local<'a, Object>
    where
        [(); { T::TAG } as usize]:;
    fn unwrap_element_object<T: GarbageCollected + Tag + 'static>(
        &mut self,
        obj: Local<Object>,
    ) -> Option<Ptr<T>>
    where
        [(); { T::TAG } as usize]:;
}

impl<'a> HandleScopeExt<'a> for HandleScope<'a> {
    fn get_fn_template<T: 'static>(&mut self) -> Global<FunctionTemplate> {
        let mut templates = self.get_inner_wrapped::<FunctionTemplatesMap>(FUNCTION_TEMPLATE_SLOT);
        let type_id = TypeId::of::<T>();

        let template = templates.entry(type_id).or_insert_with(|| {
            let templ = FunctionTemplate::new(self, _constructor);
            Global::new(self, templ)
        });

        template.clone()
    }
    fn set_fn_template<T: 'static>(
        &mut self,
        template: impl Handle<Data = FunctionTemplate>,
    ) -> Option<Global<FunctionTemplate>> {
        let global = Global::new(self, template);
        let type_id = TypeId::of::<T>();

        let templates = self.get_inner_mut::<FunctionTemplatesMap>(FUNCTION_TEMPLATE_SLOT);
        templates.insert(type_id, global)
    }
    fn get_obj_template<T: 'static>(&mut self) -> Global<ObjectTemplate> {
        let mut templates = self.get_inner_wrapped::<ObjectTemplatesMap>(OBJECT_TEMPLATE_SLOT);
        let type_id = TypeId::of::<T>();

        let template = templates.entry(type_id).or_insert_with(|| {
            let templ = ObjectTemplate::new(self);
            Global::new(self, templ)
        });

        template.clone()
    }
    fn set_obj_template<T: 'static>(
        &mut self,
        template: impl Handle<Data = ObjectTemplate>,
    ) -> Option<Global<ObjectTemplate>> {
        let global = Global::new(self, template);
        let type_id = TypeId::of::<T>();

        let templates = self.get_inner_mut::<ObjectTemplatesMap>(OBJECT_TEMPLATE_SLOT);
        templates.insert(type_id, global)
    }
    fn init_templates(&mut self) {
        super::element::set_element_template(self);
        super::event::set_event_template(self);
    }
    fn create_wrapped_object<T: GarbageCollected + Tag + 'static>(
        &mut self,
        object: T,
    ) -> Local<'a, Object>
    where
        [(); { T::TAG } as usize]:,
    {
        let template = self.get_fn_template::<T>();
        let template = Local::new(self, template);
        let func = template.get_function(self).unwrap();
        let obj = func.new_instance(self, &[]).unwrap();

        assert!(obj.is_api_wrapper(), "Object is not an API wrapper");

        let heap = self.get_cpp_heap().unwrap();
        let member = unsafe { v8::cppgc::make_garbage_collected(heap, object) };
        unsafe {
            v8::Object::wrap::<{ T::TAG }, T>(self, obj, &member);
        }
        obj
    }
    fn unwrap_element_object<T: GarbageCollected + Tag>(
        &mut self,
        obj: Local<Object>,
    ) -> Option<Ptr<T>>
    where
        [(); { T::TAG } as usize]:,
    {
        unsafe { v8::Object::unwrap::<{ T::TAG }, T>(self, obj) }
    }
}

fn _constructor(_scope: &mut HandleScope<'_>, _args: FunctionCallbackArguments, _ret: ReturnValue) {
    // Implementation of the constructor function
}

pub fn element_template<'a>(scope: &mut HandleScope<'a, ()>) -> Local<'a, FunctionTemplate> {
    FunctionTemplate::new(scope, _constructor)
}

pub trait Tag {
    const TAG: u16;
}

pub fn wrap_element_object<'a, T: GarbageCollected + Tag + 'static>(
    scope: &mut v8::HandleScope<'a>,
    t: T,
) -> v8::Local<'a, v8::Object>
where
    [(); { T::TAG } as usize]:,
{
    // This is from https://github.com/denoland/deno_core/blob/b37d41fc036653d4dccb0cf6992abed94168f5d8/core/cppgc.rs#L47
    // let obj = match templates.get::<T>() {
    // Some(templ) => {
    // let templ = v8::Local::new(scope, templ);
    // let inst = templ.instance_template(scope);
    // inst.new_instance(scope).unwrap()
    // }
    // _ => {
    // let templ = v8::Local::new(scope, state.cppgc_template.borrow().as_ref().unwrap());
    // let func = templ.get_function(scope).unwrap();
    // func.new_instance(scope, &[]).unwrap()
    // }
    // };

    let template = element_template(scope);
    let func = template.get_function(scope).unwrap();
    let obj = func.new_instance(scope, &[]).unwrap();

    let heap = scope.get_cpp_heap().unwrap();

    let member = unsafe { v8::cppgc::make_garbage_collected(heap, t) };

    unsafe {
        v8::Object::wrap::<{ T::TAG }, T>(scope, obj, &member);
    }
    obj
}

pub fn unwrap_element_object<T: GarbageCollected + Tag>(
    isolate: &mut Isolate,
    obj: Local<Object>,
) -> Option<Ptr<T>>
where
    [(); { T::TAG } as usize]:,
{
    unsafe { v8::Object::unwrap::<{ T::TAG }, T>(isolate, obj) }
}

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

struct SlotWrapper<T> {
    data: NonNull<T>,
}
impl<T> SlotWrapper<T> {
    pub fn new(data: *mut T) -> Self {
        Self {
            data: NonNull::new(data).unwrap(),
        }
    }
}
impl<T> Deref for SlotWrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}
impl<T> DerefMut for SlotWrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.data.as_mut() }
    }
}

pub(crate) struct IsolatePtr {
    isolate: *mut Isolate,
}
impl IsolatePtr {
    pub fn new(isolate: *mut Isolate) -> Self {
        Self { isolate }
    }
}
impl Deref for IsolatePtr {
    type Target = Isolate;

    fn deref(&self) -> &Self::Target {
        unsafe { self.isolate.as_ref().unwrap() }
    }
}
impl DerefMut for IsolatePtr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.isolate.as_mut().unwrap() }
    }
}

pub fn empty(
    _scope: &mut HandleScope<'_>,
    _args: FunctionCallbackArguments<'_>,
    _ret: ReturnValue,
) {
}
