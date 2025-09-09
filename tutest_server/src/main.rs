use turs::{log, tokio};
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
            })
        })
        .await
        .expect("Failed to init ws server.");
}
