//! Git Tools
//!
//! Provides structured tools for Git operations.
//!
//! ## Tools
//! - git_status: Show working tree status
//! - git_add: Add file contents to the index
//! - git_commit: Record changes to the repository
//! - git_push: Update remote refs along with associated objects
//! - git_diff: Show changes between commits, commit and working tree, etc
//! - git_log: Show commit logs
//! - git_checkout: Switch branches or restore working tree files

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{error, info};

use crate::Tool;
use crate::ToolRegistry;

// Helper to execute git commands
async fn execute_git(args: &[&str], working_dir: &str) -> Result<Value> {
    info!(args = ?args, dir = %working_dir, "Executing git command");

    let mut child = Command::new("git")
        .args(args)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn git: {}", e))?;

    let mut stdout = String::new();
    let mut stderr = String::new();

    if let Some(mut stdout_pipe) = child.stdout.take() {
        stdout_pipe
            .read_to_string(&mut stdout)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read stdout: {}", e))?;
    }

    if let Some(mut stderr_pipe) = child.stderr.take() {
        stderr_pipe
            .read_to_string(&mut stderr)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read stderr: {}", e))?;
    }

    let status = child
        .wait()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wait for git: {}", e))?;

    let exit_code = status.code().unwrap_or(-1);

    if exit_code != 0 {
        error!(args = ?args, exit_code = %exit_code, stderr = %stderr, "Git command failed");
        return Err(anyhow::anyhow!("Git command failed ({}): {}", exit_code, stderr));
    }

    Ok(json!({
        "stdout": stdout,
        "stderr": stderr,
        "exit_code": exit_code
    }))
}

// ============================================================================
// GIT STATUS
// ============================================================================

pub struct GitStatusTool;

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Show the working tree status"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "working_dir": {
                    "type": "string",
                    "description": "Repository root directory",
                    "default": "/home/jeremy/git/op-dbus-v2"
                }
            }
        })
    }

    fn category(&self) -> &str {
        "development"
    }

    fn tags(&self) -> Vec<String> {
        vec!["git".to_string(), "status".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/home/jeremy/git/op-dbus-v2");

        execute_git(&["status"], working_dir).await
    }
}

// ============================================================================
// GIT ADD
// ============================================================================

pub struct GitAddTool;

#[async_trait]
impl Tool for GitAddTool {
    fn name(&self) -> &str {
        "git_add"
    }

    fn description(&self) -> &str {
        "Add file contents to the index"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of files to add (use '.' for all)"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Repository root directory",
                    "default": "/home/jeremy/git/op-dbus-v2"
                }
            },
            "required": ["files"]
        })
    }

    fn category(&self) -> &str {
        "development"
    }

    fn tags(&self) -> Vec<String> {
        vec!["git".to_string(), "add".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/home/jeremy/git/op-dbus-v2");

        let files = input
            .get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: files"))?;

        let mut args = vec!["add"];
        for file in files {
            if let Some(f) = file.as_str() {
                args.push(f);
            }
        }

        execute_git(&args, working_dir).await
    }
}

// ============================================================================
// GIT COMMIT
// ============================================================================

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Record changes to the repository"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Commit message"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Repository root directory",
                    "default": "/home/jeremy/git/op-dbus-v2"
                }
            },
            "required": ["message"]
        })
    }

    fn category(&self) -> &str {
        "development"
    }

    fn tags(&self) -> Vec<String> {
        vec!["git".to_string(), "commit".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/home/jeremy/git/op-dbus-v2");

        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;

        execute_git(&["commit", "-m", message], working_dir).await
    }
}

// ============================================================================
// GIT PUSH
// ============================================================================

pub struct GitPushTool;

#[async_trait]
impl Tool for GitPushTool {
    fn name(&self) -> &str {
        "git_push"
    }

    fn description(&self) -> &str {
        "Update remote refs along with associated objects"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "remote": {
                    "type": "string",
                    "description": "Remote name (default: origin)",
                    "default": "origin"
                },
                "branch": {
                    "type": "string",
                    "description": "Branch name (default: current branch)"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Repository root directory",
                    "default": "/home/jeremy/git/op-dbus-v2"
                }
            }
        })
    }

    fn category(&self) -> &str {
        "development"
    }

    fn tags(&self) -> Vec<String> {
        vec!["git".to_string(), "push".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/home/jeremy/git/op-dbus-v2");

        let remote = input
            .get("remote")
            .and_then(|v| v.as_str())
            .unwrap_or("origin");

        let mut args = vec!["push", remote];
        
        if let Some(branch) = input.get("branch").and_then(|v| v.as_str()) {
            args.push(branch);
        }

        execute_git(&args, working_dir).await
    }
}

// ============================================================================
// GIT DIFF
// ============================================================================

pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show changes between commits, commit and working tree, etc"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Arguments for git diff (e.g. ['HEAD^', 'HEAD'])"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Repository root directory",
                    "default": "/home/jeremy/git/op-dbus-v2"
                }
            }
        })
    }

    fn category(&self) -> &str {
        "development"
    }

    fn tags(&self) -> Vec<String> {
        vec!["git".to_string(), "diff".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/home/jeremy/git/op-dbus-v2");

        let mut args = vec!["diff"];
        
        if let Some(extra_args) = input.get("args").and_then(|v| v.as_array()) {
            for arg in extra_args {
                if let Some(a) = arg.as_str() {
                    args.push(a);
                }
            }
        }

        execute_git(&args, working_dir).await
    }
}

// ============================================================================
// GIT LOG
// ============================================================================

pub struct GitLogTool;

#[async_trait]
impl Tool for GitLogTool {
    fn name(&self) -> &str {
        "git_log"
    }

    fn description(&self) -> &str {
        "Show commit logs"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "max_count": {
                    "type": "integer",
                    "description": "Maximum number of commits to show (default: 10)",
                    "default": 10
                },
                "working_dir": {
                    "type": "string",
                    "description": "Repository root directory",
                    "default": "/home/jeremy/git/op-dbus-v2"
                }
            }
        })
    }

    fn category(&self) -> &str {
        "development"
    }

    fn tags(&self) -> Vec<String> {
        vec!["git".to_string(), "log".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/home/jeremy/git/op-dbus-v2");

        let max_count = input
            .get("max_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .to_string();

        execute_git(&["log", "-n", &max_count, "--oneline"], working_dir).await
    }
}

// ============================================================================
// REGISTRATION
// ============================================================================

pub async fn register_git_tools(registry: &ToolRegistry) -> Result<()> {
    use std::sync::Arc;

    registry.register_tool(Arc::new(GitStatusTool)).await?;
    registry.register_tool(Arc::new(GitAddTool)).await?;
    registry.register_tool(Arc::new(GitCommitTool)).await?;
    registry.register_tool(Arc::new(GitPushTool)).await?;
    registry.register_tool(Arc::new(GitDiffTool)).await?;
    registry.register_tool(Arc::new(GitLogTool)).await?;

    Ok(())
}
