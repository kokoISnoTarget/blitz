pub mod console;
pub mod document;
pub mod element;
mod event;
pub mod util;
pub mod window;

pub use console::add_console;
pub use document::add_document;
pub use event::event_object;
pub use window::add_window;

pub use util::*;

mod tag {
    pub const ELEMENT: u16 = 0x0001;
    pub const EVENT: u16 = 0x0002;
}
pub use tag::*;
pub use util::empty;

use v8::{Context, HandleScope, Local};
