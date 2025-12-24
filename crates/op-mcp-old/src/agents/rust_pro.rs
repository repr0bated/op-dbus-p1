use serde::Deserialize;
use std::process::Command;
use uuid::Uuid;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

// Security configuration
const ALLOWED_DIRECTORIES: &[&str] = &["/tmp", "/home", "/opt"];
const FORBIDDEN_CHARS: &[char] = &[
    '$', '`', ';', '&', '|', '>', '<', '(', ')', '{', '}', '\n', '\r',
];
const MAX_PATH_LENGTH: usize = 4096;

#[derive(Debug, Deserialize)]
struct RustProTask {
    #[serde(rename = "type")]
    task_type: String,
    operation: String, // check, clippy, build, test, run
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    features: Option<String>,
    #[serde(default)]
    release: bool,
}

struct RustProAgent {
    agent_id: String,
}

#[interface(name = "org.dbusmcp.Agent.RustPro")]
impl RustProAgent {
    /// Execute a Rust development task safely
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: RustProTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "rust-pro" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!(
            "[{}] Rust operation: {} on path: {:?}",
            self.agent_id, task.operation, task.path
        );

        let result = match task.operation.as_str() {
            "check" => self.cargo_check(task.path.as_deref(), task.features.as_deref()),
            "clippy" => self.cargo_clippy(task.path.as_deref(), task.features.as_deref()),
            "build" => {
                self.cargo_build(task.path.as_deref(), task.features.as_deref(), task.release)
            }
            "test" => self.cargo_test(task.path.as_deref(), task.features.as_deref(), task.release),
            _ => Err(format!("Unknown Rust operation: {}", task.operation)),
        };

        match result {
            Ok(data) => {
                let response = serde_json::json!({
                    "success": true,
                    "operation": task.operation,
                    "data": data,
                });
                Ok(response.to_string())
            }
            Err(e) => Err(zbus::fdo::Error::Failed(e)),
        }
    }

    /// Get agent status
    async fn get_status(&self) -> zbus::fdo::Result<String> {
        Ok(format!("Rust Pro agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

impl RustProAgent {
    fn new(agent_id: String) -> Self {
        Self { agent_id }
    }

    fn validate_path(&self, path: &str) -> Result<String, String> {
        if path.len() > MAX_PATH_LENGTH {
            return Err("Path exceeds maximum length".to_string());
        }

        for forbidden_char in FORBIDDEN_CHARS {
            if path.contains(*forbidden_char) {
                return Err(format!(
                    "Path contains forbidden character: {:?}",
                    forbidden_char
                ));
            }
        }

        // Check if path is within allowed directories
        let mut is_allowed = false;
        for allowed in ALLOWED_DIRECTORIES {
            if path.starts_with(allowed) {
                is_allowed = true;
                break;
            }
        }

        if !is_allowed {
            return Err(format!(
                "Path must be within allowed directories: {:?}",
                ALLOWED_DIRECTORIES
            ));
        }

        Ok(path.to_string())
    }

    fn validate_features(&self, features: &str) -> Result<(), String> {
        if features.len() > 256 {
            return Err("Features string too long".to_string());
        }

        for forbidden_char in FORBIDDEN_CHARS {
            if features.contains(*forbidden_char) {
                return Err(format!(
                    "Features contains forbidden character: {:?}",
                    forbidden_char
                ));
            }
        }

        Ok(())
    }

    fn cargo_check(&self, path: Option<&str>, features: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("cargo");
        cmd.arg("check");

        if let Some(feat) = features {
            self.validate_features(feat)?;
            cmd.arg("--features").arg(feat);
        }

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.current_dir(validated_path);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run cargo check: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Check passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Check failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn cargo_clippy(&self, path: Option<&str>, features: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("cargo");
        cmd.arg("clippy");

        if let Some(feat) = features {
            self.validate_features(feat)?;
            cmd.arg("--features").arg(feat);
        }

        cmd.arg("--").arg("-D").arg("warnings");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.current_dir(validated_path);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run cargo clippy: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Clippy passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Clippy failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn cargo_build(
        &self,
        path: Option<&str>,
        features: Option<&str>,
        release: bool,
    ) -> Result<String, String> {
        let mut cmd = Command::new("cargo");
        cmd.arg("build");

        if release {
            cmd.arg("--release");
        }

        if let Some(feat) = features {
            self.validate_features(feat)?;
            cmd.arg("--features").arg(feat);
        }

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.current_dir(validated_path);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run cargo build: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Build succeeded\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Build failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn cargo_test(
        &self,
        path: Option<&str>,
        features: Option<&str>,
        release: bool,
    ) -> Result<String, String> {
        let mut cmd = Command::new("cargo");
        cmd.arg("test");

        if release {
            cmd.arg("--release");
        }

        if let Some(feat) = features {
            self.validate_features(feat)?;
            cmd.arg("--features").arg(feat);
        }

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.current_dir(validated_path);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run cargo test: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Tests passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Tests failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let agent_id = if args.len() > 1 {
        args[1].clone()
    } else {
        format!("rust-pro-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting Rust Pro Agent: {}", agent_id);

    let agent = RustProAgent::new(agent_id.clone());

    let path = format!("/org/dbusmcp/Agent/RustPro/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.RustPro.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("Rust Pro agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
