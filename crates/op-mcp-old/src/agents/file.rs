use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

// Security configuration
const ALLOWED_DIRECTORIES: &[&str] = &["/home", "/tmp", "/var/log", "/opt"];

const FORBIDDEN_DIRECTORIES: &[&str] = &[
    "/etc",
    "/root",
    "/boot",
    "/sys",
    "/proc",
    "/dev",
    "/usr/bin",
    "/usr/sbin",
    "/bin",
    "/sbin",
];

const FORBIDDEN_FILES: &[&str] = &[
    ".ssh/id_rsa",
    ".ssh/id_ed25519",
    ".ssh/authorized_keys",
    ".bash_history",
    ".zsh_history",
    "shadow",
    "passwd",
    "sudoers",
    ".env",
    ".git/config",
];

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const MAX_PATH_LENGTH: usize = 4096;

#[derive(Debug, Deserialize)]
struct FileTask {
    #[serde(rename = "type")]
    task_type: String,
    operation: String, // read, write, delete, exists, list, mkdir
    path: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    recursive: bool,
}

#[derive(Debug, Serialize)]
struct FileResult {
    success: bool,
    operation: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub struct FileAgent {
    agent_id: String,
    base_directory: PathBuf,
}

impl FileAgent {
    fn new(agent_id: String) -> Self {
        // Set a safe base directory - can be configured
        let base_directory = PathBuf::from(
            std::env::var("FILE_AGENT_BASE_DIR").unwrap_or_else(|_| "/tmp/file-agent".to_string()),
        );

        // Ensure base directory exists
        let _ = fs::create_dir_all(&base_directory);

        Self {
            agent_id,
            base_directory,
        }
    }

    fn validate_path(&self, path: &str) -> Result<PathBuf, String> {
        // Check path length
        if path.len() > MAX_PATH_LENGTH {
            return Err("Path exceeds maximum length".to_string());
        }

        // Convert to PathBuf and canonicalize
        let path_buf = Path::new(path);

        // Check if path is absolute
        let absolute_path = if path_buf.is_absolute() {
            path_buf.to_path_buf()
        } else {
            // Make relative paths relative to base directory
            self.base_directory.join(path_buf)
        };

        // Canonicalize to resolve .. and symlinks
        let canonical = absolute_path
            .canonicalize()
            .or_else(|_| {
                // If file doesn't exist yet (for write operations), canonicalize parent
                if let Some(parent) = absolute_path.parent() {
                    parent
                        .canonicalize()
                        .map(|p| p.join(absolute_path.file_name().unwrap_or_default()))
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Invalid path",
                    ))
                }
            })
            .map_err(|e| format!("Failed to resolve path: {}", e))?;

        // Convert to string for validation
        let path_str = canonical
            .to_str()
            .ok_or_else(|| "Invalid UTF-8 in path".to_string())?;

        // Check against forbidden directories
        for forbidden in FORBIDDEN_DIRECTORIES {
            if path_str.starts_with(forbidden) {
                return Err(format!("Access to {} is forbidden", forbidden));
            }
        }

        // Check against forbidden files
        for forbidden in FORBIDDEN_FILES {
            if path_str.contains(forbidden) {
                return Err(format!("Access to {} is forbidden", forbidden));
            }
        }

        // Check if path is within allowed directories
        let mut is_allowed = false;
        for allowed in ALLOWED_DIRECTORIES {
            if path_str.starts_with(allowed) {
                is_allowed = true;
                break;
            }
        }

        // Also allow if within base directory
        if canonical.starts_with(&self.base_directory) {
            is_allowed = true;
        }

        if !is_allowed {
            return Err(format!(
                "Path must be within allowed directories: {:?} or base directory: {:?}",
                ALLOWED_DIRECTORIES, self.base_directory
            ));
        }

        Ok(canonical)
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        let validated_path = self.validate_path(path)?;

        // Check file size before reading
        let metadata = fs::metadata(&validated_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(format!(
                "File size exceeds maximum of {} bytes",
                MAX_FILE_SIZE
            ));
        }

        // Read file
        fs::read_to_string(validated_path).map_err(|e| format!("Failed to read file: {}", e))
    }

    fn write_file(&self, path: &str, content: Option<&str>) -> Result<String, String> {
        let content = content.ok_or("Content is required for write operation")?;

        // Check content size
        if content.len() > MAX_FILE_SIZE as usize {
            return Err(format!(
                "Content size exceeds maximum of {} bytes",
                MAX_FILE_SIZE
            ));
        }

        let validated_path = self.validate_path(path)?;

        // Ensure parent directory exists
        if let Some(parent) = validated_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }

        // Write file
        fs::write(validated_path, content).map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(format!("Wrote {} bytes", content.len()))
    }

    fn delete_file(&self, path: &str, recursive: bool) -> Result<String, String> {
        let validated_path = self.validate_path(path)?;

        // Extra safety: Don't allow recursive deletion of important directories
        if recursive && validated_path.parent() == Some(Path::new("/")) {
            return Err("Cannot recursively delete root-level directories".to_string());
        }

        if validated_path.is_dir() {
            if recursive {
                // Count files before deletion as safety check
                let count = fs::read_dir(&validated_path)
                    .map_err(|e| format!("Failed to read directory: {}", e))?
                    .count();

                if count > 100 {
                    return Err(format!(
                        "Directory contains {} items. Too many for safe deletion",
                        count
                    ));
                }

                fs::remove_dir_all(validated_path)
                    .map_err(|e| format!("Failed to delete directory: {}", e))?;
            } else {
                fs::remove_dir(validated_path)
                    .map_err(|e| format!("Failed to delete directory: {}", e))?;
            }
        } else {
            fs::remove_file(validated_path).map_err(|e| format!("Failed to delete file: {}", e))?;
        }

        Ok("Deleted successfully".to_string())
    }

    fn file_exists(&self, path: &str) -> Result<String, String> {
        match self.validate_path(path) {
            Ok(validated_path) => {
                let exists = validated_path.exists();
                Ok(exists.to_string())
            }
            Err(_) => {
                // If path validation fails, file doesn't exist in allowed areas
                Ok("false".to_string())
            }
        }
    }

    fn list_directory(&self, path: &str) -> Result<String, String> {
        let validated_path = self.validate_path(path)?;

        if !validated_path.is_dir() {
            return Err("Path is not a directory".to_string());
        }

        let entries =
            fs::read_dir(validated_path).map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut files = Vec::new();
        for entry in entries {
            if let Ok(entry) = entry {
                if let Some(name) = entry.file_name().to_str() {
                    // Skip hidden files unless explicitly requested
                    if !name.starts_with('.') {
                        files.push(name.to_string());
                    }
                }
            }
        }

        Ok(serde_json::to_string(&files).unwrap_or_else(|_| "[]".to_string()))
    }

    fn create_directory(&self, path: &str, recursive: bool) -> Result<String, String> {
        let validated_path = self.validate_path(path)?;

        if recursive {
            fs::create_dir_all(validated_path)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        } else {
            fs::create_dir(validated_path)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        Ok("Directory created successfully".to_string())
    }
}

#[interface(name = "org.dbusmcp.Agent.File")]
impl FileAgent {
    /// Execute a file operation task safely
    async fn execute(&self, task_json: String) -> zbus::fdo::Result<String> {
        println!("[{}] Received task: {}", self.agent_id, task_json);

        let task: FileTask = match serde_json::from_str(&task_json) {
            Ok(t) => t,
            Err(e) => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Failed to parse task: {}",
                    e
                )));
            }
        };

        if task.task_type != "file" {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown task type: {}",
                task.task_type
            )));
        }

        println!(
            "[{}] File operation: {} on path: {}",
            self.agent_id, task.operation, task.path
        );

        let result = match task.operation.as_str() {
            "read" => self.read_file(&task.path),
            "write" => self.write_file(&task.path, task.content.as_deref()),
            "delete" => self.delete_file(&task.path, task.recursive),
            "exists" => self.file_exists(&task.path),
            "list" => self.list_directory(&task.path),
            "mkdir" => self.create_directory(&task.path, task.recursive),
            _ => Err(format!("Unknown file operation: {}", task.operation)),
        };

        let response = match result {
            Ok(data) => FileResult {
                success: true,
                operation: task.operation,
                path: task.path,
                data: Some(data),
                error: None,
            },
            Err(e) => FileResult {
                success: false,
                operation: task.operation,
                path: task.path,
                data: None,
                error: Some(e),
            },
        };

        serde_json::to_string(&response)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Serialization error: {}", e)))
    }

    /// Get agent status
    async fn get_status(&self) -> zbus::fdo::Result<String> {
        let status = serde_json::json!({
            "agent_id": self.agent_id,
            "status": "running",
            "base_directory": self.base_directory.to_str(),
            "allowed_directories": ALLOWED_DIRECTORIES,
            "max_file_size": MAX_FILE_SIZE,
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
        format!("file-{}", Uuid::new_v4().to_string()[..8].to_string())
    };

    println!("Starting Secure File Agent: {}", agent_id);
    println!("Allowed directories: {:?}", ALLOWED_DIRECTORIES);
    println!("Max file size: {} bytes", MAX_FILE_SIZE);

    let agent = FileAgent::new(agent_id.clone());
    println!("Base directory: {:?}", agent.base_directory);

    let path = format!("/org/dbusmcp/Agent/File/{}", agent_id.replace('-', "_"));
    let service_name = format!("org.dbusmcp.Agent.File.{}", agent_id.replace('-', "_"));

    let _conn = Builder::system()?
        .name(service_name.as_str())?
        .serve_at(path.as_str(), agent)?
        .build()
        .await?;

    println!("Secure File agent {} ready on D-Bus", agent_id);
    println!("Service: {}", service_name);
    println!("Path: {}", path);

    // Keep running
    std::future::pending::<()>().await;

    Ok(())
}
