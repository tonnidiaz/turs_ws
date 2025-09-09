//! A chat server that broadcasts a message to all connections.
//!
//! This is a simple line-based server which accepts WebSocket connections,
//! reads lines from those connections, and broadcasts the lines to all other
//! connected clients.
//!
//! You can test this out by running:
//!
//!     cargo run --example server 127.0.0.1:12345
//!
//! And then in another window run:
//!
//!     cargo run --example client ws://127.0.0.1:12345/
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.

use std::{collections::HashMap, error, net::SocketAddr, sync::Arc};

use futures::{
    SinkExt,
    stream::SplitSink,
};
use futures_util::StreamExt;

use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock},
};
use tokio_tungstenite::{WebSocketStream, tungstenite::protocol::Message};
use turs::{Fut, Res, elog, log};

type Stream = WebSocketStream<TcpStream>;
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Msg {
    pub ev: String,
    pub data: serde_json::Value,
}



type Peers = Arc<RwLock<HashMap<SocketAddr, Peer>>>;
pub struct WsServer {
    pub url: String,
    peers: Peers,
}

struct PeerInner {
    on_msg: Mutex<Option<Box<dyn Fn(Msg) -> Fut<()> + Send + Sync>>>,
    wrt: Mutex<SplitSink<Stream, Message>>,
}
#[derive(Clone)]
pub struct Peer {
    pub id: String,
    pub addr: SocketAddr,
    inner: Arc<PeerInner>,
    peers: Peers,
}

impl Peer {
    /// sets on on_msg callback
    pub async fn on_msg<F>(&self, f: F)
    where
        F: Fn(Msg) -> Fut<()> + Send + Sync + 'static,
    {
        self.inner.on_msg.lock().await.replace(Box::new(f));
    }
    pub async fn send(&self, msg: &str) -> Result<(), Box<dyn error::Error>> {
        self.inner
            .wrt
            .lock()
            .await
            .send(Message::Text(msg.into()))
            .await?;
        Ok(())
    }
    pub async fn broadcast(&self, msg: &str) {
        for (addr, peer) in self.peers.write().await.iter() {
            if addr != &self.addr {
                if let Err(err) = peer.send(msg).await {
                    elog!("Failed to broadcast message to peer: {}.\n{err:?}", peer.id);
                };
            }
        }
    }
}
impl WsServer {
    /// The call self.init()
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn init<F>(&self, on_connect: F) -> Res<()>
    where
        F: Fn(Peer) -> Fut<()> + Send + Sync + 'static,
    {
        let addr = &self.url.clone();

        let listener = TcpListener::bind(addr).await?;

        println!("Listening on: {}", addr);
        let clients = self.peers.clone();
        let on_connect = Arc::new(on_connect);
        // tokio::spawn(async move {
        while let Ok((stream, addr)) = listener.accept().await {
            let clients = clients.clone();
            let on_connect = on_connect.clone();

            tokio::spawn(async move {
                let ws_stream = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("\nError during the websocket handshake occurred");
                let (wrt, mut rdr) = ws_stream.split();

                // server.onconnect()
                let peer = Peer {
                    id: format!("peer__{}", addr.port()),
                    addr: addr.clone(),
                    inner: Arc::new(PeerInner {
                        on_msg: Mutex::new(None),
                        wrt: Mutex::new(wrt),
                    }),
                    peers: clients.clone()
                };

                clients.write().await.insert(addr.clone(), peer.clone());
                tokio::spawn({
                    let peer = peer.clone();
                    async move {
                        on_connect(peer).await;
                    }
                });

                while let Some(msg) = rdr.next().await {
                    // turs::log!("{addr:?} on_message:\n{msg:?}");

                    if let Ok(msg) = msg {
                        let has_msg_listener = peer.inner.on_msg.lock().await.is_some();
                        if has_msg_listener {
                            match msg {
                                Message::Text(text) => {
                                    let msg = text.clone().to_string();
                                    if let Ok(msg) = serde_json::from_str(&msg) {
                                        let inner = peer.inner.clone();
                                        tokio::spawn(async move {
                                            let binding = inner.on_msg.lock().await;
                                            let cb = binding.as_ref().unwrap();
                                            cb(msg).await
                                        });
                                    } else {
                                        elog!("invalid message type (expected {{ ev: string, data: any }}), got: {msg:#?}");
                                    }
                                }
                                Message::Close(msg) => elog!("Close: {msg:#?}"),
                                _ => {
                                    log!("OTHER MSG TYPE:\n {msg:?}");
                                }
                            }
                        }
                    }
                }

                turs::elog!("Client_{} disconnected!", addr.port());
                clients.write().await.remove(&addr);
            });
        }
        clients.write().await.clear();
        // });
        Ok(())
    }
}
