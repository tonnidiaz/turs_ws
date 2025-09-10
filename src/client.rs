

use std::{borrow::Cow, pin::Pin, sync::Arc};

use futures::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use tokio::{
    net::TcpStream,
    sync::{RwLock, mpsc},
};
use tokio_tungstenite::{
    connect_async, tungstenite::{self, protocol::Message, Utf8Bytes}, MaybeTlsStream, WebSocketStream
};
use tungstenite::Bytes;
use turs::{Fut, Res, elog, log, sleep};

pub type TBytes = Bytes;
pub type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type TuStream = (SplitSink<Stream, Message>, SplitStream<Stream>);
pub type OnConnectCb =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>;
// pub type ConnectFn<F> = RwLock<Option<Arc<F>>>;
struct WsInner {
    wrt_tx: mpsc::Sender<Message>,
    on_msg: RwLock<Option<Box<dyn Fn(Utf8Bytes) -> Fut<()> + Send + Sync + 'static>>>,
    on_bin_msg: RwLock<Option<Box<dyn Fn(Bytes) -> Fut<()> + Send + Sync + 'static>>>,
    on_connect: RwLock<Option<OnConnectCb>>,
    url: Cow<'static, str>,
    tag: Cow<'static, str>,
    reconnect_interval: u64,
    max_reconnects: u64,
}

#[derive(Clone)]
pub struct Ws {
    inner: Arc<WsInner>,
}

impl Ws {
    /// Just creates a new Ws instance.
    ///
    pub async fn new(url: &str, tag: &str) -> Self {
        let (wrt_tx, wrt_rx) = mpsc::channel(100);
        let ws = Self {
            inner: WsInner {
                url: Cow::Owned(url.into()),
                tag: Cow::Owned(tag.into()),
                reconnect_interval: 5,
                max_reconnects: 5,
                wrt_tx,
                on_msg: RwLock::new(None),
                on_bin_msg: RwLock::new(None),
                on_connect: RwLock::new(None),
            }
            .into(),
        };

        tokio::spawn(ws.clone().connection_loop(wrt_rx));
        ws
    }

    pub fn tag(&self) -> Cow<'static, str> {
        self.inner.tag.clone()
    }

    /// sets the on_msg callback
    pub async fn on_msg<F>(&self, f: F)
    where
        F: Fn(Utf8Bytes) -> Fut<()> + Sync + Send + 'static,
    {
        self.inner.on_msg.write().await.replace(Box::new(f));
    }

    pub async fn send(&self, msg: &str) -> Res<()> {
        // println!("\n[send] {:?}", self.wrt);
        self.inner.wrt_tx.send(Message::Text(msg.into())).await?;
        Ok(())
    }
}

impl Ws {
    async fn connection_loop(self, mut wrt_rx: mpsc::Receiver<Message>) {
        let mut reconnects = 0;
        let tag = self.tag();
        loop {
            log!("{} connecting to {}...", tag, self.inner.url);
            let (ws_stream, _) = match connect_async(self.inner.url.as_ref()).await {
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
                    sleep(self.inner.reconnect_interval).await;
                    continue;
                }
            };

            // If a connection is successful, run the message loops
            let (mut wrt, mut rdr) = ws_stream.split();

            loop {
                tokio::select! {
                    msg = rdr.next() => {
                        if let Some(msg) = msg {
                            // Match and handle the message
                            if let Ok(msg) = msg {
                                match msg {
                                    Message::Text(text) => {
                                        // Process text message
                                        if let Some(on_msg) = self.inner.on_msg.read().await.as_ref() {
                                            on_msg(text).await;
                                        }
                                    }
                                    Message::Binary(bin) => {
                                        // Process binary message
                                        if let Some(on_bin_msg) = self.inner.on_bin_msg.read().await.as_ref() {
                                            on_bin_msg(bin).await;
                                        }
                                    }
                                    Message::Close(msg) => {
                                        elog!("{} Close: {msg:#?}", tag);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            // Stream has closed, break out of this inner loop to reconnect
                            elog!("{} Stream closed, reconnecting...", tag);
                            break;
                        }
                    },
                    // Outgoing messages from the MPSC channel
                    msg = wrt_rx.recv() => {
                        if let Some(msg) = msg {
                            if let Err(e) = wrt.send(msg).await {
                                elog!("{} FAILED to send message: {}", tag, e);
                                break; // Break the inner loop on a send failure
                            }
                        } else {
                            // The channel is closed, break
                            break;
                        }
                    }
                }
            }
        }
    }
}
