//! System Tools

use async_trait::async_trait;
use serde_json::{json, Value};
use op_core::Tool;
use sysinfo::{System, SystemExt, CpuExt, DiskExt, ProcessExt};

pub struct SystemTool {
    name: String,
    description: String,
}

impl SystemTool {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for SystemTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    async fn execute(&self, _args: Value) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let mut sys = System::new_all();
        sys.refresh_all();

        match self.name.as_str() {
            "system_info" => {
                Ok(json!({
                    "hostname": gethostname::gethostname().to_string_lossy(),
                    "kernel": sys.kernel_version(),
                    "os": sys.name(),
                    "cpu_count": sys.cpus().len(),
                    "memory_total_mb": sys.total_memory() / 1024 / 1024,
                    "memory_used_mb": sys.used_memory() / 1024 / 1024
                }))
            }
            "system_processes" => {
                let processes: Vec<_> = sys.processes().iter()
                    .take(20)
                    .map(|(pid, proc)| json!({
                        "pid": pid.as_u32(),
                        "name": proc.name(),
                        "cpu": proc.cpu_usage(),
                        "memory_mb": proc.memory() / 1024 / 1024
                    }))
                    .collect();
                Ok(json!({"processes": processes}))
            }
            "system_memory" => {
                Ok(json!({
                    "total_mb": sys.total_memory() / 1024 / 1024,
                    "used_mb": sys.used_memory() / 1024 / 1024,
                    "free_mb": sys.free_memory() / 1024 / 1024,
                    "percent": (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0
                }))
            }
            "system_disk" => {
                let disks: Vec<_> = sys.disks().iter()
                    .map(|d| json!({
                        "name": d.name().to_string_lossy(),
                        "mount": d.mount_point().to_string_lossy(),
                        "total_gb": d.total_space() / 1024 / 1024 / 1024,
                        "free_gb": d.available_space() / 1024 / 1024 / 1024
                    }))
                    .collect();
                Ok(json!({"disks": disks}))
            }
            _ => Ok(json!({"error": "Not implemented"}))
        }
    }
}
