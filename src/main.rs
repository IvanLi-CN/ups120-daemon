#[tokio::main]
async fn main() {
    println!("Hello, ups120-daemon!");
    // 可以在这里添加更多的异步逻辑
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    println!("Daemon finished.");
}
