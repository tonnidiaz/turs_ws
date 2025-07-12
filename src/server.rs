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

use std::{
    collections::HashMap, error, io::Error as IoError, net::SocketAddr, pin::Pin, sync::Arc,
};

use futures::{
    SinkExt,
    stream::{SplitSink, SplitStream},
};
use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::StreamExt;

use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc},
};
use tokio_tungstenite::{WebSocketStream, tungstenite::protocol::Message};
use turs::{Fut, funcs::ts};

type Stream = WebSocketStream<TcpStream>;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Msg {
    pub ev: String,
    pub data: serde_json::Value,
}

pub struct TuStream {
    pub rdr: Arc<Mutex<SplitStream<Stream>>>,
    pub wrt: Arc<Mutex<SplitSink<Stream, Message>>>,
}

struct ClientStream(Arc<Mutex<TuStream>>);

type MStream = Arc<Option<TuStream>>;
type OnConnectCb =
    Box<dyn Fn(MStream) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>;

    type Clients = Arc<Mutex<HashMap<SocketAddr, Arc<MClient>>>>;
pub struct WsServer {
    pub url: String,
    pub clients: Clients,
    pub connect: mpsc::Receiver<Arc<MClient>>,
    connect_tx: mpsc::Sender<Arc<MClient>>,
}

pub struct MClient {
    stream: TuStream,
    pub addr: SocketAddr,
    _on_msg: Mutex<Option<Box<dyn Fn(Msg) -> Fut + Send + Sync + 'static>>>,
    rx: Mutex<mpsc::Receiver<Msg>>,
    peers: Clients,
}

impl MClient {
    pub fn new(addr: SocketAddr, stream: TuStream, rx: mpsc::Receiver<Msg>, peers: Clients) -> Arc<Self> {
        let s = Self {
            addr,
            stream,
            rx: Mutex::new(rx),
            _on_msg: Mutex::new(None),
            peers,
            
        };

        let s = Arc::new(s);
        let s_c = s.clone();
        tokio::spawn(async move {
            while let Some(msg) = s_c.rx.lock().await.recv().await {
                let s = s_c.clone();
                tokio::spawn(async move {
                    if let Some(cb) = s._on_msg.lock().await.as_ref() {
                        cb(msg).await;
                    }
                });
            }
        });
        s
    }
    pub async fn on_msg<F>(&self, f: F)
    where
        F: Fn(Msg) -> Fut + Send + Sync + 'static,
    {
        self._on_msg.lock().await.replace(Box::new(f));
    }
    pub async fn send(&self, msg: &str) -> Result<(), Box<dyn error::Error>> {
        self.stream
            .wrt
            .lock()
            .await
            .send(Message::Text(msg.into()))
            .await?;
        Ok(())
    }

    pub async fn broadcast(&self, msg: &str) {
        let peers = self.peers.lock().await;
        let peers: Vec<_> = peers.values().collect();
        for peer in peers {
            if peer.addr != self.addr {
                if let Err(err) = peer.send(msg).await{
                    println!("\n[{}] failed to broadcast message to peer: {}.\n{err:?}", ts(), peer.addr.port());
                };
            }
        }
    }
}

impl WsServer {
    pub fn new(url: &str) -> Self {
        let (connect_tx, connect) = mpsc::channel(100);
        Self {
            url: url.to_string(),
            clients: Arc::new(Mutex::new(HashMap::new())),
            connect,
            connect_tx,
        }
    }

    pub async fn init(&self) -> bool {
        let addr = &self.url.clone();

        let try_socket = TcpListener::bind(addr).await;
        let listener = match try_socket {
            Ok(l) => l,
            Err(err) => {
                eprintln!("\nFailed to bind. {err:?}");
                return false;
            }
        };

        println!("Listening on: {}", addr);
        let clients = self.clients.clone();
        let connect_tx = self.connect_tx.clone();
        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let clients = clients.clone();
                let connect_tx = connect_tx.clone();

                tokio::spawn(async move {
                    let ws_stream = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("\nError during the websocket handshake occurred");
                    let (wrt, rdr) = ws_stream.split();

                    // server.onconnect()
                    let wrt = Arc::new(Mutex::new(wrt));
                    let rdr = Arc::new(Mutex::new(rdr));
                    println!("\nWebSocket connection established: {}", addr);

                    let (tx, rx) = mpsc::channel(100);
                    let tx_c = tx.clone();
                    let client = MClient::new(
                        addr,
                        TuStream {
                            wrt: wrt.clone(),
                            rdr: rdr.clone(),
                        },
                        rx, clients.clone()
                    );

                    // report
                    connect_tx
                        .send(client.clone())
                        .await
                        .expect("Failed to send client to connect tx.");

                    /* let client = Arc::new(Mutex::new(client));
                    let client_cl = client.clone(); */
                     clients.lock().await.insert(addr, client.clone());

                    while let Some(msg) = rdr.lock().await.next().await {
                        if let Ok(msg) = msg {
                            let ts = ts();
                            match msg {
                                Message::Text(text) => {
                                    let msg = text.clone().to_string();
                                    if let Ok(msg) = serde_json::from_str(&msg) {
                                        tx_c.send(msg)
                                            .await
                                            .expect("Failed to send message to client");
                                    } else {
                                        println!("\n[{ts}] invalid message type: {msg:#?}");
                                    }
                                }
                                Message::Close(msg) => println!("\n[{ts}] Close: {msg:#?}"),
                                _ => {
                                    println!("\n[{ts}] OTHER MSG TYPE:\n {msg:?}");
                                }
                            }
                        }
                    }
                });
            }
        });

        // Let's spawn the handling of each connection in a separate task.
        /*      while let Ok((stream, addr)) = listener.accept().await {
             let s = self.clone();
             tokio::spawn(async move {
                 let ws_stream = tokio_tungstenite::accept_async(stream)
                     .await
                     .expect("Error during the websocket handshake occurred");
                 let (wrt, rdr) = ws_stream.split();
                 println!("WebSocket connection established: {}", addr);
                 let client = Client { rdr, wrt };
                 let client = Arc::new(Mutex::new(client));
                 s.clients.lock().await.insert(addr, client.clone());

                 while let Some(msg) = client.lock().await.rdr.next().await {
                     if let Ok(msg) = msg {
                         let ts = ts();
                         match msg {
                             Message::Text(text) => {
                                 let s = s.clone();
                                 let msg = text.clone().to_string();
                                 tokio::spawn(async move {
                                     if let Some(on_msg) = s.on_msg.read().await.as_ref() {
                                         if let Ok(msg) = serde_json::from_str(&msg) {
                                             on_msg(msg).await;
                                         }
                                     } else {
                                         println!("\n[{ts}] invalid message type: {msg:#?}");
                                     }
                                 });
                             }
                             Message::Close(msg) => println!("\n[{ts}] Close: {msg:#?}"),
                             _ => {
                                 println!("\n[{ts}] OTHER MSG TYPE:\n {msg:?}");
                             }
                         }
                     }
                 }
             });
         }

        */
        true
    }
}

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    peer_map.lock().await.insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    /* let broadcast_incoming = incoming.try_for_each(|msg| {
        println!(
            "\nReceived a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );
        let peers = peer_map.lock().await;

        future::ok(())
    }); */

    /*  let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr); */
}

pub async fn main() -> Result<(), IoError> {
    let addr = "127.0.0.1:5000";

    let state = PeerMap::new(Mutex::new(HashMap::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }

    /*  loop {
        time::sleep(time::Duration::from_secs(1)).await;
    } */
    Ok(())
}
