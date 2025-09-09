pub mod client;
pub mod server;
// mod tests;

use tokio::time::Instant;

#[tokio::main]
async fn main() {
    println!("\nHello from turs_ws");
    let t = Instant::now();
    // tests::server::main().await;
    // tests::all::main().await;

    println!("\nDONE IN {:.2?}", t.elapsed());
}
