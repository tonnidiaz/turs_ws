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
    connect_async, tungstenite::{self, protocol::Message}, MaybeTlsStream, WebSocketStream
};
use tungstenite::{Bytes, client::IntoClientRequest};
use turs::log;

pub type TBytes = Bytes;
pub type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type TuStream = (SplitSink<Stream, Message>, SplitStream<Stream>);
pub type OnConnectCb =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>;
// pub type ConnectFn<F> = RwLock<Option<Arc<F>>>;
pub struct Ws {
    pub url: String,
    pub tag: String,
    pub rdr: Mutex<Option<SplitStream<Stream>>>,
    pub wrt: Mutex<Option<SplitSink<Stream, Message>>>,
    pub on_msg: RwLock<
        Option<
            Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>,
        >,
    >,
    pub on_bin_msg: RwLock<
        Option<
            Box<
                dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = ()> + Send + Sync>>
                    + Send
                    + Sync
                    + 'static,
            >,
        >,
    >,
    pub on_connect: RwLock<Option<OnConnectCb>>,
    reconnect_interval: u64,
    max_reconnects: u64,
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
        let url_str = url.to_string();
        let ws = Self { 
            url: url_str,
            tag: tag.to_string(),
            rdr: Mutex::new(None),
            wrt: Mutex::new(None),
            on_connect: RwLock::new(None),
            on_msg: RwLock::new(None),
            on_bin_msg: RwLock::new(None),
            reconnect_interval: 5,
            max_reconnects: 5,
        };
        ws
    }

    pub async fn connect(self: Arc<Self>) -> bool {
        let mut ok = false;
        log!(
            "{} connecting to {}...",
            self.tag,
            self.url
        );
        let stream = self.init().await;

        if stream.is_some() {
            // *self.stream.write().await = stream;
            let (wrt, rdr) = stream.unwrap().split();
            self.wrt.lock().await.replace(wrt); //RwLock::new(stream.unwrap());
            self.rdr.lock().await.replace(rdr); //RwLock::new(stream.unwrap());
            ok = true;
        }
        if ok {
            log!("{} connected!", self.tag);
            if let Some(ref cb) = *self.on_connect.read().await {
                cb().await;
            }
            self.read_msg();
        }

        // log!("{:?}", self.stream.read().await.is_some());

        ok
    }

    async fn init(&self) -> Option<Stream> {
        let url = &self.url;
        let tag = &self.tag;
        let url = url.into_client_request().unwrap();
        let stream;
        match connect_async(url).await {
            Ok(v) => stream = v.0,
            Err(err) => {
                let e = err.to_string();
                log!("{tag} failed to connect. {e:?}");
                return None;
            }
        }
        Some(stream)
    }

    fn read_msg(self: Arc<Self>) {
        tokio::spawn(async move {
            let tag = self.tag.clone();
            while let Some(msg) = &self.rdr.lock().await.as_mut().unwrap().next().await {
                // println!("\n{tag} on_message",);
                match msg {
                    Ok(msg) => {
                        match msg {
                            Message::Text(msg) => {
                                let s = self.clone();
                                let msg = msg.clone();
                                tokio::spawn(async move {
                                    if let Some(on_msg) = s.on_msg.read().await.as_ref() {
                                        on_msg(msg.to_string()).await;
                                    } else {
                                        println!("\n{} no message readers", s.tag);
                                    }
                                });
                            }

                            Message::Binary(bin) => {
                                let s = self.clone();
                                let bin = bin.clone();

                                tokio::spawn(async move {
                                    if let Some(on_bin_msg) = s.on_bin_msg.read().await.as_ref() {
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

            for i in 1..=self.max_reconnects {
                log!("{} RECONNECT ATTEMPT #{i}...", tag);
                let t = self.clone().connect().await;
                if t {
                    break;
                } else {
                    tokio::time::sleep(tokio::time::Duration::from_secs(self.reconnect_interval))
                        .await;
                }
            }
        });
    }

    pub async fn send(&self, msg: &str) -> Result<(), Box<dyn std::error::Error>> {
        // println!("\n[send] {:?}", self.wrt);
        if let Some(s) = self.wrt.lock().await.as_mut() {
            match s.send(Message::Text(msg.into())).await {
                Ok(_) => {
                    println!("\n[sent]");
                }
                Err(err) => {
                    println!("Failed to send. {err:?}");
                    return Err(err.into());
                }
            }
        } else {
            return Err("Socket has no stream".into());
        };
        Ok(())
    }
}

pub async fn main(url: &str) {
    log!("STARTING WS CLIENT ON {url}...");
    /* rustls::crypto::ring::default_provider()
    .install_default()
    .expect("Failed to install provider"); */
}
