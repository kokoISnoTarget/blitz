use std::{cmp::Ordering, collections::BinaryHeap, sync::Arc};

use blitz_dom::net::Resource;
use blitz_traits::net::{BoxedHandler, Bytes, NetHandler, NetProvider, Request, SharedCallback};
use data_url::DataUrl;
use reqwest::Client;
use thiserror::Error;
use tokio::spawn;

/// Net implementation with an priority queue.
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/81.0";

pub struct ThunderProvider {
    client: Client,
    callback: SharedCallback<Resource>,
}
impl ThunderProvider {
    pub fn new(client: Client, callback: SharedCallback<Resource>) -> Self {
        Self { client, callback }
    }
    pub async fn fetch_imediate(&self, request: Request) -> Bytes {
        Self::fetch_inner(self.client.clone(), request)
            .await
            .unwrap()
    }

    async fn fetch_inner(client: Client, request: Request) -> Result<Bytes, ProviderError> {
        match request.url.scheme() {
            "data" => {
                let data_url = DataUrl::process(request.url.as_str())?;
                let decoded = data_url.decode_to_vec()?;
                Ok(Bytes::from(decoded.0))
            }
            "file" => {
                let file_content = std::fs::read(request.url.path())?;
                Ok(Bytes::from(file_content))
            }
            _ => {
                let response = client
                    .request(request.method, request.url)
                    .headers(request.headers)
                    .header("User-Agent", USER_AGENT)
                    .body(request.body)
                    .send()
                    .await?;

                Ok(response.bytes().await?)
            }
        }
    }
}

impl NetProvider for ThunderProvider {
    type Data = Resource;
    fn fetch(&self, doc_id: usize, request: Request, handler: BoxedHandler<Resource>) {
        let client = self.client.clone();
        let callback = Arc::clone(&self.callback);
        println!("Fetching {}", &request.url);
        drop(spawn(async move {
            let url = request.url.to_string();
            let res = Self::fetch_inner(client, request).await;
            match res {
                Ok(bytes) => {
                    handler.bytes(doc_id, bytes, callback);
                    println!("Success {}", url);
                }
                Err(e) => {
                    eprintln!("Error fetching {}: {e}", url);
                }
            }
        }));
    }
}

#[derive(Error, Debug)]
enum ProviderError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    DataUrl(#[from] data_url::DataUrlError),
    #[error("{0}")]
    DataUrlBas64(#[from] data_url::forgiving_base64::InvalidBase64),
    #[error("{0}")]
    ReqwestError(#[from] reqwest::Error),
}

pub struct OuterJsHandler {
    pub node_id: usize,
    pub defer: bool,
}
impl NetHandler for OuterJsHandler {
    type Data = Resource;

    fn bytes(self: Box<Self>, doc_id: usize, bytes: Bytes, callback: SharedCallback<Self::Data>) {
        callback.call(doc_id, Resource::ScriptFile(self.node_id, bytes));
    }
}
