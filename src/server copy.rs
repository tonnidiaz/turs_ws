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

use std::{collections::HashMap, io::Error as IoError, net::SocketAddr, pin::Pin, sync::Arc};

use futures::stream::{SplitSink, SplitStream};
use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::{StreamExt, future, pin_mut, stream::TryStreamExt};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock},
};
use tokio_tungstenite::{WebSocketStream, tungstenite::protocol::Message};
use turs::funcs::ts;

type Stream = WebSocketStream<TcpStream>;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Msg {
    pub ev: String,
    pub data: serde_json::Value,
}

pub struct Client {
    pub rdr: SplitStream<Stream>,
    pub wrt: SplitSink<Stream, Message>,
}

impl Client {}
pub struct WsServer {
    pub url: String,
    pub clients: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<Client>>>>>,
    pub on_msg: RwLock<
        Option<
            Box<dyn Fn(Msg) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>,
        >,
    >,
}

impl WsServer {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            clients: Arc::new(Mutex::new(HashMap::new())),
            on_msg: RwLock::new(None),
        }
    }

    pub async fn connect(self: Arc<Self>) -> bool {
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

        // Let's spawn the handling of each connection in a separate task.
        while let Ok((stream, addr)) = listener.accept().await {
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
                                        if let Ok(msg) = serde_json::from_str(&msg){
                                            on_msg(msg).await;
                                        }
                                        
                                    }else{
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
