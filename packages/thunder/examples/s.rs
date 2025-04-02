#![feature(string_from_utf8_lossy_owned)]

use core::str;
use std::sync::Arc;

use blitz_renderer_vello::BlitzVelloRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent, WindowConfig, create_default_event_loop};
use blitz_traits::{navigation::DummyNavigationProvider, net::Request};
use thunder::{DocumentHandler, JsDocument};
use url::Url;
use v8::inspector::V8Inspector;
use winit::window::WindowAttributes;

fn main() {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt::init();
        tracing::info!("Tracing initialized");
    }

    //launch_static_html(include_str!("../../../google.html"));
    launch_url("https://google.com");
}

fn launch_static_html(source: &str) {
    let event_loop = create_default_event_loop::<BlitzShellEvent>();

    let navigation_provider = Arc::new(DummyNavigationProvider);

    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform.clone());
    v8::V8::initialize();
    v8::cppgc::initialize_process(platform.clone());

    let heap = v8::cppgc::Heap::create(platform, v8::cppgc::HeapCreateParams::default());
    let isolate = v8::Isolate::new(v8::CreateParams::default().cpp_heap(heap));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _enter = runtime.enter();

    let mut doc = JsDocument::new(isolate);

    runtime.block_on(doc.parse(source));

    let window_attributes = WindowAttributes::default();

    let window: WindowConfig<JsDocument, BlitzVelloRenderer> =
        WindowConfig::with_attributes(doc, window_attributes);

    // Create application
    let mut application = BlitzApplication::new(event_loop.create_proxy());
    application.add_window(window);

    // Run event loop
    event_loop.run_app(&mut application).unwrap()
}
fn launch_url(url: &str) {
    let event_loop = create_default_event_loop::<BlitzShellEvent>();

    let navigation_provider = Arc::new(DummyNavigationProvider);

    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform.clone());
    v8::V8::initialize();
    v8::cppgc::initialize_process(platform.clone());

    let heap = v8::cppgc::Heap::create(platform, v8::cppgc::HeapCreateParams::default());
    let isolate = v8::Isolate::new(v8::CreateParams::default().cpp_heap(heap));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _enter = runtime.enter();

    let (send, recv) = tokio::sync::oneshot::channel();

    let mut doc = JsDocument::new(isolate);
    doc.net_provider.fetch(
        0,
        Request::get(Url::parse(url).unwrap()),
        Box::new(DocumentHandler(send)),
    );

    doc.as_mut().set_base_url(url);

    runtime.block_on(async {
        let response = recv.await.unwrap();

        let string = String::from_utf8(response.to_vec());

        let str = string.unwrap_or_else(|err| err.into_utf8_lossy());
        doc.parse(&str).await;
    });

    let window_attributes = WindowAttributes::default();

    let window: WindowConfig<JsDocument, BlitzVelloRenderer> =
        WindowConfig::with_attributes(doc, window_attributes);

    // Create application
    let mut application = BlitzApplication::new(event_loop.create_proxy());
    application.add_window(window);

    // Run event loop
    event_loop.run_app(&mut application).unwrap()
}
