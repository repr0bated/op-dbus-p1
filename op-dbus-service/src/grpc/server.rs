//! gRPC server setup for op-dbus-service.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use op_cache::grpc::{AgentServiceImpl, CacheServiceImpl, OrchestratorServiceImpl};
use op_cache::proto::{
    agent_service_server::AgentServiceServer,
    cache_service_server::CacheServiceServer,
    mcp_service_server::McpServiceServer,
    orchestrator_service_server::OrchestratorServiceServer,
};
use op_tools::ToolRegistry;
use tonic::transport::Server;
use tracing::info;

use super::mcp_service::McpServiceImpl;

pub async fn start_grpc_server(
    addr: SocketAddr,
    registry: Arc<ToolRegistry>,
) -> Result<()> {
    let agent_service = Arc::new(AgentServiceImpl::new());
    let cache_service = Arc::new(CacheServiceImpl::new());
    let orchestrator_service = Arc::new(OrchestratorServiceImpl::new(
        agent_service.clone(),
        cache_service.clone(),
    ));
    let mcp_service = McpServiceImpl::new(registry);

    info!("Starting gRPC server on {}", addr);

    Server::builder()
        .add_service(AgentServiceServer::from_arc(agent_service))
        .add_service(CacheServiceServer::from_arc(cache_service))
        .add_service(OrchestratorServiceServer::from_arc(orchestrator_service))
        .add_service(McpServiceServer::new(mcp_service))
        .serve(addr)
        .await?;

    Ok(())
}
