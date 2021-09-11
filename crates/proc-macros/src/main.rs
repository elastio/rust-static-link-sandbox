use async_trait::async_trait;

#[async_trait]
trait TestTrait {
    async fn foo(&self) -> usize;
}

struct MyStruct;

#[async_trait]
impl TestTrait for MyStruct {
    async fn foo(&self) -> usize { 42 }
}

#[tokio::main]
async fn main() {
    let ms = MyStruct;

    println!("Hello, world: {}", ms.foo().await);
}
