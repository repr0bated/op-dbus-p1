// AI Context Provider
// Provides rich context to AI about the system state and capabilities

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Complete system context for AI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContext {
    pub hardware: HardwareContext,
    pub network: NetworkContext,
    pub services: ServicesContext,
    pub capabilities: CapabilitiesContext,
    pub restrictions: RestrictionsContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareContext {
    pub cpu_vendor: String,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub memory_gb: f64,
    pub virtualization_available: bool,
    pub is_virtual_machine: bool,
    pub architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkContext {
    pub hostname: String,
    pub interfaces: Vec<String>,
    pub provider: Option<String>,
    pub public_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesContext {
    pub systemd_available: bool,
    pub dbus_available: bool,
    pub packagekit_available: bool,
    pub lxc_available: bool,
    pub running_services_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesContext {
    pub cpu_features: Vec<String>,
    pub can_use_iommu: bool,
    pub can_use_nested_virt: bool,
    pub can_passthrough_gpu: bool,
    pub can_use_sgx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestrictionsContext {
    pub bios_locks: Vec<String>,
    pub provider_restrictions: Vec<String>,
    pub missing_features: Vec<String>,
}

/// AI Context Provider
pub struct AiContextProvider;

impl Default for AiContextProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AiContextProvider {
    pub fn new() -> Self {
        Self
    }

    /// Gather comprehensive system context
    pub async fn gather_context(&self) -> Result<SystemContext> {
        let hardware = self.gather_hardware_context().await?;
        let network = self.gather_network_context().await?;
        let services = self.gather_services_context().await?;
        let capabilities = self.gather_capabilities_context().await?;
        let restrictions = self.gather_restrictions_context().await?;

        Ok(SystemContext {
            hardware,
            network,
            services,
            capabilities,
            restrictions,
        })
    }

    /// Generate a human-readable summary for AI
    pub fn generate_summary(&self, context: &SystemContext) -> String {
        let mut summary = String::from("SYSTEM OVERVIEW:\n\n");

        // Hardware
        summary.push_str(&format!(
            "Hardware:\n\
            - CPU: {} ({})\n\
            - Cores: {}\n\
            - Memory: {:.1} GB\n\
            - Architecture: {}\n\
            - Virtual Machine: {}\n\n",
            context.hardware.cpu_model,
            context.hardware.cpu_vendor,
            context.hardware.cpu_cores,
            context.hardware.memory_gb,
            context.hardware.architecture,
            if context.hardware.is_virtual_machine {
                "Yes"
            } else {
                "No"
            }
        ));

        // Network
        summary.push_str(&format!(
            "Network:\n\
            - Hostname: {}\n\
            - Interfaces: {}\n",
            context.network.hostname,
            context.network.interfaces.join(", ")
        ));

        if let Some(provider) = &context.network.provider {
            summary.push_str(&format!("- Provider: {}\n", provider));
        }
        summary.push('\n');

        // Capabilities
        summary.push_str("Capabilities:\n");
        if context.capabilities.can_use_iommu {
            summary.push_str("✓ IOMMU available\n");
        }
        if context.capabilities.can_use_nested_virt {
            summary.push_str("✓ Nested virtualization available\n");
        }
        if context.capabilities.can_passthrough_gpu {
            summary.push_str("✓ GPU passthrough available\n");
        }
        if context.capabilities.can_use_sgx {
            summary.push_str("✓ Intel SGX available\n");
        }

        if !context.capabilities.cpu_features.is_empty() {
            summary.push_str(&format!(
                "- CPU Features: {}\n",
                context.capabilities.cpu_features.join(", ")
            ));
        }
        summary.push('\n');

        // Restrictions
        if !context.restrictions.bios_locks.is_empty() {
            summary.push_str("⚠ BIOS Locks:\n");
            for lock in &context.restrictions.bios_locks {
                summary.push_str(&format!("  - {}\n", lock));
            }
        }

        if !context.restrictions.provider_restrictions.is_empty() {
            summary.push_str("⚠ Provider Restrictions:\n");
            for restriction in &context.restrictions.provider_restrictions {
                summary.push_str(&format!("  - {}\n", restriction));
            }
        }

        summary
    }

    async fn gather_hardware_context(&self) -> Result<HardwareContext> {
        // Read CPU info
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();

        let cpu_vendor = cpuinfo
            .lines()
            .find(|l| l.starts_with("vendor_id"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let cpu_model = cpuinfo
            .lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let cpu_cores = cpuinfo
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count() as u32;

        // Read memory info
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let memory_kb = meminfo
            .lines()
            .find(|l| l.starts_with("MemTotal"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        let memory_gb = memory_kb / 1024.0 / 1024.0;

        // Check if VM
        let is_virtual_machine = std::fs::read_to_string("/sys/class/dmi/id/product_name")
            .unwrap_or_default()
            .to_lowercase()
            .contains("virtual")
            || std::fs::read_to_string("/sys/class/dmi/id/sys_vendor")
                .unwrap_or_default()
                .to_lowercase()
                .contains("qemu");

        // Get architecture
        let architecture = std::env::consts::ARCH.to_string();

        // Check virtualization support
        let flags = cpuinfo
            .lines()
            .find(|l| l.starts_with("flags"))
            .map(|l| l.to_lowercase())
            .unwrap_or_default();

        let virtualization_available = flags.contains("vmx") || flags.contains("svm");

        Ok(HardwareContext {
            cpu_vendor,
            cpu_model,
            cpu_cores,
            memory_gb,
            virtualization_available,
            is_virtual_machine,
            architecture,
        })
    }

    async fn gather_network_context(&self) -> Result<NetworkContext> {
        let hostname = gethostname::gethostname().to_string_lossy().to_string();

        // Get network interfaces
        let interfaces = std::fs::read_dir("/sys/class/net")
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .filter(|name| name != "lo")
                    .collect()
            })
            .unwrap_or_default();

        // Try to detect provider from hostname
        let provider = if hostname.contains("linode") {
            Some("Linode".to_string())
        } else if hostname.contains("digitalocean") {
            Some("DigitalOcean".to_string())
        } else if hostname.contains("aws") || hostname.contains("ec2") {
            Some("AWS".to_string())
        } else if hostname.contains("azure") {
            Some("Azure".to_string())
        } else if hostname.contains("gcp") || hostname.contains("google") {
            Some("Google Cloud".to_string())
        } else {
            None
        };

        Ok(NetworkContext {
            hostname,
            interfaces,
            provider,
            public_ip: None, // Could be enhanced with actual IP detection
        })
    }

    async fn gather_services_context(&self) -> Result<ServicesContext> {
        let systemd_available = std::path::Path::new("/run/systemd/system").exists();
        let dbus_available = std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
            || std::path::Path::new("/var/run/dbus/system_bus_socket").exists();

        Ok(ServicesContext {
            systemd_available,
            dbus_available,
            packagekit_available: false, // Could check for PackageKit
            lxc_available: std::path::Path::new("/usr/bin/lxc").exists(),
            running_services_count: 0, // Could enumerate
        })
    }

    async fn gather_capabilities_context(&self) -> Result<CapabilitiesContext> {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let flags = cpuinfo
            .lines()
            .find(|l| l.starts_with("flags"))
            .map(|l| l.to_lowercase())
            .unwrap_or_default();

        let mut cpu_features = Vec::new();

        if flags.contains("vmx") {
            cpu_features.push("Intel VT-x".to_string());
        }
        if flags.contains("svm") {
            cpu_features.push("AMD-V".to_string());
        }
        if flags.contains("avx") {
            cpu_features.push("AVX".to_string());
        }
        if flags.contains("avx2") {
            cpu_features.push("AVX2".to_string());
        }

        let can_use_iommu = std::path::Path::new("/sys/kernel/iommu_groups").exists();
        let can_use_nested_virt = flags.contains("vmx") || flags.contains("svm");
        let can_passthrough_gpu = can_use_iommu;
        let can_use_sgx = flags.contains("sgx");

        Ok(CapabilitiesContext {
            cpu_features,
            can_use_iommu,
            can_use_nested_virt,
            can_passthrough_gpu,
            can_use_sgx,
        })
    }

    async fn gather_restrictions_context(&self) -> Result<RestrictionsContext> {
        let bios_locks = Vec::new();
        let mut provider_restrictions = Vec::new();
        let mut missing_features = Vec::new();

        // Check for common restrictions
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let flags = cpuinfo
            .lines()
            .find(|l| l.starts_with("flags"))
            .map(|l| l.to_lowercase())
            .unwrap_or_default();

        if !flags.contains("vmx") && !flags.contains("svm") {
            missing_features.push("Hardware virtualization (VT-x/AMD-V)".to_string());
        }

        if !std::path::Path::new("/sys/kernel/iommu_groups").exists() {
            missing_features.push("IOMMU support".to_string());
        }

        // Check if running in VM (common provider restriction)
        let is_vm = std::fs::read_to_string("/sys/class/dmi/id/product_name")
            .unwrap_or_default()
            .to_lowercase()
            .contains("virtual");

        if is_vm {
            provider_restrictions.push(
                "Running in virtual machine - nested virtualization may be restricted".to_string(),
            );
        }

        Ok(RestrictionsContext {
            bios_locks,
            provider_restrictions,
            missing_features,
        })
    }
}
