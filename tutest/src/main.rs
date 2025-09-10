use std::thread::park;

use turs::{
    log,
    tokio::{self},
};
use turs_ws::client;
#[tokio::main]
async fn main() {
    log!("Hello, world!");
    let _ws = client::Ws::new("ws://localhost:5000", "[my_ws]").await;
    _ws.clone()
        .on_msg({
            let _ws = _ws.clone();
            move |msg| {
                let _ws = _ws.clone();
                Box::pin(async move {
                    log!("{} [on_msg] {msg}", _ws.tag());
                })
            }
        }) 
        .await;
    park();
    /* tokio::spawn({
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
    sleep(1500).await; */
    // _ws.connect().await.expect("Failed to init ws client.");
}
