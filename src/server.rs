
use std::{borrow::Cow, collections::HashMap, error, net::SocketAddr, sync::Arc};

use futures::SinkExt;
use futures_util::StreamExt;

use tokio::{ 
    net::TcpListener,
    sync::{mpsc, Mutex, RwLock},
};
use tokio_tungstenite::tungstenite::protocol::Message;
use turs::{Fut, Res, elog, log};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Msg {
    pub ev: String,
    pub data: serde_json::Value,
}



type Peers = Arc<RwLock<HashMap<SocketAddr, Peer>>>;
pub struct WsServer {
    pub url: Cow<'static, str>,
    peers: Peers,
}

struct PeerInner {
    on_msg: Mutex<Option<Box<dyn Fn(Msg) -> Fut<()> + Send + Sync>>>,
    wrt_tx: mpsc::Sender<Message>
}
#[derive(Clone)]
pub struct Peer {
    pub id: Cow<'static, str>,
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
            .wrt_tx
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
            url: Cow::Owned(url.to_string()),
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn init<F>(&self, on_connect: F) -> Res<()>
    where
        F: Fn(Peer) -> Fut<()> + Send + Sync + 'static,
    {

        let listener = TcpListener::bind(self.url.as_ref()).await?;

        log!("Listening on: {}", self.url);
        let clients = &self.peers;
        let on_connect = Arc::new(on_connect);
        // tokio::spawn(async move {
        while let Ok((stream, addr)) = listener.accept().await {
            let clients = clients.clone();
            let on_connect = on_connect.clone();

            tokio::spawn(async move {
                let ws_stream = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("\nError during the websocket handshake occurred");
                let (mut wrt, mut rdr) = ws_stream.split();
                let (wrt_tx, mut wrt_rcv) = mpsc::channel(100);

               
                // server.onconnect()
                let peer = Peer {
                    id: Cow::Owned(format!("peer__{}", addr.port())),
                    addr: addr.clone(),
                    inner: Arc::new(PeerInner {
                        on_msg: Mutex::new(None),
                        wrt_tx,
                    }),
                    peers: clients.clone()
                };
                 // spawn msg listener
                tokio::spawn({
                    let clients = clients.clone();
                    let peer = peer.clone();
                    async move{
                    // from peer.send()
                    while let Some(msg) = wrt_rcv.recv().await{
                        // forward to client
                        if let Err(err) = wrt.send(msg).await{
                            elog!("Failed to send msg to client {}. {err:?}", peer.id);
                            // remove from peer list
                            clients.write().await.remove(&addr);
                            break;
                        };
                    }
                }});

                clients.write().await.insert(addr.clone(), peer.clone());
                tokio::spawn({
                    let peer = peer.clone();
                    async move {
                        on_connect(peer).await;
                    }
                });

                while let Some(msg) = rdr.next().await {

                    if let Ok(msg) = msg {
                            match msg {
                                Message::Text(text) => {
                                    if let Ok(msg) = serde_json::from_str(&text) {
                                        let inner = peer.inner.clone();
                                        tokio::spawn(async move {
                                            let binding = inner.on_msg.lock().await;
                                            if let Some(cb) = binding.as_ref(){
                                                cb(msg).await;
                                            };
                                            
                                        });
                                    } else {
                                        elog!("invalid message type (expected {{ ev: string, data: any }}), got: {text:#?}");
                                    }
                                }
                                Message::Close(msg) => elog!("Close: {msg:#?}"),
                                _ => {
                                    log!("OTHER MSG TYPE:\n {msg:?}");
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
