use v8::{Global, PromiseResolver};

pub(crate) fn host_import_module_dynamically_callback<'s>(
    scope: &mut v8::HandleScope<'s>,
    host_defined_options: v8::Local<'s, v8::Data>,
    resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    //let modulemap = scope.modulemap();
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(u32);

pub struct ModuleMap {
    //modules: HashMap<ModuleId, Module>,
    next_id: u32,
    //pending: HashMap<ModuleId, Global<PromiseResolver>>,
}
impl ModuleMap {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            //pending: HashMap::new(),
        }
    }
    pub fn next_id(&mut self) -> ModuleId {
        let id = self.next_id;
        self.next_id += 1;
        ModuleId(id)
    }
}
