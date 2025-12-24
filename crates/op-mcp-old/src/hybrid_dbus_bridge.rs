//! Hybrid D-Bus Bridge
//! Exposes non-D-Bus system resources (filesystem, processes, hardware) via D-Bus
//! Creates a unified D-Bus interface for everything discovered by hybrid scanner

use crate::hybrid_scanner::{FilesystemResource, HardwareDevice, HybridScanner};
use anyhow::Result;
use zbus::{connection::Builder, interface};

/// D-Bus service that exposes hybrid system resources
pub struct HybridSystemBridge {
    scanner: HybridScanner,
}

impl HybridSystemBridge {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            scanner: HybridScanner::new().await?,
        })
    }
}

#[interface(name = "org.opdbus.HybridSystem")]
impl HybridSystemBridge {
    /// Scan all system resources (D-Bus + non-D-Bus)
    async fn scan_all(&self) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// List all filesystem resources
    async fn list_filesystem_resources(&self, path: String) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let filtered: Vec<&FilesystemResource> = result
                    .filesystem_resources
                    .iter()
                    .filter(|r| r.path.starts_with(&path))
                    .collect();

                let json = serde_json::to_string_pretty(&filtered)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// List all running processes
    async fn list_processes(&self) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result.processes)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Get process by PID
    async fn get_process(&self, pid: u32) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                if let Some(process) = result.processes.iter().find(|p| p.pid == pid) {
                    let json = serde_json::to_string_pretty(process)
                        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                    Ok(json)
                } else {
                    Err(zbus::fdo::Error::Failed(format!(
                        "Process {} not found",
                        pid
                    )))
                }
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// List all hardware devices
    async fn list_hardware(&self) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result.hardware)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// List hardware by type (pci, block, network, etc.)
    async fn list_hardware_by_type(&self, device_type: String) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let filtered: Vec<&HardwareDevice> = result
                    .hardware
                    .iter()
                    .filter(|d| d.device_type == device_type)
                    .collect();

                let json = serde_json::to_string_pretty(&filtered)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// List all network interfaces
    async fn list_network_interfaces(&self) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result.network_interfaces)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Get network interface by name
    async fn get_network_interface(&self, name: String) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                if let Some(interface) = result.network_interfaces.iter().find(|i| i.name == name) {
                    let json = serde_json::to_string_pretty(interface)
                        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                    Ok(json)
                } else {
                    Err(zbus::fdo::Error::Failed(format!(
                        "Interface {} not found",
                        name
                    )))
                }
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// List system configuration files
    async fn list_config_files(&self) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let json = serde_json::to_string_pretty(&result.system_config)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Get D-Bus services count
    async fn get_dbus_services_count(&self) -> zbus::fdo::Result<u32> {
        match self.scanner.scan_all().await {
            Ok(result) => Ok(result.dbus_services.len() as u32),
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }

    /// Get system stats summary
    async fn get_stats_summary(&self) -> zbus::fdo::Result<String> {
        match self.scanner.scan_all().await {
            Ok(result) => {
                let stats = serde_json::json!({
                    "dbus_services": result.dbus_services.len(),
                    "filesystem_resources": result.filesystem_resources.len(),
                    "processes": result.processes.len(),
                    "hardware_devices": result.hardware.len(),
                    "network_interfaces": result.network_interfaces.len(),
                    "config_files": result.system_config.len(),
                    "timestamp": result.timestamp,
                });

                let json = serde_json::to_string_pretty(&stats)
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e.to_string())),
        }
    }
}

/// Start the hybrid system bridge D-Bus service
pub async fn start_hybrid_bridge() -> Result<()> {
    let bridge = HybridSystemBridge::new().await?;

    let _connection = Builder::system()?
        .name("org.opdbus.HybridSystem")?
        .serve_at("/org/opdbus/HybridSystem", bridge)?
        .build()
        .await?;

    log::info!("Hybrid System Bridge started on D-Bus");
    log::info!("Service: org.opdbus.HybridSystem");
    log::info!("Path: /org/opdbus/HybridSystem");

    // Keep the connection alive
    std::future::pending::<()>().await;

    Ok(())
}

/// CLI tool for testing the hybrid bridge
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage:");
        println!(
            "  {} start              - Start hybrid D-Bus bridge service",
            args[0]
        );
        println!("  {} test <method>      - Test a specific method", args[0]);
        println!("\nAvailable test methods:");
        println!("  scan-all              - Scan all resources");
        println!("  list-processes        - List all processes");
        println!("  list-hardware         - List hardware devices");
        println!("  list-network          - List network interfaces");
        println!("  stats                 - Get stats summary");
        return Ok(());
    }

    match args[1].as_str() {
        "start" => {
            println!("Starting Hybrid D-Bus Bridge...");
            start_hybrid_bridge().await?;
        }
        "test" => {
            if args.len() < 3 {
                eprintln!("Error: Test method required");
                return Ok(());
            }

            let bridge = HybridSystemBridge::new().await?;

            match args[2].as_str() {
                "scan-all" => {
                    let result = bridge.scan_all().await?;
                    println!("{}", result);
                }
                "list-processes" => {
                    let result = bridge.list_processes().await?;
                    println!("{}", result);
                }
                "list-hardware" => {
                    let result = bridge.list_hardware().await?;
                    println!("{}", result);
                }
                "list-network" => {
                    let result = bridge.list_network_interfaces().await?;
                    println!("{}", result);
                }
                "stats" => {
                    let result = bridge.get_stats_summary().await?;
                    println!("{}", result);
                }
                _ => {
                    eprintln!("Unknown test method: {}", args[2]);
                }
            }
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
        }
    }

    Ok(())
}
