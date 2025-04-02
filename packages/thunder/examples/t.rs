use thunder::{HtmlParser, JsDocument};
use tracing;
use tracing_subscriber;
use v8::{
    CreateParams, Isolate,
    cppgc::{Heap, HeapCreateParams},
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
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
    let isolate = Isolate::new(CreateParams::default().cpp_heap(heap));

    let mut document = JsDocument::new(isolate);

    document.parse(
        "<html><body>
        <h1>Hello, World!</h1>
        <script>
            let h1 = document.querySelector('h1').remove();
        </script>
        </body></html>",
    );

    document.print_tree();

    #[cfg(feature = "tracing")]
    tracing::info!("Finished");
}
