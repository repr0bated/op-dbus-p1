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
struct SqlProTask {
    #[serde(rename = "type")]
    task_type: String,
    operation: String, // format, lint, validate
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    dialect: Option<String>, // postgres, mysql, sqlite
}

struct SqlProAgent {
    agent_id: String,
}

#[interface(name = "org.dbusmcp.Agent.SqlPro")]
impl SqlProAgent {
    /// Execute a SQL development task safely
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: SqlProTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "sql-pro" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!(
            "[{}] SQL operation: {} on path: {:?}",
            self.agent_id, task.operation, task.path
        );

        let result = match task.operation.as_str() {
            "format" => self.sqlfluff_format(task.path.as_deref(), task.dialect.as_deref()),
            "lint" => self.sqlfluff_lint(task.path.as_deref(), task.dialect.as_deref()),
            "validate" => self.sql_validate(task.path.as_deref(), task.dialect.as_deref()),
            _ => Err(format!("Unknown SQL operation: {}", task.operation)),
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
        Ok(format!("SQL Pro agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

impl SqlProAgent {
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

    fn validate_dialect(&self, dialect: &str) -> Result<(), String> {
        let valid_dialects = ["postgres", "mysql", "sqlite", "bigquery", "snowflake"];
        if !valid_dialects.contains(&dialect) {
            return Err(format!(
                "Invalid dialect: {}. Valid: {:?}",
                dialect, valid_dialects
            ));
        }
        Ok(())
    }

    fn sqlfluff_format(&self, path: Option<&str>, dialect: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("sqlfluff");
        cmd.arg("format");

        if let Some(d) = dialect {
            self.validate_dialect(d)?;
            cmd.arg("--dialect").arg(d);
        }

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        } else {
            return Err("Path required for formatting".to_string());
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run sqlfluff format: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "SQL formatted\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Formatting failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn sqlfluff_lint(&self, path: Option<&str>, dialect: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("sqlfluff");
        cmd.arg("lint");

        if let Some(d) = dialect {
            self.validate_dialect(d)?;
            cmd.arg("--dialect").arg(d);
        }

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        } else {
            return Err("Path required for linting".to_string());
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run sqlfluff lint: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "SQL linting passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "SQL linting found issues\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn sql_validate(&self, path: Option<&str>, dialect: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("sqlfluff");
        cmd.arg("parse");

        if let Some(d) = dialect {
            self.validate_dialect(d)?;
            cmd.arg("--dialect").arg(d);
        }

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        } else {
            return Err("Path required for validation".to_string());
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run sqlfluff parse: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "SQL validation passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "SQL validation failed\nstdout: {}\nstderr: {}",
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
        format!("sql-pro-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting SQL Pro Agent: {}", agent_id);

    let agent = SqlProAgent::new(agent_id.clone());

    let path = format!("/org/dbusmcp/Agent/SqlPro/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.SqlPro.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("SQL Pro agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
