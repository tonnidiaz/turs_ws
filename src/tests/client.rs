use std::{env::args, sync::Arc, thread};

use chrono::Local;
use serde_json::json;
use turs::funcs::ts;

use crate::client;

pub async fn main() {
    let port = 5000;
    let max: usize = args()
        .nth(1)
        .unwrap_or_else(|| String::from("100"))
        .parse()
        .unwrap();
    let url = &format!("ws://127.0.0.1:{port}");

    println!("\n[{}] max: {max}, url: {url}", ts());

    let client = client::Ws::new(url, "test").await;
    let client = Arc::new(client);
    if !client.clone().connect().await {
        return;
    }
    for i in 1..=max {
        println!("\n[{}] [{i}] sending...", ts());
        client
            .send(
                &json!({
                    "ev": "sub", "data": {"pair": "SOL-USDT", "ts": Local::now().timestamp_millis()}
                })
                .to_string(),
            )
            .await
            .expect(&format!("[{i}] Failed to send msg."));
    }

    thread::park();
}
