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

use futures::{SinkExt, StreamExt};
use tokio::{net::TcpStream, sync::{RwLock, Mutex}};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message,
};
use tungstenite::client::IntoClientRequest;

use crate::log;

pub type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type OnConnectCb =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>;
// pub type ConnectFn<F> = RwLock<Option<Arc<F>>>;
pub struct Ws {
    pub url: String,
    pub tag: String,
    pub stream: Option<Stream>,
    pub on_msg: RwLock<
        Option<Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>>,
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
        let tag = if tag == "" { "[ws]" } else { tag };

        let url_str = url.to_string();
        let ws = Self {
            url: url_str,
            tag: tag.to_string(),
            stream: RwLock::new(None),
            on_connect: RwLock::new(None),
            on_msg: RwLock::new(None),
            reconnect_interval: 5,
            max_reconnects: 5,
        };
        ws
    }

    /* pub async fn connect(self: Arc<Self>) -> bool {
        let mut ok = false;
        log!("{} connecting to {}...", self.tag, self.url);
        let stream = self.init().await;//Self::init(&self.url, &self.tag, &self.on_connect).await;
        if stream.is_some() {
            *self.stream.write().await = stream;
            ok = true;
        }
        if ok {
            self.read_msg();
        }
        ok
    } */
    pub async fn connect(self: Arc<Self>) -> bool {
        let mut ok = false;

        let stream = self.init().await;
        log!(
            "{} connecting to {} {:?}...",
            self.tag,
            self.url,
            stream.is_some()
        );
        if stream.is_some() {
            // *self.stream.write().await = stream;
            *self.stream.write().await = stream; //RwLock::new(stream.unwrap());
            ok = true;
        }
        if ok {
            log!("{} connected!", self.tag);
            if let Some(ref cb) = *self.on_connect.read().await {
                cb().await;
            }
            self.clone().read_msg();
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
                log!("{tag} failed to connect. {err:?}");
                return None;
            }
        }
        Some(stream)
    }

    fn read_msg(self: Arc<Self>) {
        tokio::spawn(async move {
            let tag = self.tag.clone();

            loop{
                if let Some(msg) = &self.stream.write().await.as_mut().unwrap().next().await{
                    if let  Ok(msg) = msg  {
                        match msg {
                            Message::Text(msg) => {
                                if let Some(on_msg) = self.on_msg.read().await.as_ref() {
                                    on_msg(msg.to_string()).await;
                                }
                            }
                            Message::Close(msg) => println!("\n{tag} Close: {msg:#?}"),
                            _ => {}
                        };
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
            return;
            while let Some(msg) = &self.stream.write().await.as_mut().unwrap().next().await {
                println!("\n{tag} on_message",);
                match msg {
                    Ok(msg) => {
                        match msg {
                            Message::Text(msg) => {
                                if let Some(on_msg) = self.on_msg.read().await.as_ref() {
                                    on_msg(msg.to_string()).await;
                                }
                            }
                            Message::Close(msg) => println!("\n{tag} Close: {msg:#?}"),
                            _ => {}
                        };
                    }
                    Err(err) => {
                        println!("\n{tag} ERROR: {err:?}");
                    }
                };
            }

            // DISCONNECTED
            log!("{} DISCONNECTED", self.tag);

            for i in 1..=self.max_reconnects {
                log!("{} RECONNECT ATTEMPT #{i}...", tag);

                if self.clone().connect().await {
                    break;
                } else {
                    tokio::time::sleep(tokio::time::Duration::from_secs(self.reconnect_interval))
                        .await;
                }
            }
        });
    }

    pub async fn send(&self, msg: String) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n[send] {:?}", self.stream);
        if let Some(s) = self.stream.write().await.as_mut() {
            match s.send(Message::Text(msg.into())).await{
                Ok(_) => println!("\n[sent]"),
                Err(err) => {println!("Failed to send. {err:?}"); return Err(err.into());}
            }
        } else {
            return Err("Socket has no stream".into());
        };
        Ok(())
    }
}

pub async fn main(url: &str) {
    log!("STARTING WS CLIENT ON {url}...");
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install provider");
}
