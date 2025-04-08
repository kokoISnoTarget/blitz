use std::{
    ffi::c_void,
    sync::{Arc, atomic::AtomicBool},
};

use blitz_dom::net::Resource;
use blitz_traits::net::{
    Bytes, DummyNetCallback, NetHandler, NetProvider, Request, SharedCallback,
};
use reqwest::header::{HeaderMap, HeaderName};
use tokio::task::LocalSet;
use tokio::{
    runtime::Handle,
    sync::mpsc::{Receiver, Sender, channel},
    sync::oneshot::{Sender as OneshotSender, channel as oneshot_channel},
};
use url::Url;
use v8::{
    Context, Global, HandleScope, Isolate, IsolateHandle,
    script_compiler::{self, CompileOptions, NoCacheReason, Source},
};

use crate::{html::ShouldParse, objects::IsolateExt, util::todo};

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/81.0";

pub fn init_fetch_thread(isolate: &mut Isolate) {
    let handle = isolate.thread_safe_handle();
    let (send, recv) = oneshot_channel();

    std::thread::spawn(|| fetch_thread_main(send, handle));

    let (fetch_thread_sender, should_parse) = recv.blocking_recv().unwrap();
    let fetch_thread = FetchThread::new(fetch_thread_sender.clone());
    isolate.set_fetch_thread(fetch_thread);
    let provider_impl = ProviderImpl::new(fetch_thread_sender);
    isolate
        .document_mut()
        .set_net_provider(Arc::new(provider_impl));
    isolate.parser().should_parse = should_parse;
    #[cfg(feature = "tracing")]
    tracing::info!("Fetch thread initialized");
}

#[tokio::main(flavor = "multi_thread")]
async fn fetch_thread_main(
    ret: OneshotSender<(Sender<ToFetch>, ShouldParse)>,
    isolate_handle: IsolateHandle,
) {
    let (sender, message_recv) = channel(10);
    let message_sender = sender.clone();

    let should_parse = ShouldParse::new();

    ret.send((sender, should_parse.clone())).unwrap();

    let client = reqwest::ClientBuilder::new().build().unwrap();

    let mut state = FetchThreadState {
        message_sender,
        message_recv,
        tokio_handle: Handle::current(),
        isolate_handle,
        should_parse,

        client,
        net_provider_callback: Arc::new(DummyNetCallback::default()),
    };

    state.receive().await;
}

enum ToFetch {
    Empty,

    FetchForProvider(
        Box<(
            usize,
            blitz_traits::net::Request,
            blitz_traits::net::BoxedHandler<Resource>,
        )>,
    ),
    SetCallbackForProvider(SharedCallback<Resource>),

    FetchScript(Box<ScriptOptions>),

    Quit,
}

struct FetchThreadState {
    message_sender: Sender<ToFetch>,
    message_recv: Receiver<ToFetch>,
    tokio_handle: Handle,
    isolate_handle: IsolateHandle,

    should_parse: ShouldParse,

    client: reqwest::Client,

    net_provider_callback: SharedCallback<Resource>,
}
impl FetchThreadState {
    async fn receive(&mut self) {
        while let Some(message) = self.message_recv.recv().await {
            #[cfg(feature = "tracing")]
            tracing::info!("Received message");
            match message {
                ToFetch::Empty => {}
                ToFetch::FetchForProvider(data) => {
                    #[cfg(feature = "tracing")]
                    tracing::info!("Fetching document for provider");
                    let (doc_id, request, handler) = *data;
                    let response = self.fetch_request(request).await;
                    let bytes = response.into_bytes();
                    handler.bytes(doc_id, bytes, self.net_provider_callback.clone());
                }
                ToFetch::SetCallbackForProvider(callback) => {
                    self.net_provider_callback = callback;
                }
                ToFetch::FetchScript(options) => {
                    todo!();
                }
                ToFetch::Quit => {
                    self.message_recv.close();
                    break;
                }
            }
        }
    }
    async fn run_js(&self) {
        let data = Box::new(CallbackData {
            is_module: false,
            url: Url::parse("localhost").unwrap(),
        });
        let data_ptr = Box::into_raw(data) as *mut c_void;
        if !self.isolate_handle.request_interrupt(callback, data_ptr) {
            self.message_sender.send(ToFetch::Quit).await.unwrap();
        }
        self.should_parse
            .0
            .state
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(waker) = self.should_parse.0.waker.lock().unwrap().as_ref() {
            waker.wake_by_ref();
        }
    }

    async fn fetch_request(&self, request: Request) -> Response {
        let response = self
            .client
            .request(request.method, request.url)
            .headers(request.headers)
            .header("User-Agent", USER_AGENT)
            .body(request.body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.bytes().await.unwrap();
        Response::new_net(status, headers, body)
    }
}

unsafe extern "C" fn callback(isolate: &mut Isolate, data: *mut c_void) {
    let callback_data = unsafe { *Box::from_raw(data as *mut CallbackData) };
    callback_inner(isolate, callback_data);
}
fn callback_inner(isolate: &mut Isolate, callback_data: CallbackData) {
    let context = isolate.remove_slot::<Global<Context>>().unwrap();
    let scope = &mut HandleScope::with_context(isolate, &context);

    let source_string = v8::String::new(scope, "value").unwrap();
    //let origin = v8::ScriptOrigin::new(scope, resource_name, resource_line_offset, resource_column_offset, resource_is_shared_cross_origin, script_id, source_map_url, resource_is_opaque, is_wasm, is_module, host_defined_options)
    let source = &mut Source::new(source_string, None);
    if callback_data.is_module {
        let module = script_compiler::compile_module2(
            scope,
            source,
            CompileOptions::NoCompileOptions,
            NoCacheReason::NoReason,
        )
        .unwrap();
    } else {
        let script = script_compiler::compile(
            scope,
            source,
            CompileOptions::NoCompileOptions,
            NoCacheReason::NoReason,
        )
        .unwrap();
    }

    scope.set_slot(context);
}
struct CallbackData {
    is_module: bool,
    url: Url,
}

struct ProviderImpl(Sender<ToFetch>);
impl ProviderImpl {
    pub fn new(fetch_thread_sender: Sender<ToFetch>) -> Self {
        ProviderImpl(fetch_thread_sender)
    }
}

impl NetProvider for ProviderImpl {
    type Data = Resource;
    fn fetch(
        &self,
        doc_id: usize,
        request: blitz_traits::net::Request,
        handler: blitz_traits::net::BoxedHandler<Self::Data>,
    ) {
        let content = Box::new((doc_id, request, handler));
        let handle = Handle::current();
        let sender = self.0.clone();
        handle.spawn(async move {
            sender
                .send(ToFetch::FetchForProvider(content))
                .await
                .unwrap();
        });
    }
}

pub struct FetchThread(Sender<ToFetch>);
impl FetchThread {
    pub fn new(sender: Sender<ToFetch>) -> Self {
        FetchThread(sender)
    }
    pub fn fetch(&self, options: ScriptOptions) {
        self.0
            .blocking_send(ToFetch::FetchScript(Box::new(options)))
            .unwrap();
    }
}

enum Response {
    Net {
        status: u16,
        headers: HeaderMap,
        body: Bytes,
    },
    Local(Bytes),
}
impl Response {
    fn new_net(status: u16, headers: HeaderMap, body: Bytes) -> Self {
        Response::Net {
            status,
            headers,
            body,
        }
    }

    fn new_local(body: Bytes) -> Self {
        Response::Local(body)
    }

    fn into_bytes(self) -> Bytes {
        match self {
            Response::Net { body, .. } => body,
            Response::Local(body) => body,
        }
    }
}

pub struct ScriptOptions {
    pub url: Url,
    pub is_module: bool,
    pub is_defer: bool,
    pub is_async: bool,
}

pub struct DocumentHandler(pub OneshotSender<Bytes>);
impl NetHandler for DocumentHandler {
    type Data = Resource;
    fn bytes(self: Box<Self>, _doc_id: usize, bytes: Bytes, _callback: SharedCallback<Self::Data>) {
        self.0.send(bytes).unwrap();
    }
}
