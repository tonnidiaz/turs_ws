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

use std::{borrow::Cow, pin::Pin, sync::Arc};

use futures::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex, RwLock},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, protocol::Message},
};
use tungstenite::{Bytes, client::IntoClientRequest};
use turs::{elog, log, sleep, Fut, Res};

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
    url: Cow<'static, str>,
    tag: Cow<'static, str>,
    reconnect_interval: u64,
    max_reconnects: u64,
}

#[derive(Clone)]
pub struct Ws {
    inner: Arc<WsInner>
}

impl Ws {
    /// Just creates a new Ws instance.
    ///
    /// Call self.connect() after.
    pub async fn new(url: &str, tag: &str) -> Self {

        let ws = Self {
          
            inner: WsInner {
                url: Cow::Owned(url.into()),
                tag: Cow::Owned(tag.into()),
                reconnect_interval: 5,
                max_reconnects: 5,
                ..Default::default()
            }
            .into(),
        };
        ws
    }

    pub fn tag(&self) -> Cow<'static, str>{
        self.inner.tag.clone()
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

impl Ws{
    async fn connection_loop(self, mut wrt_rx: mpsc::Receiver<Message>) {
        let tag = self.tag();
        let mut reconnects = 0;

        loop {
            log!("{} connecting to {}...", tag, self.inner.url);
            let (mut ws_stream, _) = match connect_async(self.inner.url.as_ref()).await {
                Ok(stream) => {
                    log!("{} connected!", tag);
                    reconnects = 0;
                    stream
                }
                Err(err) => {
                    elog!("{} FAILED to connect: {}", tag, err);
                    reconnects += 1;
                    if reconnects > self.inner.max_reconnects {
                        elog!("{} MAX RECONNECTS REACHED. Giving up.", tag);
                        break;
                    }
                    turs::sleep(self.inner.reconnect_interval).await;
                    continue;
                }
            };

             // If a connection is successful, run the message loops
             let (mut wrt, mut rdr) = ws_stream.split();
            
             // Spawn one task to handle all outgoing messages
             let mut send_task = tokio::spawn(async move {
                 while let Some(msg) = wrt_rx.recv().await {
                     if let Err(e) = wrt.send(msg).await {
                         elog!("{} FAILED to send message: {}", tag, e);
                         break; // Break the loop on a send failure
                     }
                 }
             });

             // The main task reads all incoming messages
            let mut read_task = tokio::spawn({
                let inner = self.inner.clone();
                let tag = tag.clone();
                async move {
                    while let Some(msg) = rdr.next().await {
                        if let Ok(msg) = msg {
                            match msg {
                                Message::Text(text) => {
                                    if let Some(on_msg) = inner.on_msg.read().await.as_ref() {
                                        on_msg(text).await;
                                    } else {
                                        elog!("{} no message readers", tag);
                                    }
                                }
                                Message::Binary(bin) => {
                                    if let Some(on_bin_msg) = inner.on_bin_msg.read().await.as_ref() {
                                        on_bin_msg(bin).await;
                                    }
                                }
                                Message::Close(msg) => {
                                    elog!("{} Close: {msg:#?}", tag);
                                    break;
                                }
                                _ => {} // Ignore other message types for simplicity
                            }
                        } else {
                            elog!("{} READ ERROR: {:?}", tag, msg.unwrap_err());
                            break; // Break the loop on a read failure
                        }
                    }
                }
            });

            tokio::select! {
                _ = (&mut read_task) => {
                    elog!("{} Read task finished, restarting connection...", tag);
                    // Abort the send task since the connection is dead
                    send_task.abort();
                }
                _ = (&mut send_task) => {
                     elog!("{} Write task finished, restarting connection...", tag);
                    // Abort the read task
                    read_task.abort();
                }
            }
 
        }


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
        let tag = &self.tag;
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
                    elog!("\n{tag} ERROR: {err:?}");
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
