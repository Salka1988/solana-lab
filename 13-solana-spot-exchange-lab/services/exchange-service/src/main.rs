use std::net::SocketAddr;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, exchange_service::app()).await
}
