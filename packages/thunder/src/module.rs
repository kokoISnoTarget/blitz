use std::{collections::HashMap, ffi::c_int};

use v8::{CachedData, Global, HandleScope, Local, Module, Object, PromiseResolver, UniqueRef};

use crate::v8intergration::IsolateExt;

pub(crate) fn initialize_import_meta_object_callback(
    scope: &mut HandleScope,
    module: Local<Module>,
    _meta: Local<Object>,
) {
    let module_id = module.script_id().expect("Failed to get module ID") as u32;

    // https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/import.meta

    // HostInitializeImportMetaObjectCallback is called the first time import.meta is accessed for a module. Subsequent access will reuse the same value.
    // The method combines two implementation-defined abstract operations into one: HostGetImportMetaProperties and HostFinalizeImportMeta.
    // The embedder should use v8::Object::CreateDataProperty to add properties on the meta object.
}

pub(crate) fn host_import_module_dynamically_callback<'s>(
    scope: &mut v8::HandleScope<'s>,
    host_defined_options: v8::Local<'s, v8::Data>,
    resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let resolver = PromiseResolver::new(scope).unwrap();
    let out = resolver.get_promise(scope);

    let specifier = specifier.to_rust_string_lossy(scope);

    #[cfg(feature = "tracing")]
    tracing::info!("::host_import_module_dynamically_callback {specifier}");

    let name = scope.importmap().resolve(&specifier);

    let resolver = Global::new(scope, resolver);
    let module_id = scope.modulemap().create_entry(name, resolver);

    Some(out)
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ModuleId(u32);

enum ModuleState {
    Pending {
        resolver: Global<PromiseResolver>,
    },
    Loaded {
        module: Global<Module>,
        cache: Option<UniqueRef<CachedData<'static>>>,
    },
}

struct ModuleEntry {
    name: String,
    script_id: Option<c_int>,
    state: ModuleState,
}

pub struct ModuleMap {
    entries: HashMap<ModuleId, ModuleEntry>,
    module_name_to_id: HashMap<String, ModuleId>,
    script_id_to_id: HashMap<c_int, ModuleId>,
    next_id: u32,
}
impl ModuleMap {
    pub fn new() -> ModuleMap {
        ModuleMap {
            entries: HashMap::new(),
            module_name_to_id: HashMap::new(),
            script_id_to_id: HashMap::new(),
            next_id: 0,
        }
    }
    fn create_entry(&mut self, name: String, resolver: Global<PromiseResolver>) -> ModuleId {
        let id = ModuleId(self.next_id);
        self.next_id += 1;

        let entry = ModuleEntry {
            name: name.clone(),
            script_id: None,
            state: ModuleState::Pending { resolver },
        };
        self.entries.insert(id, entry);
        self.module_name_to_id.insert(name, id);

        id
    }
}
