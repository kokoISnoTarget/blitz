use std::{
    any::TypeId,
    collections::HashMap,
    ffi::c_void,
    hash::BuildHasher,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use super::*;

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

    fn setup_import_map(&mut self);
    fn import_map(&mut self) -> &mut ImportMap;
    fn clear_import_map(&mut self);
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
    fn setup_import_map(&mut self) {
        todo!()
    }
    fn import_map(&mut self) -> &mut ImportMap {
        todo!()
    }
    fn clear_import_map(&mut self) {
        todo!()
    }
}

pub trait HandleScopeExt<'a> {
    fn get_fn_template<T: 'static>(&mut self) -> Option<Global<FunctionTemplate>>;
    fn set_fn_template<T: 'static>(
        &mut self,
        template: impl Handle<Data = FunctionTemplate>,
    ) -> Option<Global<FunctionTemplate>>;
    fn init_templates(&mut self);

    fn unwrap_object<T: WrappedObject + 'static>(&mut self, obj: Local<Object>) -> Option<Ptr<T>>
    where
        [(); { T::TAG } as usize]:;
}

impl<'a> HandleScopeExt<'a> for HandleScope<'a> {
    fn get_fn_template<T: 'static>(&mut self) -> Option<Global<FunctionTemplate>> {
        let templates = self.get_inner::<FunctionTemplatesMap>(FUNCTION_TEMPLATE_SLOT);
        let type_id = TypeId::of::<T>();
        templates.get(&type_id).cloned()
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
    fn init_templates(&mut self) {
        //super::element::set_element_template(self);
        //super::event::set_event_template(self);
        //super::node_list::set_node_list_template(self);
        Element::init(self);
        EventObject::init(self);
        NodeList::init(self);
    }
    fn unwrap_object<T: WrappedObject>(&mut self, obj: Local<Object>) -> Option<Ptr<T>>
    where
        [(); { T::TAG } as usize]:,
    {
        unsafe { v8::Object::unwrap::<{ T::TAG }, T>(self, obj) }
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

pub struct ImportMap {
    map: HashMap<String, String>,
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
