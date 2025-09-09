
use turs::{chrono::{self, Local}, log, tokio::{self, time}};
#[tokio::main]
async fn main() {
    let l = chrono::Local::now();
    log!("Hello, world! {:?}", l.to_rfc2822());
    let mut m = MWs::new("url").await;
    log!("m: {:?}", m.url);
    m.set_change_url(|old| format!("{old}_{}", Local::now().timestamp()));
    for i in 1..=10{
        time::sleep(time::Duration::from_millis(1000)).await;
        m.call_change_url();
        log!("[{i}] {}", m.url);
    }
}

struct MWs{
    url: String,
    cb: Option<Box<dyn FnMut(&str) -> String>>
}
 
impl MWs
{

    async fn new(url: &str) -> Self{
      Self{url: url.to_string(), cb: None}
    }

    fn set_change_url<F>(&mut self, f: F)
    where F: FnMut(&str) -> String + 'static
    {
        self.cb.replace(Box::new(f));
    }

    fn call_change_url(&mut self){
        if let Some(cb) = &mut self.cb{
            self.url = cb(&self.url);
        }
    }

}
