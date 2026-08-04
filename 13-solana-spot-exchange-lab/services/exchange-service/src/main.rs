use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    exchange_service::init_tracing();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "exchange service listening");
    axum::serve(listener, exchange_service::app_from_env().await?).await?;
    Ok(())
}
