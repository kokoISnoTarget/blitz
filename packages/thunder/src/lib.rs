#![feature(generic_const_exprs)]
#![feature(let_chains)]
#![feature(string_from_utf8_lossy_owned)]

mod application;
mod document;
mod fetch_thread;
mod html;
mod importmap;
mod module;
mod objects;
mod rusty_v8_ext;
mod script;
mod util;
mod v8intergration;

use application::ThunderApplication;
use blitz_renderer_vello::BlitzVelloRenderer;
use blitz_shell::{
    BlitzApplication, BlitzShellEvent, BlitzShellNetCallback, WindowConfig,
    create_default_event_loop,
};
use blitz_traits::net::Request;
use url::Url;
use v8intergration::IsolateExt;
use winit::window::WindowAttributes;

pub use self::document::JsDocument;

pub use self::html::HtmlParser;

pub use self::fetch_thread::DocumentHandler;

pub fn launch_static_html(base_url: &str, source: &str) {
    let event_loop = create_default_event_loop::<BlitzShellEvent>();

    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform.clone());
    v8::V8::initialize();
    v8::cppgc::initialize_process(platform.clone());

    let heap = v8::cppgc::Heap::create(platform, v8::cppgc::HeapCreateParams::default());
    let isolate = v8::Isolate::new(v8::CreateParams::default().cpp_heap(heap));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _enter = runtime.enter();

    let mut doc = JsDocument::new(isolate, event_loop.create_proxy());
    doc.as_mut().set_base_url(base_url);
    doc.add_source(source);

    let window_attributes = WindowAttributes::default();

    let window: WindowConfig<JsDocument, BlitzVelloRenderer> =
        WindowConfig::with_attributes(doc, window_attributes);

    // Create application
    let mut application = ThunderApplication::new(event_loop.create_proxy());
    application.add_window(window);

    // Run event loop
    event_loop.run_app(&mut application).unwrap()
}
pub fn launch_url(url: &str) {
    let event_loop = create_default_event_loop::<BlitzShellEvent>();

    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform.clone());
    v8::V8::initialize();
    v8::cppgc::initialize_process(platform.clone());

    let heap = v8::cppgc::Heap::create(platform, v8::cppgc::HeapCreateParams::default());
    let isolate = v8::Isolate::new(v8::CreateParams::default().cpp_heap(heap));
    #[cfg(feature = "tracing")]
    tracing::info!("Init Isolate");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _enter = runtime.enter();

    let mut doc = JsDocument::new(isolate, event_loop.create_proxy());

    doc.isolate
        .fetch_thread()
        .fetch_document(Url::parse(url).unwrap());

    let window_attributes = WindowAttributes::default();

    let window: WindowConfig<JsDocument, BlitzVelloRenderer> =
        WindowConfig::with_attributes(doc, window_attributes);

    // Create application
    let mut application = BlitzApplication::new(event_loop.create_proxy());
    application.add_window(window);

    // Run event loop
    event_loop.run_app(&mut application).unwrap()
}
