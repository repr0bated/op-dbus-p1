//! gRPC services for op-dbus-service.

pub mod mcp_service;
pub mod server;

pub use server::start_grpc_server;
