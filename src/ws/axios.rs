use ruxios::prelude::*;

fn main(){
    let api = Ruxios::from(RuxiosConfig {
    base_url: String::from("https://api.mysite.com"),
    ..Default::default()
});

let res = api.get::<Value, Value>("/my-route").await;

match res {
    Ok(res) => println!("{:?}", res.data),
    Err(err) => println!("{:?}", err),
}
}
