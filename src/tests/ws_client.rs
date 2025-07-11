use std::sync::Arc;

use serde_json::{json, Value, Map};

use crate::{log, ws};

pub async fn main() {
    /* log!("Hello, world!");

    let url = "ws://localhost:5000/ws/okx";
    // ws::client::main(url).await;
    let ws = ws::client::Ws::new(url, "[TWS]")
        .await;    /* *ws.on_connect.write().await = Some(Arc::new(async {

        // send test msg
        /* ws.clone()
             */
    })); */
    let ws = Arc::new(ws);
    let ws_clone = Arc::clone(&ws);
    *ws_clone.on_connect.write().await = Some(Box::new(move || {
        let ws = Arc::clone(&ws);
        Box::pin(async move {
            println!("Sending test msg...{:?}", ws.tag);
            let test_msg = json!({
                "ev": "sub",
                "data": {"plat":  "okx","symb":   "SOL-USDT"},
            })
            .to_string();

            ws.send(test_msg)
                .await
                .unwrap_or_else(|err| println!("Failed to send msg. {:?}", err ));
        })
    }));

    // let ws_clone = Arc::clone(&ws_clone);
    if !ws_clone.clone().connect().await {
        return;
    };
    let ws = Arc::clone(&ws_clone);
    let tag = ws.tag.clone();
    *ws.clone().on_msg.write().await = Some(Box::new( move |msg| {
        log!("{} on_msg", tag);
        Box::pin( async {
            on_msg(msg);
        })
    }));

    std::thread::park(); */
}

fn on_msg(msg: String) {
    let json_msg: Option<Map<String, Value>> = serde_json::from_str(&msg).unwrap_or_else(|_| None);
    if let Some(msg) = json_msg {
        println!("\nJSON:\n{msg:?}");
    } else {
        println!("\nSTR:\n{msg}");
    }
}
