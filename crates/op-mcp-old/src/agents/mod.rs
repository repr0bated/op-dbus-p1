//! MCP Agents - Specialized programming and operations agents

pub mod c_pro;
pub mod cpp_pro;
pub mod executor;
pub mod file;
pub mod golang_pro;
pub mod javascript_pro;
pub mod monitor;
pub mod network;
pub mod packagekit;
pub mod php_pro;
pub mod python_pro;
pub mod rust_pro;
pub mod sql_pro;
pub mod systemd;

// Re-export for convenience
pub use executor::ExecutorAgent;
pub use file::FileAgent;
pub use monitor::MonitorAgent;
pub use network::NetworkAgent;
pub use packagekit::PackageKitAgent;
pub use systemd::SystemdAgent;
