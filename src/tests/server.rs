use std::thread;
use serde_json::json;
use tokio::time;
use turs::funcs::ts;

use crate::server;

pub async fn main() {
    let port = 5000;
    let url = &format!("127.0.0.1:{port}");

    let mut server = server::WsServer::new(url);
    if !server.init().await {
        return;
    }

    // on_connect
    while let Some(client) = server.connect.recv().await {
        let id = format!("client_{}", client.addr.port());
        println!("\n[{}] {id} connected!", ts());
        // let client = client.clone();
        client.clone()
            .on_msg(move |msg| {
                let id = id.clone();
                let client = client.clone();
                Box::pin(async move {
                    println!("\n[{}] [{id}] on_message:\n{msg:?}", ts());
                    time::sleep(time::Duration::from_millis(1000)).await;
                    client.broadcast(&json!({
                        "ev": format!("message from {id}"),
                        "data": msg
                    }).to_string()).await;

                })
            })
            .await;
    }

    thread::park();
}
