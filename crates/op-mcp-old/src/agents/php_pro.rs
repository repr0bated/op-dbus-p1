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
struct PhpProTask {
    #[serde(rename = "type")]
    task_type: String,
    operation: String, // run, test, lint, analyze
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    args: Option<String>,
}

struct PhpProAgent {
    agent_id: String,
}

#[interface(name = "org.dbusmcp.Agent.PhpPro")]
impl PhpProAgent {
    /// Execute a PHP development task safely
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: PhpProTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "php-pro" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!(
            "[{}] PHP operation: {} on path: {:?}",
            self.agent_id, task.operation, task.path
        );

        let result = match task.operation.as_str() {
            "run" => self.php_run(task.path.as_deref(), task.args.as_deref()),
            "test" => self.phpunit_test(task.path.as_deref()),
            "lint" => self.phpcs_check(task.path.as_deref()),
            "analyze" => self.phpstan_analyze(task.path.as_deref()),
            _ => Err(format!("Unknown PHP operation: {}", task.operation)),
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
        Ok(format!("PHP Pro agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

impl PhpProAgent {
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

    fn validate_args(&self, args: &str) -> Result<(), String> {
        if args.len() > 256 {
            return Err("Args string too long".to_string());
        }

        for forbidden_char in FORBIDDEN_CHARS {
            if args.contains(*forbidden_char) {
                return Err(format!(
                    "Args contains forbidden character: {:?}",
                    forbidden_char
                ));
            }
        }

        Ok(())
    }

    fn php_run(&self, path: Option<&str>, args: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("php");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        }

        if let Some(a) = args {
            self.validate_args(a)?;
            // Split args and add them
            for arg in a.split_whitespace() {
                cmd.arg(arg);
            }
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run php: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "PHP execution succeeded\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "PHP execution failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn phpunit_test(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("php");
        cmd.arg("vendor/bin/phpunit");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        }

        cmd.arg("--verbose");

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run phpunit: {}", e))?;

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

    fn phpcs_check(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("php");
        cmd.arg("vendor/bin/phpcs");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(p);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run phpcs: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "PHPCS passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "PHPCS found issues\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn phpstan_analyze(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("php");
        cmd.arg("vendor/bin/phpstan");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg("analyse").arg(p);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run phpstan: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "PHPStan passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "PHPStan found issues\nstdout: {}\nstderr: {}",
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
        format!("php-pro-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting PHP Pro Agent: {}", agent_id);

    let agent = PhpProAgent::new(agent_id.clone());

    let path = format!("/org/dbusmcp/Agent/PhpPro/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.PhpPro.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("PHP Pro agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
