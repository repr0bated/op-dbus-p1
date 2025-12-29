//! Shell execution tool (MCP-only registration).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::{Tool, ToolRegistry};

const FORBIDDEN_CLI_TOOLS: &[&str] = &[
    "ovs-vsctl",
    "ovs-ofctl",
    "ovs-dpctl",
    "ovs-appctl",
    "ovsdb-client",
    "systemctl",
    "service",
    "ip ",
    "ip\t",
    "ifconfig",
    "nmcli",
    "brctl",
    "apt ",
    "apt\t",
    "apt-get",
    "dnf",
    "yum",
];

fn deny_forbidden_cli(command: &str) -> Result<()> {
    let normalized = format!(" {} ", command.replace('\n', " ").replace('\t', " "));
    for token in FORBIDDEN_CLI_TOOLS {
        let needle = format!(" {} ", token);
        if normalized.contains(&needle) {
            return Err(anyhow::anyhow!(
                "Forbidden CLI tool detected: {} (use native D-Bus/JSON-RPC tools instead)",
                token.trim()
            ));
        }
    }
    Ok(())
}

pub struct ShellExecuteTool;

#[async_trait]
impl Tool for ShellExecuteTool {
    fn name(&self) -> &str {
        "shell_execute"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return stdout/stderr. MCP-only tool."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 300)",
                    "default": 30
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory (default: /tmp)",
                    "default": "/tmp"
                }
            },
            "required": ["command"]
        })
    }

    fn category(&self) -> &str {
        "system"
    }

    fn tags(&self) -> Vec<String> {
        vec!["shell".to_string(), "bash".to_string(), "command".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;
        deny_forbidden_cli(command)?;

        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .min(300);

        let working_dir = input
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/tmp");

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            execute_command(command, working_dir),
        )
        .await;

        match result {
            Ok(Ok((stdout, stderr, exit_code))) => Ok(json!({
                "command": command,
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "success": exit_code == 0
            })),
            Ok(Err(e)) => Err(anyhow::anyhow!("Command execution failed: {}", e)),
            Err(_) => Err(anyhow::anyhow!("Command timed out after {} seconds", timeout_secs)),
        }
    }
}

pub struct ShellExecuteBatchTool;

#[async_trait]
impl Tool for ShellExecuteBatchTool {
    fn name(&self) -> &str {
        "shell_execute_batch"
    }

    fn description(&self) -> &str {
        "Execute a sequence of shell commands in order. MCP-only tool."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "commands": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string" },
                            "working_dir": { "type": "string" },
                            "timeout_secs": { "type": "integer" }
                        },
                        "required": ["command"]
                    },
                    "description": "Ordered list of commands to execute"
                },
                "stop_on_error": {
                    "type": "boolean",
                    "description": "Stop after first non-zero exit or error",
                    "default": true
                },
                "default_working_dir": {
                    "type": "string",
                    "description": "Default working directory for commands",
                    "default": "/tmp"
                },
                "default_timeout_secs": {
                    "type": "integer",
                    "description": "Default timeout in seconds (max 300)",
                    "default": 30
                }
            },
            "required": ["commands"]
        })
    }

    fn category(&self) -> &str {
        "system"
    }

    fn tags(&self) -> Vec<String> {
        vec!["shell".to_string(), "batch".to_string(), "bash".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let commands = input
            .get("commands")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: commands"))?;

        if commands.is_empty() {
            return Err(anyhow::anyhow!("commands must be a non-empty array"));
        }

        let stop_on_error = input
            .get("stop_on_error")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let default_working_dir = input
            .get("default_working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/tmp");

        let default_timeout_secs = input
            .get("default_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .min(300);

        let mut results = Vec::new();

        for entry in commands {
            let command = entry
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Each command entry requires 'command'"))?;

            let working_dir = entry
                .get("working_dir")
                .and_then(|v| v.as_str())
                .unwrap_or(default_working_dir);

            let timeout_secs = entry
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(default_timeout_secs)
                .min(300);

            let run = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                execute_command(command, working_dir),
            )
            .await;

            let outcome = match run {
                Ok(Ok((stdout, stderr, exit_code))) => json!({
                    "command": command,
                    "working_dir": working_dir,
                    "exit_code": exit_code,
                    "stdout": stdout,
                    "stderr": stderr,
                    "success": exit_code == 0
                }),
                Ok(Err(e)) => json!({
                    "command": command,
                    "working_dir": working_dir,
                    "exit_code": -1,
                    "stdout": "",
                    "stderr": e,
                    "success": false
                }),
                Err(_) => json!({
                    "command": command,
                    "working_dir": working_dir,
                    "exit_code": -1,
                    "stdout": "",
                    "stderr": format!("Command timed out after {} seconds", timeout_secs),
                    "success": false
                }),
            };

            let success = outcome
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            results.push(outcome);

            if stop_on_error && !success {
                break;
            }
        }

        Ok(json!({
            "results": results,
            "stopped_early": stop_on_error && results.last().and_then(|r| r.get("success")).and_then(|v| v.as_bool()) == Some(false)
        }))
    }
}

pub struct ShellExecuteAutoTool;

#[async_trait]
impl Tool for ShellExecuteAutoTool {
    fn name(&self) -> &str {
        "shell_execute_auto"
    }

    fn description(&self) -> &str {
        "Execute a shell command or a newline-delimited batch. MCP-only tool."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command or newline-delimited commands"
                },
                "commands": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string" },
                            "working_dir": { "type": "string" },
                            "timeout_secs": { "type": "integer" }
                        },
                        "required": ["command"]
                    },
                    "description": "Explicit list of commands to execute"
                },
                "stop_on_error": {
                    "type": "boolean",
                    "description": "Stop after first non-zero exit or error",
                    "default": true
                },
                "default_working_dir": {
                    "type": "string",
                    "description": "Default working directory for commands",
                    "default": "/tmp"
                },
                "default_timeout_secs": {
                    "type": "integer",
                    "description": "Default timeout in seconds (max 300)",
                    "default": 30
                }
            },
            "required": []
        })
    }

    fn category(&self) -> &str {
        "system"
    }

    fn tags(&self) -> Vec<String> {
        vec!["shell".to_string(), "auto".to_string(), "batch".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let default_working_dir = input
            .get("default_working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/tmp");

        let default_timeout_secs = input
            .get("default_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .min(300);

        let stop_on_error = input
            .get("stop_on_error")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if let Some(commands) = input.get("commands").and_then(|v| v.as_array()) {
            if commands.is_empty() {
                return Err(anyhow::anyhow!("commands must be a non-empty array"));
            }
            return run_batch(commands, stop_on_error, default_working_dir, default_timeout_secs).await;
        }

        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing command or commands"))?;
        deny_forbidden_cli(command)?;

        let lines: Vec<String> = command
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .collect();

        if lines.len() <= 1 {
            let timeout_secs = input
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(default_timeout_secs)
                .min(300);
            let working_dir = input
                .get("working_dir")
                .and_then(|v| v.as_str())
                .unwrap_or(default_working_dir);

            let run = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                execute_command(command.trim(), working_dir),
            )
            .await;

            return match run {
                Ok(Ok((stdout, stderr, exit_code))) => Ok(json!({
                    "command": command.trim(),
                    "exit_code": exit_code,
                    "stdout": stdout,
                    "stderr": stderr,
                    "success": exit_code == 0
                })),
                Ok(Err(e)) => Err(anyhow::anyhow!("Command execution failed: {}", e)),
                Err(_) => Err(anyhow::anyhow!("Command timed out after {} seconds", timeout_secs)),
            };
        }

        let batch = lines
            .into_iter()
            .map(|line| json!({ "command": line }))
            .collect::<Vec<_>>();

        run_batch(&batch, stop_on_error, default_working_dir, default_timeout_secs).await
    }
}

async fn run_batch(
    commands: &[Value],
    stop_on_error: bool,
    default_working_dir: &str,
    default_timeout_secs: u64,
) -> Result<Value> {
    let mut results = Vec::new();

    for entry in commands {
        let command = entry
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Each command entry requires 'command'"))?;
        deny_forbidden_cli(command)?;

        let working_dir = entry
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or(default_working_dir);

        let timeout_secs = entry
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(default_timeout_secs)
            .min(300);

        let run = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            execute_command(command, working_dir),
        )
        .await;

        let outcome = match run {
            Ok(Ok((stdout, stderr, exit_code))) => json!({
                "command": command,
                "working_dir": working_dir,
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "success": exit_code == 0
            }),
            Ok(Err(e)) => json!({
                "command": command,
                "working_dir": working_dir,
                "exit_code": -1,
                "stdout": "",
                "stderr": e,
                "success": false
            }),
            Err(_) => json!({
                "command": command,
                "working_dir": working_dir,
                "exit_code": -1,
                "stdout": "",
                "stderr": format!("Command timed out after {} seconds", timeout_secs),
                "success": false
            }),
        };

        let success = outcome
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        results.push(outcome);

        if stop_on_error && !success {
            break;
        }
    }

    Ok(json!({
        "results": results,
        "stopped_early": stop_on_error && results.last().and_then(|r| r.get("success")).and_then(|v| v.as_bool()) == Some(false)
    }))
}
async fn execute_command(
    command: &str,
    working_dir: &str,
) -> Result<(String, String, i32), String> {
    let mut child = Command::new("bash")
        .arg("-c")
        .arg(command)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn command: {}", e))?;

    let mut stdout = String::new();
    let mut stderr = String::new();

    if let Some(mut stdout_pipe) = child.stdout.take() {
        stdout_pipe
            .read_to_string(&mut stdout)
            .await
            .map_err(|e| format!("Failed to read stdout: {}", e))?;
    }

    if let Some(mut stderr_pipe) = child.stderr.take() {
        stderr_pipe
            .read_to_string(&mut stderr)
            .await
            .map_err(|e| format!("Failed to read stderr: {}", e))?;
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for command: {}", e))?;

    let exit_code = status.code().unwrap_or(-1);

    let max_output = 50000;
    if stdout.len() > max_output {
        stdout.truncate(max_output);
        stdout.push_str("\n... (output truncated)");
    }
    if stderr.len() > max_output {
        stderr.truncate(max_output);
        stderr.push_str("\n... (output truncated)");
    }

    Ok((stdout, stderr, exit_code))
}

pub async fn register_shell_tools(registry: &ToolRegistry) -> Result<()> {
    registry.register_tool(std::sync::Arc::new(ShellExecuteTool)).await?;
    registry.register_tool(std::sync::Arc::new(ShellExecuteBatchTool)).await?;
    registry.register_tool(std::sync::Arc::new(ShellExecuteAutoTool)).await?;
    Ok(())
}
