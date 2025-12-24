use serde::Deserialize;
use std::process::Command;
use uuid::Uuid;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

#[derive(Debug, Deserialize)]
struct MonitorTask {
    #[serde(rename = "type")]
    task_type: String,
    metric: String, // cpu, memory, disk, processes, uptime
    #[serde(default)]
    detailed: bool,
}

pub struct MonitorAgent {
    agent_id: String,
}

#[interface(name = "org.dbusmcp.Agent.Monitor")]
impl MonitorAgent {
    /// Execute a system monitoring task
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: MonitorTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "monitor" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!("[{}] Monitoring metric: {}", self.agent_id, task.metric);

        let result = match task.metric.as_str() {
            "cpu" => self.get_cpu_info(task.detailed),
            "memory" => self.get_memory_info(task.detailed),
            "disk" => self.get_disk_info(task.detailed),
            "processes" => self.get_process_info(task.detailed),
            "uptime" => self.get_uptime(),
            "load" => self.get_load_average(),
            _ => Err(format!("Unknown metric: {}", task.metric)),
        };

        match result {
            Ok(data) => {
                let response = serde_json::json!({
                    "success": true,
                    "metric": task.metric,
                    "data": data,
                });
                Ok(response.to_string())
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e)),
        }
    }

    /// Get agent status
    async fn get_status(&self) -> zbus::fdo::Result<String> {
        Ok(format!("Monitor agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

impl MonitorAgent {
    fn get_cpu_info(&self, detailed: bool) -> Result<String, String> {
        let output = if detailed {
            Command::new("lscpu").output()
        } else {
            Command::new("sh")
                .arg("-c")
                .arg("top -bn1 | grep 'Cpu(s)'")
                .output()
        };

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to get CPU info: {}", e)),
        }
    }

    fn get_memory_info(&self, detailed: bool) -> Result<String, String> {
        let output = if detailed {
            Command::new("free").arg("-h").output()
        } else {
            Command::new("free").arg("-h").arg("--total").output()
        };

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to get memory info: {}", e)),
        }
    }

    fn get_disk_info(&self, detailed: bool) -> Result<String, String> {
        let output = if detailed {
            Command::new("df").arg("-h").output()
        } else {
            Command::new("df").arg("-h").arg("/").output()
        };

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to get disk info: {}", e)),
        }
    }

    fn get_process_info(&self, detailed: bool) -> Result<String, String> {
        let output = if detailed {
            Command::new("ps").arg("aux").arg("--sort=-pmem").output()
        } else {
            Command::new("ps")
                .arg("aux")
                .arg("--sort=-pmem")
                .arg("|")
                .arg("head")
                .arg("-20")
                .output()
        };

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to get process info: {}", e)),
        }
    }

    fn get_uptime(&self) -> Result<String, String> {
        let output = Command::new("uptime").output();

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to get uptime: {}", e)),
        }
    }

    fn get_load_average(&self) -> Result<String, String> {
        let output = Command::new("cat").arg("/proc/loadavg").output();

        match output {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Err(e) => Err(format!("Failed to get load average: {}", e)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let agent_id = if args.len() > 1 {
        args[1].clone()
    } else {
        format!("monitor-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting Monitor Agent: {}", agent_id);

    let agent = MonitorAgent {
        agent_id: agent_id.clone(),
    };

    let path = format!("/org/dbusmcp/Agent/Monitor/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.Monitor.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("Monitor agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
