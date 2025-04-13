use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ImportMap {
    pub imports: HashMap<String, String>,
    pub scopes: HashMap<String, HashMap<String, String>>,
    pub integrity: HashMap<String, String>,
}
impl ImportMap {
    pub fn new() -> ImportMap {
        ImportMap {
            imports: HashMap::new(),
            scopes: HashMap::new(),
            integrity: HashMap::new(),
        }
    }

    pub(crate) fn merge(&mut self, mut other: ImportMap) {
        self.imports.extend(other.imports.drain());
        self.scopes.extend(other.scopes.drain());
        self.integrity.extend(other.integrity.drain());
    }

    pub(crate) fn resolve_new<ResolveUrl: Fn(&str) -> String>(&mut self, f: ResolveUrl) {
        self.imports = self.imports.drain().map(|(k, v)| (k, f(&v))).collect();
        self.scopes = self
            .scopes
            .drain()
            .map(|(k, mut v)| (f(&k), v.drain().map(|(k, v)| (k, f(&v))).collect()))
            .collect();
        self.integrity = self.integrity.drain().map(|(k, v)| (f(&k), v)).collect();
    }
}
