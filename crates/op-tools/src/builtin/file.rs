//! File Tools

use async_trait::async_trait;
use serde_json::{json, Value};
use crate::Tool;
use std::path::Path;

pub struct FileTool {
    name: String,
    description: String,
}

impl FileTool {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        match self.name.as_str() {
            "file_read" => json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
            "file_write" => json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to write"},
                    "content": {"type": "string", "description": "Content to write"}
                },
                "required": ["path", "content"]
            }),
            "file_list" => json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path"}
                },
                "required": ["path"]
            }),
            _ => json!({"type": "object", "properties": {}})
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<Value> {
        match self.name.as_str() {
            "file_read" => {
                let path = args.get("path").and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
                validate_path(path)?;
                let content = tokio::fs::read_to_string(path).await?;
                Ok(json!({"path": path, "content": content}))
            }
            "file_write" => {
                let path = args.get("path").and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
                let content = args.get("content").and_then(|c| c.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing content"))?;
                validate_path(path)?;
                tokio::fs::write(path, content).await?;
                Ok(json!({"path": path, "written": content.len()}))
            }
            "file_list" => {
                let path = args.get("path").and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
                validate_path(path)?;
                let mut entries = tokio::fs::read_dir(path).await?;
                let mut files = Vec::new();
                while let Some(entry) = entries.next_entry().await? {
                    files.push(entry.file_name().to_string_lossy().to_string());
                }
                Ok(json!({"path": path, "files": files}))
            }
            "file_exists" => {
                let path = args.get("path").and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
                let exists = Path::new(path).exists();
                Ok(json!({"path": path, "exists": exists}))
            }
            "file_stat" => {
                let path = args.get("path").and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
                let metadata = tokio::fs::metadata(path).await?;
                Ok(json!({
                    "path": path,
                    "size": metadata.len(),
                    "is_file": metadata.is_file(),
                    "is_dir": metadata.is_dir()
                }))
            }
            _ => Ok(json!({"error": "Not implemented"}))
        }
    }
}

fn validate_path(path: &str) -> anyhow::Result<()> {
    // Security: Block access to sensitive paths
    let forbidden = ["/etc/shadow", "/etc/passwd", "/root", "/.ssh"];
    for f in forbidden {
        if path.contains(f) {
            return Err(anyhow::anyhow!("Access to {} is forbidden", f));
        }
    }
    Ok(())
}
