pub mod console;
pub mod document;
pub mod element;
mod event;
pub mod location;
mod node_list;
pub mod util;
pub mod window;

pub use console::add_console;
pub use document::add_document;
pub use element::Element;
pub use event::EventObject;
use location::Location;
pub use node_list::NodeList;
pub use util::*;
pub use window::add_window;

pub use util::*;

mod tags {
    pub const ELEMENT: u16 = 0x0001;
    pub const EVENT: u16 = 0x0002;
    pub const NODE_LIST: u16 = 0x0003;
}
pub use tags::*;

pub use crate::{HtmlParser, fast_str, fetch_thread::FetchThread, util::OneByteConstExt};
pub use blitz_dom::BaseDocument;
use v8::{
    Context, Exception, Function, FunctionCallback, FunctionCallbackArguments, FunctionTemplate,
    Global, Handle, HandleScope, IndexedPropertyHandlerConfiguration, Integer, Intercepted,
    Isolate, Local, MapFnTo, Number, Object, ObjectTemplate, PropertyAttribute,
    PropertyCallbackArguments, ReturnValue, ScriptOrigin, Value, cppgc::GarbageCollected,
    cppgc::Ptr, null, script_compiler::Source,
};

pub fn init_js_files(scope: &mut HandleScope) {
    let node_list_iter = fast_str!(include_str!("node_list_iter.js")).to_v8(scope);
    let node_list_iter_name = fast_str!("node_list_iter.js").to_v8(scope);
    let node_list_iter_origin = ScriptOrigin::new(
        scope,
        node_list_iter_name.cast(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );
    let mut node_list_iter_source = Source::new(node_list_iter, Some(&node_list_iter_origin));
    v8::script_compiler::compile(
        scope,
        &mut node_list_iter_source,
        v8::script_compiler::CompileOptions::NoCompileOptions,
        v8::script_compiler::NoCacheReason::NoReason,
    )
    .unwrap()
    .run(scope);
}

pub fn init_templates(scope: &mut HandleScope) {
    Element::init(scope);
    EventObject::init(scope);
    NodeList::init(scope);
    Location::init(scope);
}
