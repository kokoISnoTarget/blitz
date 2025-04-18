use std::ops::{Deref, DerefMut};

use v8::{Global, UnboundScript};

pub struct DeferedScripts(Vec<Global<UnboundScript>>);
impl DeferedScripts {
    pub fn new() -> DeferedScripts {
        DeferedScripts(Vec::new())
    }
}
impl Deref for DeferedScripts {
    type Target = Vec<Global<UnboundScript>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for DeferedScripts {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
