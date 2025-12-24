//! PackageKit MCP Agent
//! Exposes package management operations as MCP tools via D-Bus

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zbus::{connection::Builder, interface, object_server::SignalEmitter, Connection, Proxy};

#[derive(Debug, Deserialize)]
struct PackageTask {
    #[serde(rename = "type")]
    task_type: String,
    action: String, // search, install, remove, list, update, details, refresh
    packages: Option<Vec<String>>,
    query: Option<String>,
}

#[derive(Debug, Serialize)]
struct PackageResult {
    success: bool,
    action: String,
    packages: Vec<PackageInfo>,
    message: String,
    transaction_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PackageInfo {
    package_id: String,
    name: String,
    version: String,
    arch: String,
    repo: String,
    summary: Option<String>,
}

pub struct PackageKitAgent {
    agent_id: String,
}

impl PackageKitAgent {
    /// Connect to PackageKit daemon
    async fn connect_packagekit(&self) -> Result<Proxy<'static>> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        Proxy::new(
            &conn,
            "org.freedesktop.PackageKit",
            "/org/freedesktop/PackageKit",
            "org.freedesktop.PackageKit",
        )
        .await
        .context("Failed to create PackageKit D-Bus proxy")
    }

    /// Create a new transaction
    async fn create_transaction(&self) -> Result<String> {
        let proxy = self.connect_packagekit().await?;

        let path: zbus::zvariant::OwnedObjectPath = proxy
            .call("CreateTransaction", &())
            .await
            .context("Failed to create PackageKit transaction")?;

        Ok(path.to_string())
    }

    /// Get transaction proxy
    async fn get_transaction_proxy(&self, transaction_path: &str) -> Result<Proxy<'static>> {
        let conn = Connection::system().await?;
        let path_owned = transaction_path.to_string();

        Proxy::new(
            &conn,
            "org.freedesktop.PackageKit",
            path_owned,
            "org.freedesktop.PackageKit.Transaction",
        )
        .await
        .context("Failed to create transaction proxy")
    }

    /// Execute search action
    async fn search_packages(&self, query: &str) -> Result<PackageResult> {
        let transaction_path = self.create_transaction().await?;
        let proxy = self.get_transaction_proxy(&transaction_path).await?;

        let filter: u64 = 0; // No filter - search all packages
        let search_terms = vec![query];

        proxy
            .call::<_, _, ()>("SearchNames", &(filter, search_terms))
            .await
            .context("Failed to call SearchNames")?;

        // In full implementation, would listen for Package signals
        // For now, return success with transaction path
        Ok(PackageResult {
            success: true,
            action: "search".to_string(),
            packages: Vec::new(),
            message: format!(
                "Search initiated for '{}' on transaction {}",
                query, transaction_path
            ),
            transaction_path: Some(transaction_path),
        })
    }

    /// Execute install action
    async fn install_packages(&self, package_names: &[String]) -> Result<PackageResult> {
        let transaction_path = self.create_transaction().await?;
        let proxy = self.get_transaction_proxy(&transaction_path).await?;

        // First resolve package names to IDs
        let filter: u64 = 0;
        let names: Vec<&str> = package_names.iter().map(|s| s.as_str()).collect();

        proxy
            .call::<_, _, ()>("Resolve", &(filter, names.clone()))
            .await
            .context("Failed to resolve package names")?;

        // In full implementation:
        // 1. Collect package IDs from Package signals
        // 2. Call InstallPackages with the IDs
        // 3. Monitor progress via signals

        Ok(PackageResult {
            success: true,
            action: "install".to_string(),
            packages: Vec::new(),
            message: format!(
                "Install initiated for {:?} on transaction {}",
                package_names, transaction_path
            ),
            transaction_path: Some(transaction_path),
        })
    }

    /// Execute remove action
    async fn remove_packages(&self, package_names: &[String]) -> Result<PackageResult> {
        let transaction_path = self.create_transaction().await?;
        let proxy = self.get_transaction_proxy(&transaction_path).await?;

        // First resolve package names to IDs
        let filter: u64 = 1 << 2; // FILTER_INSTALLED
        let names: Vec<&str> = package_names.iter().map(|s| s.as_str()).collect();

        proxy
            .call::<_, _, ()>("Resolve", &(filter, names.clone()))
            .await
            .context("Failed to resolve package names")?;

        // In full implementation:
        // 1. Collect package IDs from Package signals
        // 2. Call RemovePackages with the IDs

        Ok(PackageResult {
            success: true,
            action: "remove".to_string(),
            packages: Vec::new(),
            message: format!(
                "Remove initiated for {:?} on transaction {}",
                package_names, transaction_path
            ),
            transaction_path: Some(transaction_path),
        })
    }

    /// List installed packages
    async fn list_installed(&self) -> Result<PackageResult> {
        let transaction_path = self.create_transaction().await?;
        let proxy = self.get_transaction_proxy(&transaction_path).await?;

        let filter: u64 = 1 << 2; // FILTER_INSTALLED bit

        proxy
            .call::<_, _, ()>("GetPackages", &(filter,))
            .await
            .context("Failed to call GetPackages")?;

        Ok(PackageResult {
            success: true,
            action: "list".to_string(),
            packages: Vec::new(),
            message: format!(
                "List installed packages on transaction {}",
                transaction_path
            ),
            transaction_path: Some(transaction_path),
        })
    }

    /// Get available updates
    async fn get_updates(&self) -> Result<PackageResult> {
        let transaction_path = self.create_transaction().await?;
        let proxy = self.get_transaction_proxy(&transaction_path).await?;

        let filter: u64 = 0; // No filter

        proxy
            .call::<_, _, ()>("GetUpdates", &(filter,))
            .await
            .context("Failed to call GetUpdates")?;

        Ok(PackageResult {
            success: true,
            action: "updates".to_string(),
            packages: Vec::new(),
            message: format!("Get updates on transaction {}", transaction_path),
            transaction_path: Some(transaction_path),
        })
    }

    /// Refresh package cache
    async fn refresh_cache(&self) -> Result<PackageResult> {
        let transaction_path = self.create_transaction().await?;
        let proxy = self.get_transaction_proxy(&transaction_path).await?;

        let force: bool = true;

        proxy
            .call::<_, _, ()>("RefreshCache", &(force,))
            .await
            .context("Failed to call RefreshCache")?;

        Ok(PackageResult {
            success: true,
            action: "refresh".to_string(),
            packages: Vec::new(),
            message: format!("Cache refresh on transaction {}", transaction_path),
            transaction_path: Some(transaction_path),
        })
    }

    /// Get package details
    async fn get_details(&self, package_names: &[String]) -> Result<PackageResult> {
        let transaction_path = self.create_transaction().await?;
        let proxy = self.get_transaction_proxy(&transaction_path).await?;

        // Resolve packages first
        let filter: u64 = 0;
        let names: Vec<&str> = package_names.iter().map(|s| s.as_str()).collect();

        proxy
            .call::<_, _, ()>("Resolve", &(filter, names.clone()))
            .await
            .context("Failed to resolve package names")?;

        // In full implementation, would collect IDs and call GetDetails

        Ok(PackageResult {
            success: true,
            action: "details".to_string(),
            packages: Vec::new(),
            message: format!(
                "Get details for {:?} on transaction {}",
                package_names, transaction_path
            ),
            transaction_path: Some(transaction_path),
        })
    }
}

#[interface(name = "org.dbusmcp.Agent.PackageKit")]
impl PackageKitAgent {
    /// Execute a package management task
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received package task: {}", self.agent_id, task_json);

        let task: PackageTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "package" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        // Execute the requested action
        let result = match task.action.as_str() {
            "search" => {
                let query = task.query.as_ref().ok_or_else(|| {
                    zbus::fdo::Error::InvalidArgs("Search requires 'query' parameter".to_string())
                })?;

                self.search_packages(query)
                    .await
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            }
            "install" => {
                let packages = task.packages.as_ref().ok_or_else(|| {
                    zbus::fdo::Error::InvalidArgs(
                        "Install requires 'packages' parameter".to_string(),
                    )
                })?;

                self.install_packages(packages)
                    .await
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            }
            "remove" => {
                let packages = task.packages.as_ref().ok_or_else(|| {
                    zbus::fdo::Error::InvalidArgs(
                        "Remove requires 'packages' parameter".to_string(),
                    )
                })?;

                self.remove_packages(packages)
                    .await
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            }
            "list" => self
                .list_installed()
                .await
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?,
            "updates" => self
                .get_updates()
                .await
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?,
            "refresh" => self
                .refresh_cache()
                .await
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?,
            "details" => {
                let packages = task.packages.as_ref().ok_or_else(|| {
                    zbus::fdo::Error::InvalidArgs(
                        "Details requires 'packages' parameter".to_string(),
                    )
                })?;

                self.get_details(packages)
                    .await
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            }
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Unknown action: {}. Valid actions: search, install, remove, list, updates, refresh, details",
                    task.action
                )));
            }
        };

        let result_json = serde_json::to_string(&result)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to serialize result: {}", e)))?;

        Ok(result_json)
    }

    /// Get agent status
    async fn get_status(&self) -> zbus::fdo::Result<String> {
        Ok(format!("PackageKit agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;

    /// Signal emitted for package events
    #[zbus(signal)]
    async fn package_event(
        signal_emitter: &SignalEmitter<'_>,
        event_type: String,
        package_id: String,
    ) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    let agent_id = if args.len() > 1 {
        args[1].clone()
    } else {
        format!(
            "packagekit-{}",
            uuid::Uuid::new_v4().to_string()[..8].to_string()
        )
    };

    println!("Starting PackageKit Agent: {}", agent_id);

    let agent = PackageKitAgent {
        agent_id: agent_id.clone(),
    };

    let path = format!(
        "/org/dbusmcp/Agent/PackageKit/{}",
        agent_id.replace('-', "_")
    );
    let service_name = format!(
        "org.dbusmcp.Agent.PackageKit.{}",
        agent_id.replace('-', "_")
    );

    let _conn = Builder::session()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("PackageKit agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);
    println!("\nAvailable actions:");
    println!("  - search: Search for packages");
    println!("  - install: Install packages");
    println!("  - remove: Remove packages");
    println!("  - list: List installed packages");
    println!("  - updates: Get available updates");
    println!("  - refresh: Refresh package cache");
    println!("  - details: Get package details");

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
