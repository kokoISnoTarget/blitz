#![feature(generic_const_exprs)]
#![feature(let_chains)]

mod document;
mod fetch_thread;
mod html;
mod objects;
mod util;

pub use self::document::JsDocument;

pub use self::html::HtmlParser;

pub use self::fetch_thread::DocumentHandler;
