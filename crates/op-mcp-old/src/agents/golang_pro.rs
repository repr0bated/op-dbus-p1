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
struct GolangProTask {
    #[serde(rename = "type")]
    task_type: String,
    operation: String, // run, test, build, fmt, vet, mod
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    args: Option<String>,
}

struct GolangProAgent {
    agent_id: String,
}

#[interface(name = "org.dbusmcp.Agent.GolangPro")]
impl GolangProAgent {
    /// Execute a Go development task safely
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: GolangProTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "golang-pro" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!(
            "[{}] Go operation: {} on path: {:?}",
            self.agent_id, task.operation, task.path
        );

        let result = match task.operation.as_str() {
            "run" => self.go_run(task.path.as_deref(), task.args.as_deref()),
            "test" => self.go_test(task.path.as_deref()),
            "build" => self.go_build(task.path.as_deref()),
            "fmt" => self.go_fmt(task.path.as_deref()),
            "vet" => self.go_vet(task.path.as_deref()),
            "mod" => self.go_mod(task.path.as_deref()),
            _ => Err(format!("Unknown Go operation: {}", task.operation)),
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
        Ok(format!("Golang Pro agent {} is running", self.agent_id))
    }

    /// Signal emitted when task completes
    #[zbus(signal)]
    async fn task_completed(signal_emitter: &SignalEmitter<'_>, result: String)
        -> zbus::Result<()>;
}

impl GolangProAgent {
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

    fn go_run(&self, path: Option<&str>, args: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("go");
        cmd.arg("run");

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
            .map_err(|e| format!("Failed to run go run: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Go run succeeded\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Go run failed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn go_test(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("go");
        cmd.arg("test");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        }

        cmd.arg("-v");

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run go test: {}", e))?;

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

    fn go_build(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("go");
        cmd.arg("build");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg("-o").arg("/tmp/go_output").arg(p);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run go build: {}", e))?;

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

    fn go_fmt(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("gofmt");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg("-d").arg(validated_path);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run gofmt: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if stdout.is_empty() {
            Ok("Code is properly formatted".to_string())
        } else {
            Ok(format!("Formatting needed:\n{}", stdout))
        }
    }

    fn go_vet(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("go");
        cmd.arg("vet");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.arg(validated_path);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run go vet: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Go vet passed\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Go vet found issues\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        }
    }

    fn go_mod(&self, path: Option<&str>) -> Result<String, String> {
        let mut cmd = Command::new("go");
        cmd.arg("mod");
        cmd.arg("tidy");

        if let Some(p) = path {
            let validated_path = self.validate_path(p)?;
            cmd.current_dir(validated_path);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run go mod tidy: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(format!(
                "Go mod tidy succeeded\nstdout: {}\nstderr: {}",
                stdout, stderr
            ))
        } else {
            Ok(format!(
                "Go mod tidy failed\nstdout: {}\nstderr: {}",
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
        format!("golang-pro-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting Golang Pro Agent: {}", agent_id);

    let agent = GolangProAgent::new(agent_id.clone());

    let path = format!(
        "/org/dbusmcp/Agent/GolangPro/{}",
        agent_id.replace('-', "_")
    );
    let service_name = format!("org.dbusmcp.Agent.GolangPro.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("Golang Pro agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
