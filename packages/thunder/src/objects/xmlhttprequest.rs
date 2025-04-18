#[cfg(feature = "tracing")]
use tracing::*;

use super::*;
use crate::v8intergration::{HandleScopeExt as _, IsolateExt}; // Import the extension trait
use std::sync::{Arc, Mutex};
// TODO: Add reqwest to Cargo.toml
use crate::fetch_thread::{FetchThread, ToFetch, XhrRequestDetails, XhrResponseDetails}; // Removed Bytes import
use blitz_dom::net::Resource; // For NetHandler associated type
use blitz_traits::net::{Bytes, NetHandler, SharedCallback, http}; // Restored Bytes import here
use reqwest::header::HeaderMap;
use reqwest::{self, Method}; // Assuming reqwest is used by FetchThread internally
use std::ffi::c_void;
use tokio::sync::mpsc::UnboundedSender; // For FetchThread sender type
use url::Url; // Use http::Method
use v8::{Function, Global, Local, Object, TryCatch}; // Added Object

// Represents the state of an XMLHttpRequest according to the spec
// https://xhr.spec.whatwg.org/#states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ReadyState {
    Unsent = 0,
    Opened = 1,
    HeadersReceived = 2, // TODO: Set this state when headers arrive
    Loading = 3,         // TODO: Set this state during download
    Done = 4,
}

// Internal state for XMLHttpRequest
// Needs to be Send + Sync to be safely shared across threads
pub(crate) struct XMLHttpRequestState {
    ready_state: ReadyState,
    method: Option<http::Method>, // Use http::Method
    url: Option<url::Url>,        // Use url::Url
    // async: bool, // TODO
    status: u16, // HTTP status code
    status_text: String,
    response_text: Option<String>,
    // TODO: Add request headers, response headers, etc.
}

impl Default for XMLHttpRequestState {
    fn default() -> Self {
        Self {
            ready_state: ReadyState::Unsent,
            method: None,
            url: None,
            status: 0,
            status_text: String::new(),
            response_text: None,
        }
    }
}

// Callback type for when the fetch thread completes the XHR request.
pub type XhrReadyStateCallback =
    Arc<Box<dyn FnOnce(Result<XhrResponseDetails, String>, &mut Isolate) + Send + Sync + 'static>>;

pub struct XMLHttpRequest {
    // State is shared between JS object and potential background tasks
    state: Arc<Mutex<XMLHttpRequestState>>,
    // Add fields for event handlers
    onreadystatechange: Option<Global<Function>>,
}

impl GarbageCollected for XMLHttpRequest {}

impl WrappedObject for XMLHttpRequest {
    const TAG: u16 = XML_HTTP_REQUEST;

    fn init_template<'s>(scope: &mut v8::HandleScope<'s>, proto: v8::Local<v8::ObjectTemplate>) {
        // --- Properties ---
        let readystate_key = v8::String::new(scope, "readyState").unwrap();
        proto.set_accessor(readystate_key.into(), Self::get_ready_state);

        let status_key = v8::String::new(scope, "status").unwrap();
        proto.set_accessor(status_key.cast(), Self::get_status);

        // TODO: Add statusText getter

        let responsetext_key = v8::String::new(scope, "responseText").unwrap();
        proto.set_accessor(responsetext_key.into(), Self::get_response_text);

        let onreadystatechange_key = v8::String::new(scope, "onreadystatechange").unwrap();
        proto.set_accessor_with_setter(
            onreadystatechange_key.into(),
            Self::get_onreadystatechange,
            Self::set_onreadystatechange,
        ); // Use actual getter

        // --- Methods ---
        let open_key = v8::String::new(scope, "open").unwrap();
        let open_fn = v8::FunctionTemplate::new(scope, Self::open);
        proto.set(open_key.into(), open_fn.into());

        let send_key = v8::String::new(scope, "send").unwrap();
        let send_fn = v8::FunctionTemplate::new(scope, Self::send);
        proto.set(send_key.into(), send_fn.into());

        // TODO: Add setRequestHeader, getResponseHeader, getAllResponseHeaders, abort
        // TODO: Add event handlers (onreadystatechange, onload, onerror, etc.)
    }

    const CLASS_NAME: &'static str = "XMLHttpRequest";

    fn init_function(
        scope: &mut HandleScope<'_>,
        args: FunctionCallbackArguments<'_>,
        mut ret: ReturnValue,
    ) {
        let this = args.this();
        let member = XMLHttpRequest::new();
        let heap = scope.get_cpp_heap().unwrap();
        let member = unsafe { v8::cppgc::make_garbage_collected(heap, member) };
        unsafe {
            v8::Object::wrap::<{ Self::TAG }, Self>(scope, this, &member);
        }
        ret.set(this.cast());
    }

    fn object<'s>(self, scope: &mut HandleScope<'s>) -> Local<'s, Object>
    where
        Self: Sized + 'static,
        [(); { Self::TAG } as usize]:,
    {
        let template = scope
            .get_fn_template::<Self>()
            .expect("Objects should get initialized before first creation");
        let template = Local::new(scope, template);
        let func = template.get_function(scope).unwrap();
        let obj = func.new_instance(scope, &[]).unwrap();

        assert!(obj.is_api_wrapper(), "Object is not an API wrapper");

        let heap = scope.get_cpp_heap().unwrap();
        let member = unsafe { v8::cppgc::make_garbage_collected(heap, self) };
        unsafe {
            v8::Object::wrap::<{ Self::TAG }, Self>(scope, obj, &member);
        }
        obj
    }
}

// --- V8 Bindings ---

impl XMLHttpRequest {
    // Creates a new XMLHttpRequest instance with default state
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(XMLHttpRequestState::default())),
            onreadystatechange: None,
        }
    }

    // Constructor called when `new XMLHttpRequest()` is used in JS
    fn constructor(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let obj = XMLHttpRequest::new().object(scope);
        rv.set(obj.cast());
    }

    // Getter for readyState
    fn get_ready_state(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        args: v8::PropertyCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let obj = args.this().unwrap_as::<XMLHttpRequest>(scope);
        let state = obj.state.lock().unwrap(); // Handle potential poisoning later
        let ready_state_val = v8::Integer::new(scope, state.ready_state as i32);
        rv.set(ready_state_val.into());
    }

    // Getter for status
    fn get_status(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        args: v8::PropertyCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let obj = args.this().unwrap_as::<XMLHttpRequest>(scope);
        let state = obj.state.lock().unwrap();
        let status_val = v8::Integer::new(scope, state.status as i32);
        rv.set(status_val.into());
    }

    // Getter for responseText
    fn get_response_text(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        args: v8::PropertyCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let obj = args.this().unwrap_as::<XMLHttpRequest>(scope);
        let state = obj.state.lock().unwrap();
        match &state.response_text {
            Some(text) => {
                let text_val = v8::String::new(scope, text).unwrap();
                rv.set(text_val.into());
            }
            None => {
                rv.set(v8::null(scope).into());
            }
        }
    }

    // Method: open(method, url, async, user, password)
    fn open(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        _rv: v8::ReturnValue,
    ) {
        let obj = args.this().unwrap_as::<XMLHttpRequest>(scope);
        let mut state = obj.state.lock().unwrap();

        // TODO: Proper argument validation and type checking
        let method_str = args.get(0).to_rust_string_lossy(scope);
        let url_str = args.get(1).to_rust_string_lossy(scope);
        // let async_req = args.get(2).boolean_value(scope); // TODO

        #[cfg(feature = "tracing")]
        info!(method = %method_str, url = %url_str, "[XHR] open called");

        // Parse method
        match http::Method::from_bytes(method_str.as_bytes()) {
            Ok(method) => state.method = Some(method),
            Err(_) => {
                // TODO: Throw JS TypeError for invalid method
                #[cfg(feature = "tracing")]
                error!(method = %method_str, "[XHR] Error: Invalid HTTP method");
                return;
            }
        }

        // Parse URL
        state.url = Some(scope.document().resolve_url(&url_str));

        state.ready_state = ReadyState::Opened;
        // Reset other state variables
        state.status = 0;
        state.status_text = String::new();
        state.response_text = None;

        // TODO: Trigger onreadystatechange if handler exists
    }

    // Method: send(body)
    fn send(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        _rv: v8::ReturnValue,
    ) {
        let this = args.this().unwrap_as::<XMLHttpRequest>(scope);
        let state = this.state.lock().unwrap();

        if state.ready_state != ReadyState::Opened {
            let str = fast_str!("InvalidStateError").to_v8(scope);
            let exception = Exception::error(scope, str);
            scope.throw_exception(exception);
        }

        // TODO: 2. If this’s send() flag is set, then throw an "InvalidStateError" DOMException.

        let mut body = args.get(0);
        if matches!(
            state.method.clone().unwrap_or_default().as_str(),
            "GET" | "PUT"
        ) {
            body = null(scope).cast();
        }
        if !body.is_null() {
            let message = fast_str!("TODO: send with body").to_v8(scope);
            let exception = Exception::error(scope, message);
            scope.throw_exception(exception.cast());
            #[cfg(feature = "tracing")]
            tracing::error!("TODO: send with body");
            return;
        }

        scope.fetch_thread().send_xhr_request(XhrRequestDetails {
            method: (),
            url: (),
            headers: (),
            body: (),
            callback: (),
        });
    }

    // --- Event Handler Setter/Getter ---

    // Getter for onreadystatechange
    fn get_onreadystatechange(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        args: v8::PropertyCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let obj = args.this().unwrap_as::<XMLHttpRequest>(scope);

        match &obj.onreadystatechange {
            Some(global_func) => {
                let local = Local::new(scope, global_func);
                rv.set(local.cast());
            }
            None => {
                rv.set_null();
            }
        }
    }
    /*
    &mut HandleScope<'s>,
      Local<'s, Name>,
      Local<'s, Value>,
      PropertyCallbackArguments<'s>,
      ReturnValue<()> */

    // Setter for onreadystatechange
    fn set_onreadystatechange(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        value: v8::Local<v8::Value>,
        args: v8::PropertyCallbackArguments,
        ret: ReturnValue<()>,
    ) {
        let this = args.this();
        // Get MUTABLE pointer to the Rust struct
        let xhr_ptr =
            unsafe { this.get_aligned_pointer_from_internal_field(0) } as *mut XMLHttpRequest;
        if xhr_ptr.is_null() {
            #[cfg(feature = "tracing")]
            warn!("[XHR] Failed to get internal pointer in set_onreadystatechange");
            return;
        }
        let obj = unsafe { &mut *xhr_ptr };

        if value.is_function() {
            let func = Local::<v8::Function>::try_from(value).unwrap();
            let global_func = v8::Global::new(scope, func);
            obj.onreadystatechange = Some(global_func);
            #[cfg(feature = "tracing")]
            info!("[XHR] onreadystatechange handler set.");
        } else if value.is_null_or_undefined() {
            obj.onreadystatechange = None;
            #[cfg(feature = "tracing")]
            info!("[XHR] onreadystatechange handler removed.");
        } else {
            #[cfg(feature = "tracing")]
            warn!("[XHR] Attempted to set onreadystatechange with non-function/null value.");
            obj.onreadystatechange = None;
        }
    }
} // impl XMLHttpRequest
