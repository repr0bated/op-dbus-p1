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
struct CProTask {
    #[serde(rename = "type")]
    task_type: String,
    operation: String, // compile, valgrind, clang-tidy, gdb
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    flags: Option<String>,
}

struct CProAgent {
    agent_id: String,
}

#[interface(name = "org.dbusmcp.Agent.CPro")]
impl CProAgent {
    /// Execute a C development task safely
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: CProTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "c-pro" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!(
            "[{}] C operation: {} on path: {:?}",
            self.agent_id, task.operation, task.path
        );

        let result = match task.operation.as_str() {
            "compile" => self.gcc_compile(task.path.as_deref(), task.flags.as_deref()),
            "valgrind" => self.valgrind_check(task.path.as_deref()),
            "clang-tidy" => self.clang_tidy(task.path.as_deref()),
            _ => Err(format!("Unknown C operation: {}", task.operation)),
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
        Ok(format!("C Pro agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

impl CProAgent {
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

    fn validate_flags(&self, flags: &str) -> Result<(), String> {
        if flags.len() > 512 {
            return Err("Flags string too long".to_string());
        }

        for forbidden_char in FORBIDDEN_CHARS {
            if flags.contains(*forbidden_char) {
                return Err(format!(
                    "Flags contains forbidden character: {:?}",
                    forbidden_char
                ));
            }
        }

        Ok(())
    }

    fn gcc_compile(&self, path: Option<&str>, flags: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("gcc");
        cmd.arg("-Wall").arg("-Wextra").arg("-Werror");

        if let Some(f) = flags {
            self.validate_flags(f)?;
            // Split flags and add them
            for flag in f.split_whitespace() {
                cmd.arg(flag);
            }
        }

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg("-o").arg("/tmp/c_output").arg(validated_path);
        } else {
            return Err("Path required for compilation".to_string());
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run gcc: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Compilation succeeded\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Compilation failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn valgrind_check(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("valgrind");
        cmd.arg("--leak-check=full").arg("--show-leak-kinds=all");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        } else {
            return Err("Path required for valgrind".to_string());
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run valgrind: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Valgrind always returns non-zero, so don't check success
        Ok(format!(
            "Valgrind output\nstdout: {}\nstderr: {}",
            stdout, stderr
        ))
    }

    fn clang_tidy(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("clang-tidy");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        } else {
            return Err("Path required for clang-tidy".to_string());
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run clang-tidy: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Clang-tidy passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Clang-tidy found issues\nstdout: {}\nstderr: {}",
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
        format!("c-pro-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting C Pro Agent: {}", agent_id);

    let agent = CProAgent::new(agent_id.clone());

    let path = format!("/org/dbusmcp/Agent/CPro/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.CPro.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("C Pro agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
