use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;
use uuid::Uuid;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

// Security configuration
const ALLOWED_COMMANDS: &[&str] = &[
    "ls", "cat", "grep", "ps", "top", "df", "du", "free", "uptime", "whoami", "date", "hostname",
    "pwd", "echo", "wc", "sort", "head", "tail",
];

const FORBIDDEN_CHARS: &[char] = &[
    '$', '`', ';', '&', '|', '>', '<', '(', ')', '{', '}', '\n', '\r',
];
const MAX_COMMAND_LENGTH: usize = 1024;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Deserialize)]
struct ExecuteTask {
    #[serde(rename = "type")]
    task_type: String,
    command: String,
    args: Option<Vec<String>>,
    timeout: Option<u64>,
    working_dir: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExecuteResult {
    success: bool,
    exit_code: i32,
    stdout: String,
    stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub struct ExecutorAgent {
    agent_id: String,
    allowed_commands: HashSet<String>,
}

impl ExecutorAgent {
    fn new(agent_id: String) -> Self {
        let allowed_commands: HashSet<String> =
            ALLOWED_COMMANDS.iter().map(|s| s.to_string()).collect();

        Self {
            agent_id,
            allowed_commands,
        }
    }

    fn validate_command(&self, command: &str) -> Result<(), String> {
        // Check command length
        if command.len() > MAX_COMMAND_LENGTH {
            return Err(format!(
                "Command exceeds maximum length of {} characters",
                MAX_COMMAND_LENGTH
            ));
        }

        // Check for empty command
        if command.trim().is_empty() {
            return Err("Command cannot be empty".to_string());
        }

        // Check for forbidden characters that could lead to injection
        for forbidden_char in FORBIDDEN_CHARS {
            if command.contains(*forbidden_char) {
                return Err(format!(
                    "Command contains forbidden character: {:?}",
                    forbidden_char
                ));
            }
        }

        // Extract the base command (first word)
        let base_command = command
            .split_whitespace()
            .next()
            .ok_or_else(|| "Invalid command format".to_string())?;

        // Check if command is in allowlist
        if !self.allowed_commands.contains(base_command) {
            return Err(format!(
                "Command '{}' is not in the allowed list",
                base_command
            ));
        }

        Ok(())
    }

    fn validate_args(&self, args: &[String]) -> Result<(), String> {
        for arg in args {
            // Check for command injection attempts in arguments
            if arg.contains("..") {
                return Err("Path traversal detected in arguments".to_string());
            }

            for forbidden_char in FORBIDDEN_CHARS {
                if arg.contains(*forbidden_char) {
                    return Err(format!(
                        "Argument contains forbidden character: {:?}",
                        forbidden_char
                    ));
                }
            }

            if arg.len() > 256 {
                return Err("Argument too long".to_string());
            }
        }
        Ok(())
    }

    fn validate_working_dir(&self, dir: &str) -> Result<(), String> {
        // Prevent directory traversal
        if dir.contains("..") {
            return Err("Directory traversal not allowed".to_string());
        }

        // Check if path is absolute and within allowed directories
        if !dir.starts_with("/home/") && !dir.starts_with("/tmp/") && !dir.starts_with("/var/log/")
        {
            return Err("Working directory must be within /home/, /tmp/, or /var/log/".to_string());
        }

        Ok(())
    }

    async fn execute_safely(&self, task: ExecuteTask) -> Result<ExecuteResult, String> {
        // Parse command and arguments
        let parts: Vec<&str> = task.command.split_whitespace().collect();
        if parts.is_empty() {
            return Err("Empty command".to_string());
        }

        let base_command = parts[0];
        let command_args: Vec<String> = if parts.len() > 1 {
            parts[1..].iter().map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        };

        // Merge with provided args if any
        let mut all_args = command_args;
        if let Some(additional_args) = task.args {
            self.validate_args(&additional_args)?;
            all_args.extend(additional_args);
        }

        // Validate the base command
        if !self.allowed_commands.contains(base_command) {
            return Err(format!("Command '{}' is not allowed", base_command));
        }

        // Validate all arguments
        self.validate_args(&all_args)?;

        // Setup command
        let mut cmd = Command::new(base_command);
        cmd.args(&all_args);

        // Set working directory if provided
        if let Some(dir) = task.working_dir {
            self.validate_working_dir(&dir)?;
            cmd.current_dir(dir);
        }

        // Set timeout
        let timeout_secs = task
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);

        // Execute with timeout
        let output = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking(move || cmd.output()),
        )
        .await;

        match output {
            Ok(Ok(Ok(output))) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                Ok(ExecuteResult {
                    success: output.status.success(),
                    exit_code,
                    stdout,
                    stderr,
                    error: None,
                })
            }
            Ok(Ok(Err(e))) => Err(format!("Failed to execute command: {}", e)),
            Ok(Err(e)) => Err(format!("Task panic: {}", e)),
            Err(_) => Err(format!("Command timed out after {} seconds", timeout_secs)),
        }
    }
}

#[interface(name = "org.dbusmcp.Agent.Executor")]
impl ExecutorAgent {
    /// Execute a command safely
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: ExecuteTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "execute" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        // Validate command before execution
        if let Err(e) = self.validate_command(&task.command) {
            return Err(zbus::fdo::Error::AccessDenied(format!(
                "Command validation failed: {}",
                e
            )));
        }

        println!(
            "[{}] Executing validated command: {}",
            self.agent_id, task.command
        );

        match self.execute_safely(task).await {
            Ok(result) => {
                let json = serde_json::to_string(&result)
                    .unwrap_or_else(|e| format!(r#"{{"error": "Serialization failed: {}"}}"#, e));
                Ok(json)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(format!("Execution failed: {}", e))),
        }
    }

    /// Get agent status
    async fn get_status(&self) -> zbus::fdo::Result<String> {
        let status = serde_json::json!({
            "agent_id": self.agent_id,
            "status": "running",
            "allowed_commands": self.allowed_commands.iter().collect::<Vec<_>>(),
            "max_timeout": MAX_TIMEOUT_SECS,
        });
        Ok(status.to_string())
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let agent_id = if args.len() > 1 {
        args[1].clone()
    } else {
        format!("executor-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting Secure Executor Agent: {}", agent_id);
    println!("Allowed commands: {:?}", ALLOWED_COMMANDS);

    let agent = ExecutorAgent::new(agent_id.clone());

    let path = format!("/org/dbusmcp/Agent/Executor/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.Executor.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("Secure Executor agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
