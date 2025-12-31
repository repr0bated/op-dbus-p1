//! Hybrid System Scanner
//! Discovers both D-Bus services AND non-D-Bus system resources
//! Bridges everything to a unified D-Bus interface

use crate::system_introspection::{DiscoveredService, SystemIntrospector};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Complete system scan result (D-Bus + filesystem + processes + hardware)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSystemScan {
    pub dbus_services: Vec<DiscoveredService>,
    pub filesystem_resources: Vec<FilesystemResource>,
    pub processes: Vec<ProcessInfo>,
    pub hardware: Vec<HardwareDevice>,
    pub network_interfaces: Vec<NetworkInterface>,
    pub system_config: Vec<ConfigFile>,
    pub timestamp: i64,
}

/// Filesystem resource discovered outside D-Bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemResource {
    pub path: String,
    pub resource_type: String, // "config", "device", "socket", "fifo", etc.
    pub permissions: String,
    pub owner: String,
    pub size: Option<u64>,
    pub metadata: HashMap<String, String>,
}

/// Process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    pub user: String,
    pub status: String,
    pub memory_kb: u64,
    pub cpu_percent: f32,
}

/// Hardware device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareDevice {
    pub device_type: String, // "cpu", "disk", "network", "pci", "usb", etc.
    pub name: String,
    pub path: String, // /sys path
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub driver: Option<String>,
    pub attributes: HashMap<String, String>,
}

/// Network interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: Option<String>,
    pub ip_addresses: Vec<String>,
    pub status: String, // "up", "down"
    pub mtu: Option<u32>,
    pub type_: String, // "ethernet", "wifi", "bridge", etc.
}

/// Configuration file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub path: String,
    pub service: Option<String>, // Associated service if known
    pub format: String,          // "ini", "yaml", "json", "conf", etc.
    pub last_modified: i64,
    pub size: u64,
}

pub struct HybridScanner {
    dbus_introspector: SystemIntrospector,
}

impl HybridScanner {
    /// Create new hybrid scanner
    pub async fn new() -> Result<Self> {
        let dbus_introspector = SystemIntrospector::new().await?;

        Ok(Self { dbus_introspector })
    }

    /// Perform complete system scan
    pub async fn scan_all(&self) -> Result<HybridSystemScan> {
        log::info!("Starting hybrid system scan...");

        // 1. Scan D-Bus services
        let dbus_result = self.dbus_introspector.introspect_all_services().await?;

        // 2. Scan filesystem resources
        let filesystem_resources = self.scan_filesystem().await?;

        // 3. Scan processes
        let processes = self.scan_processes().await?;

        // 4. Scan hardware
        let hardware = self.scan_hardware().await?;

        // 5. Scan network interfaces
        let network_interfaces = self.scan_network().await?;

        // 6. Scan system configuration files
        let system_config = self.scan_system_configs().await?;

        Ok(HybridSystemScan {
            dbus_services: dbus_result.services,
            filesystem_resources,
            processes,
            hardware,
            network_interfaces,
            system_config,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// Scan filesystem for important resources
    async fn scan_filesystem(&self) -> Result<Vec<FilesystemResource>> {
        let mut resources = Vec::new();

        // Important directories to scan
        let scan_paths = vec![
            "/dev",     // Device nodes
            "/run",     // Runtime data
            "/var/run", // Runtime data (compat link)
            "/proc",    // Process information
            "/sys",     // Hardware/kernel interface
        ];

        for base_path in scan_paths {
            if let Ok(entries) = fs::read_dir(base_path) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        let file_type = if metadata.is_dir() {
                            "directory"
                        } else if metadata.is_file() {
                            "file"
                        } else if metadata.file_type().is_symlink() {
                            "symlink"
                        } else {
                            "other"
                        };

                        resources.push(FilesystemResource {
                            path: entry.path().to_string_lossy().to_string(),
                            resource_type: file_type.to_string(),
                            permissions: format!("{:o}", metadata.permissions().mode()),
                            owner: format!("{}:{}", metadata.uid(), metadata.gid()),
                            size: if metadata.is_file() {
                                Some(metadata.len())
                            } else {
                                None
                            },
                            metadata: HashMap::new(),
                        });
                    }

                    // Limit to avoid overwhelming the system
                    if resources.len() >= 1000 {
                        break;
                    }
                }
            }

            if resources.len() >= 1000 {
                break;
            }
        }

        log::info!("Found {} filesystem resources", resources.len());
        Ok(resources)
    }

    /// Scan running processes
    async fn scan_processes(&self) -> Result<Vec<ProcessInfo>> {
        let mut processes = Vec::new();

        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    // Check if it's a PID directory
                    if let Ok(pid) = file_name.parse::<u32>() {
                        // Read process info
                        if let Ok(process) = self.read_process_info(pid).await {
                            processes.push(process);
                        }
                    }
                }

                // Limit to avoid overwhelming
                if processes.len() >= 500 {
                    break;
                }
            }
        }

        log::info!("Found {} processes", processes.len());
        Ok(processes)
    }

    /// Read process information from /proc/[pid]
    async fn read_process_info(&self, pid: u32) -> Result<ProcessInfo> {
        let proc_path = format!("/proc/{}", pid);

        // Read command line
        let cmdline_path = format!("{}/cmdline", proc_path);
        let cmdline = fs::read_to_string(&cmdline_path)
            .unwrap_or_default()
            .replace('\0', " ")
            .trim()
            .to_string();

        // Extract process name from cmdline or comm
        let name = if !cmdline.is_empty() {
            cmdline
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string()
        } else {
            let comm_path = format!("{}/comm", proc_path);
            fs::read_to_string(&comm_path)
                .unwrap_or_else(|_| "unknown".to_string())
                .trim()
                .to_string()
        };

        // Read status
        let status_path = format!("{}/status", proc_path);
        let status_content = fs::read_to_string(&status_path).unwrap_or_default();

        let mut user = "unknown".to_string();
        let mut memory_kb = 0u64;
        let mut status = "running".to_string();

        for line in status_content.lines() {
            if line.starts_with("Uid:") {
                if let Some(uid) = line.split_whitespace().nth(1) {
                    user = uid.to_string();
                }
            } else if line.starts_with("VmRSS:") {
                if let Some(mem_str) = line.split_whitespace().nth(1) {
                    memory_kb = mem_str.parse().unwrap_or(0);
                }
            } else if line.starts_with("State:") {
                if let Some(state) = line.split_whitespace().nth(1) {
                    status = state.to_string();
                }
            }
        }

        Ok(ProcessInfo {
            pid,
            name,
            cmdline,
            user,
            status,
            memory_kb,
            cpu_percent: 0.0, // Would need multiple samples to calculate
        })
    }

    /// Scan hardware devices from /sys
    async fn scan_hardware(&self) -> Result<Vec<HardwareDevice>> {
        let mut devices = Vec::new();

        // Scan PCI devices
        if let Ok(pci_devices) = self.scan_pci_devices().await {
            devices.extend(pci_devices);
        }

        // Scan block devices
        if let Ok(block_devices) = self.scan_block_devices().await {
            devices.extend(block_devices);
        }

        // Scan network devices
        if let Ok(net_devices) = self.scan_net_devices().await {
            devices.extend(net_devices);
        }

        log::info!("Found {} hardware devices", devices.len());
        Ok(devices)
    }

    /// Scan PCI devices
    async fn scan_pci_devices(&self) -> Result<Vec<HardwareDevice>> {
        let mut devices = Vec::new();
        let pci_path = "/sys/bus/pci/devices";

        if let Ok(entries) = fs::read_dir(pci_path) {
            for entry in entries.flatten() {
                let device_path = entry.path();

                let vendor = self.read_sysfs_file(&device_path.join("vendor")).ok();
                let device = self.read_sysfs_file(&device_path.join("device")).ok();
                let class = self.read_sysfs_file(&device_path.join("class")).ok();

                devices.push(HardwareDevice {
                    device_type: "pci".to_string(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: device_path.to_string_lossy().to_string(),
                    vendor,
                    model: device,
                    driver: self
                        .read_sysfs_file(&device_path.join("driver/module"))
                        .ok(),
                    attributes: HashMap::from([("class".to_string(), class.unwrap_or_default())]),
                });
            }
        }

        Ok(devices)
    }

    /// Scan block devices
    async fn scan_block_devices(&self) -> Result<Vec<HardwareDevice>> {
        let mut devices = Vec::new();
        let block_path = "/sys/class/block";

        if let Ok(entries) = fs::read_dir(block_path) {
            for entry in entries.flatten() {
                let device_path = entry.path();

                let size = self.read_sysfs_file(&device_path.join("size")).ok();
                let model = self.read_sysfs_file(&device_path.join("device/model")).ok();

                devices.push(HardwareDevice {
                    device_type: "block".to_string(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: device_path.to_string_lossy().to_string(),
                    vendor: None,
                    model,
                    driver: None,
                    attributes: HashMap::from([("size".to_string(), size.unwrap_or_default())]),
                });
            }
        }

        Ok(devices)
    }

    /// Scan network devices from /sys/class/net
    async fn scan_net_devices(&self) -> Result<Vec<HardwareDevice>> {
        let mut devices = Vec::new();
        let net_path = "/sys/class/net";

        if let Ok(entries) = fs::read_dir(net_path) {
            for entry in entries.flatten() {
                let device_path = entry.path();

                let operstate = self.read_sysfs_file(&device_path.join("operstate")).ok();
                let mtu = self.read_sysfs_file(&device_path.join("mtu")).ok();
                let address = self.read_sysfs_file(&device_path.join("address")).ok();

                devices.push(HardwareDevice {
                    device_type: "network".to_string(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: device_path.to_string_lossy().to_string(),
                    vendor: None,
                    model: address.clone(),
                    driver: None,
                    attributes: HashMap::from([
                        ("operstate".to_string(), operstate.unwrap_or_default()),
                        ("mtu".to_string(), mtu.unwrap_or_default()),
                        ("address".to_string(), address.unwrap_or_default()),
                    ]),
                });
            }
        }

        Ok(devices)
    }

    /// Read a sysfs file
    fn read_sysfs_file(&self, path: &Path) -> Result<String> {
        Ok(fs::read_to_string(path)?.trim().to_string())
    }

    /// Scan network interfaces
    async fn scan_network(&self) -> Result<Vec<NetworkInterface>> {
        let mut interfaces = Vec::new();
        let net_path = "/sys/class/net";

        if let Ok(entries) = fs::read_dir(net_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let device_path = entry.path();

                let mac_address = fs::read_to_string(device_path.join("address"))
                    .ok()
                    .map(|s| s.trim().to_string());

                let status = fs::read_to_string(device_path.join("operstate"))
                    .unwrap_or_else(|_| "unknown".to_string())
                    .trim()
                    .to_string();

                let mtu = fs::read_to_string(device_path.join("mtu"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok());

                let type_ = fs::read_to_string(device_path.join("type"))
                    .ok()
                    .map(|s| match s.trim() {
                        "1" => "ethernet",
                        "801" => "wifi",
                        _ => "other",
                    })
                    .unwrap_or("unknown")
                    .to_string();

                interfaces.push(NetworkInterface {
                    name,
                    mac_address,
                    ip_addresses: Vec::new(), // Would use netlink to get IPs
                    status,
                    mtu,
                    type_,
                });
            }
        }

        log::info!("Found {} network interfaces", interfaces.len());
        Ok(interfaces)
    }

    /// Scan system configuration files
    async fn scan_system_configs(&self) -> Result<Vec<ConfigFile>> {
        let mut configs = Vec::new();

        let config_dirs = vec![
            "/etc/systemd/system",
            "/etc/systemd/network",
            "/etc/NetworkManager",
            "/etc/dbus-1/system.d",
        ];

        for dir in config_dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            let path = entry.path().to_string_lossy().to_string();
                            let format = self.detect_config_format(&path);

                            configs.push(ConfigFile {
                                path: path.clone(),
                                service: None,
                                format,
                                last_modified: metadata
                                    .modified()
                                    .map(|t| {
                                        t.duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs()
                                            as i64
                                    })
                                    .unwrap_or(0),
                                size: metadata.len(),
                            });
                        }
                    }
                }
            }
        }

        log::info!("Found {} configuration files", configs.len());
        Ok(configs)
    }

    /// Detect configuration file format
    fn detect_config_format(&self, path: &str) -> String {
        if path.ends_with(".json") {
            "json".to_string()
        } else if path.ends_with(".yaml") || path.ends_with(".yml") {
            "yaml".to_string()
        } else if path.ends_with(".toml") {
            "toml".to_string()
        } else if path.ends_with(".ini") || path.ends_with(".conf") {
            "ini".to_string()
        } else if path.ends_with(".xml") {
            "xml".to_string()
        } else {
            "text".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hybrid_scan() {
        let scanner = HybridScanner::new().await.unwrap();
        let result = scanner.scan_all().await.unwrap();

        println!("Hybrid Scan Results:");
        println!("  D-Bus services: {}", result.dbus_services.len());
        println!(
            "  Filesystem resources: {}",
            result.filesystem_resources.len()
        );
        println!("  Processes: {}", result.processes.len());
        println!("  Hardware devices: {}", result.hardware.len());
        println!("  Network interfaces: {}", result.network_interfaces.len());
        println!("  Config files: {}", result.system_config.len());
    }
}

// Add missing trait
use std::os::unix::fs::{MetadataExt, PermissionsExt};
