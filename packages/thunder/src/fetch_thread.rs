use std::{ffi::c_void, sync::Arc};

use blitz_dom::net::Resource;
use blitz_traits::net::{
    Bytes, DummyNetCallback, NetHandler, NetProvider, Request, SharedCallback,
};
use reqwest::header::HeaderMap;
use tokio::{
    runtime::Handle,
    spawn,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
        oneshot::{Sender as OneshotSender, channel as oneshot_channel},
    },
    task::spawn_local,
};
use url::Url;
use v8::{
    Context, Global, HandleScope, Isolate, IsolateHandle, Value,
    script_compiler::{self, CompileOptions, NoCacheReason, Source},
};

use crate::{html::ShouldParse, objects::IsolateExt};

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/81.0";

pub async fn init_fetch_thread(isolate: &mut Isolate) {
    let handle = isolate.thread_safe_handle();
    let (send, recv) = oneshot_channel();

    //std::thread::spawn(|| fetch_thread_main(send, handle));
    spawn(async { fetch_thread_main(send, handle).await });

    let (fetch_thread_sender, should_parse) = recv.await.unwrap();
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

async fn fetch_thread_main(
    ret: OneshotSender<(UnboundedSender<ToFetch>, ShouldParse)>,
    isolate_handle: IsolateHandle,
) {
    let (sender, message_recv) = unbounded_channel();
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
    message_sender: UnboundedSender<ToFetch>,
    message_recv: UnboundedReceiver<ToFetch>,
    tokio_handle: Handle,
    isolate_handle: IsolateHandle,

    should_parse: ShouldParse,

    client: reqwest::Client,

    net_provider_callback: SharedCallback<Resource>,
}
impl FetchThreadState {
    async fn receive(&mut self) {
        while let Some(message) = self.message_recv.recv().await {
            match message {
                ToFetch::FetchForProvider(data) => {
                    let (doc_id, request, handler) = *data;
                    let response = self.fetch_request(request).await;
                    let bytes = response.into_bytes();
                    handler.bytes(doc_id, bytes, self.net_provider_callback.clone());
                }
                ToFetch::SetCallbackForProvider(callback) => {
                    self.net_provider_callback = callback;
                }
                ToFetch::FetchScript(_options) => {}
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
            self.message_sender.send(ToFetch::Quit).unwrap();
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
        match request.url.scheme() {
            "data" => {
                let data_url = data_url::DataUrl::process(request.url.as_str()).unwrap();
                let decoded = data_url.decode_to_vec().unwrap();
                Response::new_local(decoded.0.into())
            }
            "file" => {
                let file_content = tokio::fs::read(request.url.path()).await.unwrap();
                Response::new_local(file_content.into())
            }
            _ => {
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
        .unwrap()
        .evaluate(scope);
        //.instantiate_module2(scope, callback, source_callback)
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

struct ProviderImpl(UnboundedSender<ToFetch>);
impl ProviderImpl {
    pub fn new(fetch_thread_sender: UnboundedSender<ToFetch>) -> Self {
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
        #[cfg(feature = "tracing")]
        tracing::info!("ProviderImpl::fetch: {}", request.url.as_str());
        let content = Box::new((doc_id, request, handler));

        self.0.send(ToFetch::FetchForProvider(content)).unwrap();
    }
}

pub struct FetchThread(UnboundedSender<ToFetch>);
impl FetchThread {
    pub fn new(sender: UnboundedSender<ToFetch>) -> Self {
        FetchThread(sender)
    }
    pub fn fetch(&self, options: ScriptOptions) {
        #[cfg(feature = "tracing")]
        tracing::info!("FetchThread::fetch {options:?}");

        self.0
            .send(ToFetch::FetchScript(Box::new(options)))
            .unwrap();
    }
    pub fn set_net_provider_callback(&self, callback: SharedCallback<Resource>) {
        #[cfg(feature = "tracing")]
        tracing::info!("FetchThread::set_net_provider_callback");

        self.0
            .send(ToFetch::SetCallbackForProvider(callback))
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

#[derive(Debug)]
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
