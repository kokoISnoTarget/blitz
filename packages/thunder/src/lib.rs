#![feature(generic_const_exprs)]

mod document;
mod html;
mod wrappers;

pub use self::document::JsDocument;

pub use self::html::HtmlParser;
