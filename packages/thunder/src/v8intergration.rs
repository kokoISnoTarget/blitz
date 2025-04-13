use std::{any::TypeId, collections::HashMap, ffi::c_void, sync::Arc};

use blitz_dom::BaseDocument;
use v8::{
    Context, ContextScope, FunctionTemplate, Global, Handle, HandleScope, Isolate, Local, Object,
    ObjectTemplate, Value,
};

use crate::{
    HtmlParser,
    fetch_thread::init_fetch_thread,
    importmap::ImportMap,
    module::ModuleMap,
    objects::{BuildTypeIdHasher, Element, EventObject, FetchThread, NodeList, WrappedObject},
    util::IsolatePtr,
};

type FunctionTemplatesMap = HashMap<TypeId, Global<FunctionTemplate>, BuildTypeIdHasher>;
type ObjectTemplatesMap = HashMap<TypeId, Global<ObjectTemplate>, BuildTypeIdHasher>;

type EventListeners = HashMap<u32, HashMap<String, Global<Value>>>;

pub struct GlobalState {
    pub(crate) document: BaseDocument,
    pub(crate) parser: HtmlParser,
    pub(crate) fetch_thread: FetchThread,
    pub(crate) importmap: ImportMap,
    pub(crate) modulemap: ModuleMap,
    pub(crate) event_listeners: EventListeners,
    pub(crate) function_templates: FunctionTemplatesMap,
    pub(crate) object_templates: ObjectTemplatesMap,
    pub(crate) context: Global<Context>,
}
impl GlobalState {
    const GLOBAL_STATE_SLOT: u32 = 0;
    pub fn new(isolate: &mut Isolate, mut document: BaseDocument) -> GlobalState {
        let mut scope = HandleScope::new(isolate);
        let context = Context::new(&mut scope, v8::ContextOptions::default());
        let context = Global::new(&mut scope, context);
        let function_templates = HashMap::with_hasher(BuildTypeIdHasher::default());
        let object_templates = HashMap::with_hasher(BuildTypeIdHasher::default());
        let event_listeners = HashMap::new();
        let importmap = ImportMap::new();
        let modulemap = ModuleMap::new();

        let (fetch_thread, provider_impl) = init_fetch_thread();
        document.set_net_provider(Arc::new(provider_impl));

        drop(scope);

        GlobalState {
            parser: HtmlParser::new(isolate, document.id()),
            document,
            fetch_thread,
            importmap,
            modulemap,
            event_listeners,
            function_templates,
            object_templates,
            context,
        }
    }

    pub fn document(&self) -> &BaseDocument {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut BaseDocument {
        &mut self.document
    }

    // Parser accessor method
    pub fn parser(&mut self) -> &mut HtmlParser {
        &mut self.parser
    }

    // Fetch thread accessor method
    pub fn fetch_thread(&self) -> &FetchThread {
        &self.fetch_thread
    }

    // Event listeners accessor methods
    pub fn event_listeners(&self) -> &EventListeners {
        &self.event_listeners
    }

    pub fn event_listeners_mut(&mut self) -> &mut EventListeners {
        &mut self.event_listeners
    }

    // Import map accessor method
    pub fn importmap(&mut self) -> &mut ImportMap {
        &mut self.importmap
    }

    pub fn get_fn_template<T: 'static>(&self) -> Option<Global<FunctionTemplate>> {
        let type_id = TypeId::of::<T>();
        self.function_templates.get(&type_id).cloned()
    }

    pub fn set_fn_template<T: 'static>(
        &mut self,
        scope: &mut Isolate,
        template: impl Handle<Data = FunctionTemplate>,
    ) -> Option<Global<FunctionTemplate>> {
        let global = Global::new(scope, template);
        let type_id = TypeId::of::<T>();
        self.function_templates.insert(type_id, global)
    }

    pub fn context(&self) -> &Global<Context> {
        &self.context
    }
}

pub trait IsolateExt<'s> {
    fn global_state(&self) -> &GlobalState;
    fn global_state_mut(&mut self) -> &mut GlobalState;
    fn global_state_mut_from_ref(&self) -> &mut GlobalState;
    fn set_global_state(&mut self, state: GlobalState);
    fn remove_global_state(&mut self) -> Option<GlobalState>;

    fn context_scope(&'s mut self) -> HandleScope<'s>;
    unsafe fn isolate_ptr(&self) -> IsolatePtr;

    fn document(&self) -> &BaseDocument;
    fn document_mut(&mut self) -> &mut BaseDocument;
    fn document_mut_from_ref(&self) -> &mut BaseDocument;
    fn parser(&mut self) -> &mut HtmlParser;
    fn event_listeners(&self) -> &EventListeners;
    fn event_listeners_mut(&mut self) -> &mut EventListeners;
    fn fetch_thread(&self) -> &FetchThread;
    fn importmap(&mut self) -> &mut ImportMap;
    fn get_fn_template<T: 'static>(&self) -> Option<Global<FunctionTemplate>>;
    fn set_fn_template<T: 'static>(
        &mut self,
        template: impl Handle<Data = FunctionTemplate>,
    ) -> Option<Global<FunctionTemplate>>;
}

impl<'s> IsolateExt<'s> for Isolate {
    fn global_state(&self) -> &GlobalState {
        let raw_ptr = self.get_data(GlobalState::GLOBAL_STATE_SLOT);
        assert!(!raw_ptr.is_null(), "Global state is not initialized");
        unsafe { &*(raw_ptr as *const GlobalState) }
    }

    fn global_state_mut(&mut self) -> &mut GlobalState {
        let raw_ptr = self.get_data(GlobalState::GLOBAL_STATE_SLOT);
        assert!(!raw_ptr.is_null(), "Global state is not initialized");
        unsafe { &mut *(raw_ptr as *mut GlobalState) }
    }

    fn global_state_mut_from_ref(&self) -> &mut GlobalState {
        let raw_ptr = self.get_data(GlobalState::GLOBAL_STATE_SLOT);
        assert!(!raw_ptr.is_null(), "Global state is not initialized");
        unsafe { &mut *(raw_ptr as *mut GlobalState) }
    }

    fn set_global_state(&mut self, state: GlobalState) {
        let ptr = Box::into_raw(Box::new(state));
        self.set_data(GlobalState::GLOBAL_STATE_SLOT, ptr as *mut c_void);
    }

    fn remove_global_state(&mut self) -> Option<GlobalState> {
        let raw_ptr = self.get_data(GlobalState::GLOBAL_STATE_SLOT);
        if raw_ptr.is_null() {
            return None;
        }
        self.set_data(
            GlobalState::GLOBAL_STATE_SLOT,
            std::ptr::null_mut() as *mut c_void,
        );
        Some(*unsafe { Box::from_raw(raw_ptr as *mut GlobalState) })
    }

    fn context_scope(&'s mut self) -> HandleScope<'s> {
        let context = {
            let raw_ptr = self.get_data(GlobalState::GLOBAL_STATE_SLOT);
            assert!(!raw_ptr.is_null(), "Global state is not initialized");
            unsafe { &*(raw_ptr as *const GlobalState) }
        }
        .context();
        HandleScope::with_context(self, context)
    }

    unsafe fn isolate_ptr(&self) -> IsolatePtr {
        IsolatePtr::new(self as *const Isolate as *mut Isolate)
    }

    fn document(&self) -> &BaseDocument {
        self.global_state().document()
    }

    fn document_mut(&mut self) -> &mut BaseDocument {
        self.global_state_mut().document_mut()
    }

    ///TODO: make unsafe
    fn document_mut_from_ref(&self) -> &mut BaseDocument {
        self.global_state_mut_from_ref().document_mut()
    }

    fn parser(&mut self) -> &mut HtmlParser {
        self.global_state_mut().parser()
    }

    fn event_listeners(&self) -> &EventListeners {
        self.global_state().event_listeners()
    }

    fn event_listeners_mut(&mut self) -> &mut EventListeners {
        self.global_state_mut().event_listeners_mut()
    }

    fn fetch_thread(&self) -> &FetchThread {
        self.global_state().fetch_thread()
    }

    fn importmap(&mut self) -> &mut ImportMap {
        self.global_state_mut().importmap()
    }

    fn get_fn_template<T: 'static>(&self) -> Option<Global<FunctionTemplate>> {
        self.global_state().get_fn_template::<T>()
    }
    fn set_fn_template<T: 'static>(
        &mut self,
        template: impl Handle<Data = FunctionTemplate>,
    ) -> Option<Global<FunctionTemplate>> {
        {
            let raw_ptr = self.get_data(GlobalState::GLOBAL_STATE_SLOT);
            assert!(!raw_ptr.is_null(), "Global state is not initialized");
            unsafe { &mut *(raw_ptr as *mut GlobalState) }
        }
        .set_fn_template::<T>(self, template)
    }
}
pub trait HandleScopeExt<'s> {
    fn global_this(&mut self) -> Local<'s, Object>;
}
impl<'s> HandleScopeExt<'s> for HandleScope<'s> {
    fn global_this(&mut self) -> Local<'s, Object> {
        {
            let raw_ptr = self.get_data(GlobalState::GLOBAL_STATE_SLOT);
            assert!(!raw_ptr.is_null(), "Global state is not initialized");
            unsafe { &mut *(raw_ptr as *mut GlobalState) }
        }
        .context()
        .open(self)
        .global(self)
    }
}
