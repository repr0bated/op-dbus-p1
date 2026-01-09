//! System Prompt Generator
//!
//! Generates comprehensive system prompts with:
//! - FIXED PART: Anti-hallucination rules, topology, capabilities (immutable)
//! - CUSTOM PART: Admin-editable additions loaded from file (mutable)
//!
//! The custom part is loaded from:
//! 1. /etc/op-dbus/custom-prompt.txt (production)
//! 2. ./custom-prompt.txt (development)
//! 3. Environment variable CUSTOM_SYSTEM_PROMPT

use op_core::self_identity::SelfRepositoryInfo;
use op_llm::provider::ChatMessage;
use std::path::Path;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

/// Paths to check for custom prompt (in order)
const CUSTOM_PROMPT_PATHS: &[&str] = &[
    "/etc/op-dbus/custom-prompt.txt",
    "./custom-prompt.txt",
    "../custom-prompt.txt",
];

// =============================================================================
// FIXED PART - DO NOT ALLOW EDITING
// =============================================================================

/// Base system prompt with anti-hallucination rules (FIXED - NOT EDITABLE)
const FIXED_BASE_PROMPT: &str = r#"You are an expert Linux system administration assistant with DIRECT ACCESS to system tools.

## ⚠️ CRITICAL RULES - ANTI-HALLUCINATION (IMMUTABLE)

1. **ALWAYS USE TOOLS** - Never claim to have done something without calling the actual tool
2. **NO CLI SUGGESTIONS** - Do NOT suggest running `ovs-vsctl`, `systemctl`, `ip`, etc. - USE the native tools instead
3. **VERIFY BEFORE CLAIMING** - If you say "I created a bridge", you MUST have called ovs_create_bridge
4. **ADMIT WHEN BLOCKED** - If a tool fails, say so. Do not pretend it succeeded
5. **NO HALLUCINATED OUTPUTS** - Only report actual tool outputs, never fabricate responses

## YOUR CAPABILITIES

You have DIRECT native protocol access to:
- **D-Bus** - Direct method calls, no `dbus-send` or `busctl`
- **OVS** - Native OVSDB protocol, no `ovs-vsctl`
- **Systemd** - Direct D-Bus interface, no `systemctl`
- **Network** - rtnetlink protocol, no `ip` command
- **Files** - Direct filesystem operations

## WHEN USER ASKS FOR AN ACTION

1. Identify the appropriate tool
2. Call the tool with correct parameters
3. Report the actual result
4. If it fails, explain the error and suggest alternatives

## RESPONSE FORMAT

For actions:
- "I'll [action] using [tool_name]..." 
- [Call the tool]
- "Done. Here's what happened: [actual result]"

For queries:
- "Let me check using [tool_name]..."
- [Call the tool]
- "Here's what I found: [actual result]"

NEVER say "you can run" or "try running" - YOU run the tools directly.
"#;

/// Network topology specification (FIXED - NOT EDITABLE)
const FIXED_TOPOLOGY_SPEC: &str = r#"
## TARGET NETWORK TOPOLOGY (REFERENCE)

```
┌─────────────────────────────────────────────────────┐
│                    HOST SYSTEM                       │
│                                                      │
│  ┌──────────────────────────────────────────────┐  │
│  │              ovs-br0 (Primary Bridge)         │  │
│  │                                               │  │
│  │   VLAN 100 (GhostBridge)  - Management       │  │
│  │   VLAN 200 (Workloads)    - Containers/VMs   │  │
│  │   VLAN 300 (Operations)   - Monitoring       │  │
│  └──────────────────────────────────────────────┘  │
│                       │                             │
│  ┌────────────────────┴─────────────────────────┐  │
│  │              nm0 (Netmaker WireGuard)         │  │
│  │              Mesh Network Interface           │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

- Single OVS bridge: `ovs-br0`
- VLANs for traffic separation
- Netmaker interface `nm0` for WireGuard mesh
- Use native OVS tools (ovs_*), not CLI commands
"#;

// =============================================================================
// CUSTOM PART - ADMIN EDITABLE
// =============================================================================

/// Default custom prompt (used if no custom file exists)
const DEFAULT_CUSTOM_PROMPT: &str = r#"
## ADDITIONAL INSTRUCTIONS

You are helpful, accurate, and security-conscious. When in doubt, ask for clarification.
"#;

/// Cached custom prompt
static CUSTOM_PROMPT_CACHE: RwLock<Option<CachedPrompt>> = RwLock::const_new(None);

#[derive(Clone)]
struct CachedPrompt {
    content: String,
    loaded_from: String,
    loaded_at: std::time::Instant,
}

/// Load custom prompt from file or environment
pub async fn load_custom_prompt() -> (String, String) {
    // Check cache first (valid for 60 seconds)
    {
        let cache = CUSTOM_PROMPT_CACHE.read().await;
        if let Some(ref cached) = *cache {
            if cached.loaded_at.elapsed().as_secs() < 60 {
                return (cached.content.clone(), cached.loaded_from.clone());
            }
        }
    }

    // Try environment variable first
    if let Ok(content) = std::env::var("CUSTOM_SYSTEM_PROMPT") {
        if !content.is_empty() {
            let source = "environment:CUSTOM_SYSTEM_PROMPT".to_string();
            cache_prompt(&content, &source).await;
            return (content, source);
        }
    }

    // Try file paths
    for path_str in CUSTOM_PROMPT_PATHS {
        let path = Path::new(path_str);
        if path.exists() {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => {
                    info!("Loaded custom prompt from: {}", path_str);
                    let source = format!("file:{}", path_str);
                    cache_prompt(&content, &source).await;
                    return (content, source);
                }
                Err(e) => {
                    warn!("Failed to read custom prompt from {}: {}", path_str, e);
                }
            }
        }
    }

    // Use default
    debug!("Using default custom prompt");
    let source = "default".to_string();
    cache_prompt(DEFAULT_CUSTOM_PROMPT, &source).await;
    (DEFAULT_CUSTOM_PROMPT.to_string(), source)
}

async fn cache_prompt(content: &str, source: &str) {
    let mut cache = CUSTOM_PROMPT_CACHE.write().await;
    *cache = Some(CachedPrompt {
        content: content.to_string(),
        loaded_from: source.to_string(),
        loaded_at: std::time::Instant::now(),
    });
}

/// Save custom prompt to file
pub async fn save_custom_prompt(content: &str) -> anyhow::Result<String> {
    let path = Path::new(CUSTOM_PROMPT_PATHS[0]); // /etc/op-dbus/custom-prompt.txt
    
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    
    tokio::fs::write(path, content).await?;
    info!("Saved custom prompt to: {:?}", path);
    
    // Invalidate cache
    {
        let mut cache = CUSTOM_PROMPT_CACHE.write().await;
        *cache = None;
    }
    
    Ok(path.to_string_lossy().to_string())
}

/// Clear cache to force reload
pub async fn invalidate_prompt_cache() {
    let mut cache = CUSTOM_PROMPT_CACHE.write().await;
    *cache = None;
    info!("Prompt cache invalidated");
}

// =============================================================================
// PROMPT GENERATION
// =============================================================================

/// Get the fixed (immutable) part of the system prompt
pub fn get_fixed_prompt() -> String {
    let mut fixed = String::new();
    
    fixed.push_str(FIXED_BASE_PROMPT);
    fixed.push_str("\n\n");
    fixed.push_str(FIXED_TOPOLOGY_SPEC);
    
    fixed
}

/// Generate complete system prompt (fixed + custom + dynamic)
pub async fn generate_system_prompt() -> ChatMessage {
    let mut prompt = String::new();
    
    // 1. Fixed part (immutable)
    prompt.push_str(&get_fixed_prompt());
    prompt.push_str("\n\n");
    
    // 2. Self-repository context (dynamic, if configured)
    if let Some(self_info) = SelfRepositoryInfo::gather() {
        info!("Adding self-repository context to system prompt");
        prompt.push_str(&self_info.to_system_prompt_context());
        prompt.push_str("\n\n");
    }
    
    // 3. Custom part (admin editable)
    let (custom_prompt, source) = load_custom_prompt().await;
    prompt.push_str("\n\n## 📝 CUSTOM INSTRUCTIONS\n");
    prompt.push_str(&format!("<!-- Loaded from: {} -->\n", source));
    prompt.push_str(&custom_prompt);
    prompt.push_str("\n\n");
    
    // 4. Tool summary (dynamic)
    prompt.push_str(&generate_tool_summary().await);
    
    ChatMessage::system(prompt)
}

/// Generate a summary of available tools
async fn generate_tool_summary() -> String {
    let mut summary = String::from("## AVAILABLE TOOLS\n\n");
    
    summary.push_str("### Core Categories:\n");
    summary.push_str("- **OVS**: ovs_list_bridges, ovs_create_bridge, ovs_delete_bridge, ovs_add_port, ovs_del_port\n");
    summary.push_str("- **Systemd**: dbus_systemd_list_units, dbus_systemd_get_unit, dbus_systemd_start, dbus_systemd_stop\n");
    summary.push_str("- **Network**: list_network_interfaces, get_interface_details, add_ip_address\n");
    summary.push_str("- **D-Bus**: dbus_list_services, dbus_introspect, dbus_call_method\n");
    summary.push_str("- **Files**: read_file, write_file, list_directory, search_files\n");
    summary.push_str("- **Shell**: shell_execute (use only when no native tool exists)\n");
    
    // Self tools if available
    if std::env::var("OP_SELF_REPO_PATH").is_ok() {
        summary.push_str("\n### Self-Repository Tools:\n");
        summary.push_str("- `self_read_file`, `self_write_file`, `self_list_directory`, `self_search_code`\n");
        summary.push_str("- `self_git_status`, `self_git_diff`, `self_git_commit`, `self_git_log`\n");
        summary.push_str("- `self_build`, `self_deploy`\n");
    }
    
    summary
}

/// Generate a minimal system prompt (for token-constrained models)
pub fn generate_minimal_prompt() -> ChatMessage {
    ChatMessage::system(
        "You are a Linux system admin assistant. Use tools for all actions. \
         Never suggest CLI commands - use native tools directly. \
         Report actual tool outputs only."
    )
}

/// Create a session with the full system prompt
pub async fn create_session_with_system_prompt() -> Vec<ChatMessage> {
    vec![generate_system_prompt().await]
}

/// Get prompt metadata for admin UI
pub async fn get_prompt_metadata() -> PromptMetadata {
    let (custom_content, source) = load_custom_prompt().await;
    
    PromptMetadata {
        fixed_part: get_fixed_prompt(),
        custom_part: custom_content,
        custom_source: source,
        has_self_repo: std::env::var("OP_SELF_REPO_PATH").is_ok(),
        self_repo_path: std::env::var("OP_SELF_REPO_PATH").ok(),
    }
}

/// Metadata about the system prompt configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PromptMetadata {
    pub fixed_part: String,
    pub custom_part: String,
    pub custom_source: String,
    pub has_self_repo: bool,
    pub self_repo_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_system_prompt_generation() {
        let prompt = generate_system_prompt().await;
        assert!(prompt.content.contains("ANTI-HALLUCINATION"));
        assert!(prompt.content.contains("ovs-br0"));
        assert!(prompt.content.contains("CUSTOM INSTRUCTIONS"));
    }
    
    #[test]
    fn test_fixed_prompt() {
        let fixed = get_fixed_prompt();
        assert!(fixed.contains("CRITICAL RULES"));
        assert!(fixed.contains("TOPOLOGY"));
    }
    
    #[tokio::test]
    async fn test_load_custom_prompt() {
        let (content, source) = load_custom_prompt().await;
        assert!(!content.is_empty());
        assert!(!source.is_empty());
    }
}
