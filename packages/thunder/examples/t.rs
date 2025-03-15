use thunder::{HtmlParser, JsDocument};
use tracing;
use tracing_subscriber;
use v8::{
    CreateParams, Isolate,
    cppgc::{Heap, HeapCreateParams},
};

fn main() {
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt::init();
        tracing::info!("Tracing initialized");
    }
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform.clone());
    v8::V8::initialize();
    v8::cppgc::initialize_process(platform.clone());

    let heap = Heap::create(platform, HeapCreateParams::default());
    let mut isolate = Isolate::new(CreateParams::default().cpp_heap(heap));

    let mut document = JsDocument::new(isolate);

    HtmlParser::parse(&mut document, "<html><body></body></html>");

    document.print_tree();

    document.setup();

    document.print_tree();

    #[cfg(feature = "tracing")]
    tracing::info!("Finished");
}
