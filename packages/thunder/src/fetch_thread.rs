use blitz_dom::net::Resource;
use blitz_traits::net::{
    Bytes, DummyNetCallback, NetHandler, NetProvider, Request, SharedCallback,
};
use reqwest::header::HeaderMap;
use std::task::Waker;
use std::{ffi::c_void, ops::Deref, sync::Arc};
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
use winit::window::WindowId;

use crate::application::EventProxy;
use crate::html::ShouldParse;
use crate::module::ModuleId;
use crate::objects::xmlhttprequest::XhrReadyStateCallback;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/81.0";

pub fn init_fetch_thread(proxy: EventProxy) -> (FetchThread, ProviderImpl) {
    let (send, recv) = oneshot_channel();

    std::thread::spawn(|| fetch_thread_main(send, proxy));

    let (fetch_thread_sender, should_parse) = recv.blocking_recv().unwrap();
    let fetch_thread = FetchThread::new(fetch_thread_sender.clone());
    let provider_impl = ProviderImpl::new(fetch_thread_sender);

    #[cfg(feature = "tracing")]
    tracing::info!("Fetch thread initialized");
    (fetch_thread, provider_impl)
}

#[tokio::main(flavor = "current_thread")]
pub(crate) async fn fetch_thread_main(
    ret: OneshotSender<(UnboundedSender<ToFetch>, ShouldParse)>,
    proxy: EventProxy,
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
        net_provider_callback: proxy.net_callback(),
        proxy,
        client,
    };

    state.receive().await;
}

pub enum ToFetch {
    FetchForProvider(
        Box<(
            usize,
            blitz_traits::net::Request,
            blitz_traits::net::BoxedHandler<Resource>,
        )>,
    ),
    FetchDocument(Box<Url>),

    SetAssociatedWindow(WindowId),

    FetchScript(Box<ScriptOptions>),

    XhrRequest(Box<XhrRequestDetails>),

    Quit,
}

struct FetchThreadState {
    message_sender: UnboundedSender<ToFetch>,
    message_recv: UnboundedReceiver<ToFetch>,
    tokio_handle: Handle,

    proxy: EventProxy,

    client: reqwest::Client,

    net_provider_callback: SharedCallback<Resource>,
}
impl FetchThreadState {
    async fn receive(&mut self) {
        while let Some(message) = self.message_recv.recv().await {
            match message {
                ToFetch::Quit => {
                    self.message_recv.close();
                    break;
                }
                ToFetch::SetAssociatedWindow(window_id) => {
                    // This is a simple operation that affects state, do it inline
                    self.proxy.set_window(window_id);
                }
                // Spawn concurrent tasks for the fetch operations
                other_message => {
                    let client = self.client.clone();
                    let proxy = self.proxy.clone();
                    let net_provider_callback = self.net_provider_callback.clone();

                    spawn(async move {
                        match other_message {
                            ToFetch::FetchForProvider(data) => {
                                let (doc_id, request, handler) = *data;
                                let response = Self::fetch_request(&client, request).await;
                                let bytes = response.into_bytes();
                                handler.bytes(doc_id, bytes, net_provider_callback.clone());
                            }
                            ToFetch::FetchScript(options) => {
                                if options.loading_style != ScriptLoadingStyle::Blocking {
                                    proxy.repoll_parser();
                                    todo!("Only clasic script suported currently");
                                }
                                let response =
                                    Self::fetch_request(&client, Request::get(options.url.clone()))
                                        .await;

                                //let data = Box::new(Script {
                                //    options: *options,
                                //    data: response.into_bytes(),
                                //});
                                proxy.fetched_script(response.into_bytes(), options);
                            }
                            ToFetch::FetchDocument(url) => {
                                let request = Request::get((*url).clone());
                                let response = Self::fetch_request(&client, request).await;

                                proxy.fetched_document((*url).into(), response.into_bytes());
                            }
                            ToFetch::XhrRequest(details) => {
                                //Self::fetch_xhr_request(&client, *details, proxy).await;
                            }
                            _ => unreachable!(),
                        }
                    });
                }
            }
        }
    }

    async fn fetch_request(client: &reqwest::Client, request: Request) -> Response {
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
                let response = client
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

pub struct Script {
    options: ScriptOptions,
    data: Bytes,
}

pub(crate) struct ProviderImpl(UnboundedSender<ToFetch>);
impl ProviderImpl {
    fn new(fetch_thread_sender: UnboundedSender<ToFetch>) -> Self {
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
    pub fn fetch_document(&self, url: Url) {
        #[cfg(feature = "tracing")]
        tracing::info!("FetchThread::fetch_document {}", url.as_str());

        self.0.send(ToFetch::FetchDocument(Box::new(url))).unwrap();
    }
    pub fn set_window(&self, window_id: WindowId) {
        self.0
            .send(ToFetch::SetAssociatedWindow(window_id))
            .unwrap();
    }

    pub fn send_xhr_request(&self, details: XhrRequestDetails) {
        #[cfg(feature = "tracing")]
        tracing::info!(method = %details.method, url = %details.url, "[FetchThread] Queuing XHR request");
        self.0.send(ToFetch::XhrRequest(Box::new(details))).unwrap();
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

#[derive(Debug, Clone)]
pub struct ScriptOptions {
    pub url: Url,
    pub module: Option<ModuleId>,
    pub loading_style: ScriptLoadingStyle,
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ScriptLoadingStyle {
    /// Clasic script which is fetched and executed while blocking parsing
    Blocking,
    /// Fetched without blocking parsing and executed after parsing ended
    /// <script defer> and <script type="module">
    AsyncDefer,
    /// Fetched without blocking parsing and executed immediate after fetching completed this blocks parsing.
    /// <script async> and <script type="module" async>
    AsyncImmediate,
}
impl ScriptLoadingStyle {
    pub fn from_attrs(is_async: bool, is_defer: bool, is_module: bool) -> ScriptLoadingStyle {
        if is_async {
            ScriptLoadingStyle::AsyncImmediate
        } else if is_defer || is_module {
            ScriptLoadingStyle::AsyncDefer
        } else {
            ScriptLoadingStyle::Blocking
        }
    }
}

pub struct DocumentHandler(pub OneshotSender<Bytes>);
impl NetHandler for DocumentHandler {
    type Data = Resource;
    fn bytes(self: Box<Self>, _doc_id: usize, bytes: Bytes, _callback: SharedCallback<Self::Data>) {
        self.0.send(bytes).unwrap();
    }
}

pub struct XhrRequestDetails {
    pub method: reqwest::Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub callback: XhrReadyStateCallback,
}
impl std::fmt::Debug for XhrRequestDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct XhrResponseDetails {
    pub status: u16,
    pub status_text: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}
