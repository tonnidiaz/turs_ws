use turs::{
    chrono::{self},
    log,
    serde_json::json,
    sleep,
    tokio::{self},
};
use turs_ws::client;
#[tokio::main]
async fn main() {
    let l = chrono::Local::now();
    log!("Hello, world! {:?}", l.to_rfc2822());
    let _ws = client::Ws::new("ws://localhost:5000", "[my_ws]").await;
    let tag = _ws.tag.clone();
    _ws.clone()
        .on_msg(move |msg| {
            let tag = tag.clone();
            Box::pin(async move {
                log!("{} [on_msg] {msg}", tag);
            })
        })
        .await;
    tokio::spawn({
        let _ws = _ws.clone();
        let mut i = 0;
        async move {
            loop {
                i += 1;
                log!("[{i}] {} sending msg...", _ws.tag);
                if let Err(err) = _ws
                    .clone()
                    .send(
                        &json!({"ev": "sub", "data": {"pair": "SOL-USDT", "plat": "binance"}})
                            .to_string(),
                    )
                    .await
                {
                    log!("[{i}] {} Failed to send msg. {err:?}", _ws.tag);
                } else{
                    log!("[{i}] msg sent!");
                };
                sleep(1000).await;
            }
        }
    });
    sleep(1500).await;
    _ws.connect().await.expect("Failed to init ws client.");
}
