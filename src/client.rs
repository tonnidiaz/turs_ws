//! A simple example of hooking up stdin/stdout to a WebSocket stream.
//!
//! This example will connect to a server specified in the argument list and
//! then forward all data read on stdin to the server, printing out all data
//! received on stdout.
//!
//! Note that this is not currently optimized for performance, especially around
//! buffer management. Rather it's intended to show an example of working with a
//! client.
//!
//! You can use this example together with the `server` example.

use std::{pin::Pin, sync::Arc};

use futures::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    net::TcpStream,
    sync::{Mutex, RwLock},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, protocol::Message},
};
use tungstenite::{Bytes, client::IntoClientRequest};
use turs::{log, sleep, Fut, Res};

pub type TBytes = Bytes;
pub type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type TuStream = (SplitSink<Stream, Message>, SplitStream<Stream>);
pub type OnConnectCb =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>;
// pub type ConnectFn<F> = RwLock<Option<Arc<F>>>;
#[derive(Default)]
struct WsInner {
    rdr: Mutex<Option<SplitStream<Stream>>>,
    wrt: Mutex<Option<SplitSink<Stream, Message>>>,
    on_msg: RwLock<Option<Box<dyn Fn(String) -> Fut<()> + Send + Sync + 'static>>>,
    on_bin_msg: RwLock<Option<Box<dyn Fn(Bytes) -> Fut<()> + Send + Sync + 'static>>>,
    on_connect: RwLock<Option<OnConnectCb>>,
    url: String,
    tag: String,
    reconnect_interval: u64,
    max_reconnects: u64,
}

#[derive(Clone)]
pub struct Ws {
    inner: Arc<WsInner>,
    pub url: String,
    pub tag: String,
}

impl Ws {
    /// Just creates a new Ws instance.
    ///
    /// Call self.connect() after.
    pub async fn new(url: &str, tag: &str) -> Self {
        // CryptoProvider::install_default();
        /* turs::log!("{tag} installing provider...");
        rustls::crypto::ring::default_provider().install_default().expect("Failed to install provider.");
        let tag = if tag == "" { "[ws]" } else { tag };
        turs::log!("{tag} provider installed!");
         */
        let ws = Self {
            url: url.to_owned(),
            tag: tag.to_string(),

            inner: WsInner {
                url: url.to_owned(),
                tag: tag.to_owned(),
                reconnect_interval: 5,
                max_reconnects: 5,
                ..Default::default()
            }
            .into(),
        };
        ws
    }

    pub async fn connect(&self) -> Res<()> {
        self.inner.clone().connect().await
    }
    /// sets the on_msg callback
    pub async fn on_msg<F>(&self, f: F)
    where
        F: Fn(String) -> Fut<()> + Sync + Send + 'static,
    {
        self.inner.on_msg.write().await.replace(Box::new(f));
    }

    pub async fn send(&self, msg: &str) -> Result<(), Box<dyn std::error::Error>> {
        // println!("\n[send] {:?}", self.wrt);
        if let Some(s) = self.inner.wrt.lock().await.as_mut() {
            s.send(Message::Text(msg.into())).await?;
        } else {
            sleep(200).await;
            Box::pin(self.send(msg)).await?;
        };
        Ok(())
    }
}

impl WsInner {
    async fn init(&self) -> Res<Stream> {
        let url = (&self.url).into_client_request().unwrap();
        let stream = connect_async(url).await?;
        Ok(stream.0)
    }
    async fn connect(self: Arc<Self>) -> Res<()> {
        log!("{} connecting to {}...", self.tag, self.url);
        let stream = self.init().await?;
        // *self.stream.write().await = stream;
        let (wrt, rdr) = stream.split();
        self.rdr.lock().await.replace(rdr); //RwLock::new(stream.unwrap());
        self.wrt.lock().await.replace(wrt); //RwLock::new(stream.unwrap());
        log!("{} connected!", self.tag);
        if let Some(ref cb) = *self.on_connect.read().await {
            cb().await;
        }
        Box::pin(async {self.clone().read_msg().await}).await;
        Ok(())
    }

    async fn read_msg(self: Arc<Self>) {
        let tag = self.tag.clone();
        let inner = self.clone();
        let reconnect_interval = self.reconnect_interval;
        let max_reconnects = self.max_reconnects;

        while let Some(msg) = inner.rdr.lock().await.as_mut().unwrap().next().await {
            // println!("\n{tag} on_message",);
            match msg {
                Ok(msg) => {
                    match msg {
                        Message::Text(msg) => {
                            let msg = msg.clone();
                            let inner = inner.clone();
                            let tag = tag.clone();
                            tokio::spawn(async move {
                                if let Some(on_msg) = inner.on_msg.read().await.as_ref() {
                                    on_msg(msg.to_string()).await;
                                } else {
                                    println!("\n{} no message readers", tag);
                                }
                            });
                        }

                        Message::Binary(bin) => {
                            let inner = inner.clone();
                            let bin = bin.clone();

                            tokio::spawn(async move {
                                if let Some(on_bin_msg) = inner.on_bin_msg.read().await.as_ref() {
                                    on_bin_msg(bin).await;
                                }
                            });
                        }
                        Message::Close(msg) => println!("\n{tag} Close: {msg:#?}"),
                        _ => {
                            println!("\n{tag} OTHER MSG TYPE:\n {msg:?}");
                        }
                    };
                }
                Err(err) => {
                    eprintln!("\n{tag} ERROR: {err:?}");
                }
            };
        }

        // DISCONNECTED
        log!("{} DISCONNECTED", tag);

        for i in 1..=max_reconnects {
            log!("{} RECONNECT ATTEMPT #{i}...", tag);
            if let Err(err) = inner.clone().connect().await {
                log!("{} FAILED TO RECONNECT. {err:?}", tag);
                tokio::time::sleep(tokio::time::Duration::from_secs(reconnect_interval)).await;
            } else {
                break;
            };
        }
    }
}
