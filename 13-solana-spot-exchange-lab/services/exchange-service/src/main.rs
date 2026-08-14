#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    exchange_service::init_tracing();

    let addr = exchange_service::http_addr_from_env()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "exchange service listening");
    axum::serve(listener, exchange_service::app_from_env().await?).await?;
    Ok(())
}
