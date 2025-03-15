use thunder::{HtmlParser, JsDocument};

fn main() {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    let mut document = JsDocument::new();

    HtmlParser::new(&mut document).parse("<html><body></body></html>");
    document.setup();
}
