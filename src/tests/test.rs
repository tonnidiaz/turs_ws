use turs::Fut;

type OnGrowCb = Box<dyn Fn(i64) -> Fut + Send + Sync + 'static>;
pub struct Person {
    pub age: i64,
    _on_grow: Option<OnGrowCb>,
}
impl Person {
    pub fn on_grow<Cb>(&mut self, cb: Cb)
    where
        Cb: Fn(i64) -> Fut + Send + Sync + 'static,
    {
        self._on_grow.replace(Box::new(cb));
    }

    pub fn new() -> Self {
        let p = Self {
            age: 20,
            _on_grow: None,
        };
        p
    }

    pub async fn start_growing(&mut self, to: usize) {
        loop {
            self.age += 1;
            if let Some(ref on_grow) = self._on_grow {
                on_grow(self.age).await;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }
}


pub async fn main(){
    let mut p = Person::new();
    p.on_grow(|x| {
        Box::pin(async move{
            println!("Person has grown to: {x}");
        })
    });
    p.start_growing(50).await;
}