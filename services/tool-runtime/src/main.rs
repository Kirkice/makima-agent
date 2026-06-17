use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use makima_tool_runtime::{
    proto::{
        shell_service_server::ShellServiceServer,
        file_service_server::FileServiceServer,
        http_service_server::HttpServiceServer,
        document_service_server::DocumentServiceServer,
        sandbox_service_server::SandboxServiceServer,
        checkpoint_service_server::CheckpointServiceServer,
        file_tracker_service_server::FileTrackerServiceServer,
        token_counter_service_server::TokenCounterServiceServer,
    },
    server::{
        ShellServiceImpl,
        FileServiceImpl,
        HttpServiceImpl,
        DocumentServiceImpl,
        SandboxServiceImpl,
        CheckpointServiceImpl,
        FileTrackerServiceImpl,
        TokenCounterServiceImpl,
    },
    checkpoint::CheckpointManager,
    tracker::FileTrackerManager,
    tokens::TokenCounter,
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

    // Create shared managers for stateful services
    let checkpoint_manager = Arc::new(CheckpointManager::new());
    let file_tracker_manager = Arc::new(FileTrackerManager::new());
    let token_counter = Arc::new(TokenCounter::new());

    Server::builder()
        .add_service(ShellServiceServer::new(ShellServiceImpl::new()))
        .add_service(FileServiceServer::new(FileServiceImpl::new()))
        .add_service(HttpServiceServer::new(HttpServiceImpl::new()))
        .add_service(DocumentServiceServer::new(DocumentServiceImpl::new()))
        .add_service(SandboxServiceServer::new(SandboxServiceImpl::new()))
        .add_service(CheckpointServiceServer::new(CheckpointServiceImpl::new(checkpoint_manager)))
        .add_service(FileTrackerServiceServer::new(FileTrackerServiceImpl::new(file_tracker_manager)))
        .add_service(TokenCounterServiceServer::new(TokenCounterServiceImpl::new(token_counter)))
        .serve(addr)
        .await?;

    Ok(())
}
