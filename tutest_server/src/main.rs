use turs::{log, serde_json::json, sleep, tokio::{self}};
use turs_ws::server;

#[tokio::main] 
async fn main() {
    log!("Hello, world!");
    let _server = server::WsServer::new("localhost:5000");

    _server
        .init(|peer| {
            Box::pin(async move {
                let tag = "[ws:server]";
                log!("{tag} New client connection: {}", peer.id);

                peer.on_msg(move |msg| {
                    Box::pin({
                        let tag = format!("{tag}[on_msg]");
                        async move { log!("{tag} {msg:?}") }
                    })
                })
                .await;

            // spawn 6 tasks to simulate each exchange
            for i in 1..=6{
                let peer = peer.clone();
                tokio::spawn(async move{
                    loop {
                      // send 1000 msg in parallel
                    for j in 0..1000{
                        // let peer = peer.clone();
                        let tag = format!("[{i}__{j}]");
                        if let Err(err) = peer.send(&json!({"ev": format!("{tag} test conc"), "data": "testing rust concurrency"}).to_string()).await{
                            log!("{tag} failed to send msg to client. {err:?}");
                        };
                    }  
                      sleep(100).await;
                    }
                    
                });
            }
            })
        })
        .await
        .expect("Failed to init ws server.");
}
