use std::net::SocketAddr;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use makima_tool_runtime::{
    proto::{
        shell_service_server::ShellServiceServer,
        file_service_server::FileServiceServer,
        http_service_server::HttpServiceServer,
        document_service_server::DocumentServiceServer,
        sandbox_service_server::SandboxServiceServer,
    },
    server::{
        ShellServiceImpl,
        FileServiceImpl,
        HttpServiceImpl,
        DocumentServiceImpl,
        SandboxServiceImpl,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr: SocketAddr = "[::1]:50051".parse()?;

    tracing::info!("Starting Tool Runtime Service on {}", addr);

    Server::builder()
        .add_service(ShellServiceServer::new(ShellServiceImpl::new()))
        .add_service(FileServiceServer::new(FileServiceImpl::new()))
        .add_service(HttpServiceServer::new(HttpServiceImpl::new()))
        .add_service(DocumentServiceServer::new(DocumentServiceImpl::new()))
        .add_service(SandboxServiceServer::new(SandboxServiceImpl::new()))
        .serve(addr)
        .await?;

    Ok(())
}