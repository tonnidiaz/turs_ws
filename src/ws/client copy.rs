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

use futures::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tungstenite::client::IntoClientRequest;

use crate::{ValueTr, log};

pub async fn main(url: &str) {
    log!("STARTING WS CLIENT ON {url}...");
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install provider");
    let url = url.into_client_request().unwrap();

    // let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
    let (mut ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");
    ws_stream
        .send(Message::Text(
            json!({
                "ev": "sub",
                "data":
                    {
                        "plat":  "okx",
                        "symb":   "SOL-USDT"
                },
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("failed to send test msg");

    while let Some(msg) = ws_stream.next().await {
        let tag = "[WS] message:";
        match msg {
            Ok(msg) => {
                match msg {
                    Message::Text(msg) => {
                        let json_msg: Map<String, Value> = serde_json::from_str(&msg.to_string()).expect("Failed to conv msg to json");
                        println!("\n{tag} Text: {:?}", json_msg)
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
}
