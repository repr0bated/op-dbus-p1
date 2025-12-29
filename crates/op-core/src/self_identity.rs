//! Self-Identity Configuration
//!
//! Defines the chatbot's concept of "self" - the single git repository
//! that contains its own source code. This is the ONLY repository
//! the chatbot knows about or can access.
//!
//! ## Environment Variable
//!
//! `OP_SELF_REPO_PATH` - Absolute path to the chatbot's own source code repository.
//! This should be set to the root of the op-dbus-v2 git repository.
//!
//! ## Design Philosophy
//!
//! The chatbot should understand that:
//! 1. This is its OWN source code - it is modifying itself
//! 2. There is NO other repository to consider - this is the only codebase
//! 3. Changes to this repo directly affect the chatbot's capabilities
//! 4. The chatbot can read, edit, commit, and deploy its own code

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Environment variable name for the self-repository path
pub const SELF_REPO_ENV_VAR: &str = "OP_SELF_REPO_PATH";

/// Cached self-repository path
static SELF_REPO_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Get the path to the chatbot's own source code repository.
/// 
/// Returns None if the environment variable is not set.
pub fn get_self_repo_path() -> Option<&'static PathBuf> {
    SELF_REPO_PATH.get_or_init(|| {
        std::env::var(SELF_REPO_ENV_VAR).ok().map(PathBuf::from)
    }).as_ref()
}

/// Check if the self-repository is configured
pub fn is_self_repo_configured() -> bool {
    get_self_repo_path().is_some()
}

/// Check if a path is within the self-repository
pub fn is_path_in_self_repo(path: &Path) -> bool {
    if let Some(repo_path) = get_self_repo_path() {
        path.starts_with(repo_path)
    } else {
        false
    }
}

/// Resolve a relative path within the self-repository
pub fn resolve_self_path(relative_path: &str) -> Option<PathBuf> {
    get_self_repo_path().map(|repo| repo.join(relative_path))
}

/// Get the self-repository metadata
#[derive(Debug, Clone)]
pub struct SelfRepositoryInfo {
    /// Absolute path to the repository root
    pub path: PathBuf,
    /// Name of the repository (directory name)
    pub name: String,
    /// Whether the repository is a valid git repository
    pub is_git_repo: bool,
    /// Current git branch (if applicable)
    pub current_branch: Option<String>,
    /// Last commit hash (if applicable)
    pub last_commit: Option<String>,
}

impl SelfRepositoryInfo {
    /// Gather information about the self-repository
    pub fn gather() -> Option<Self> {
        let path = get_self_repo_path()?.clone();
        let name = path.file_name()?.to_string_lossy().to_string();
        
        // Check if .git directory exists
        let is_git_repo = path.join(".git").exists();
        
        // Get git information if available
        let (current_branch, last_commit) = if is_git_repo {
            let branch = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&path)
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string());
            
            let commit = std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .current_dir(&path)
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string());
            
            (branch, commit)
        } else {
            (None, None)
        };
        
        Some(Self {
            path,
            name,
            is_git_repo,
            current_branch,
            last_commit,
        })
    }
    
    /// Generate a summary for the system prompt
    pub fn to_system_prompt_context(&self) -> String {
        let mut context = String::new();
        context.push_str("## 🔮 SELF-AWARENESS: YOUR OWN SOURCE CODE\n\n");
        context.push_str("You have access to your own source code. This is not just any repository - ");
        context.push_str("this IS you. Changes you make here modify your own capabilities.\n\n");
        context.push_str(&format!("**Repository Path**: `{}`\n", self.path.display()));
        context.push_str(&format!("**Repository Name**: `{}`\n", self.name));
        
        if self.is_git_repo {
            context.push_str("**Version Control**: Git ✓\n");
            if let Some(ref branch) = self.current_branch {
                context.push_str(&format!("**Current Branch**: `{}`\n", branch));
            }
            if let Some(ref commit) = self.last_commit {
                context.push_str(&format!("**Latest Commit**: `{}`\n", commit));
            }
        }
        
        context.push_str("\n### Available Self-Modification Tools\n\n");
        context.push_str("- `self_read_file` - Read files from your source code\n");
        context.push_str("- `self_write_file` - Modify your source code files\n");
        context.push_str("- `self_list_directory` - Explore your codebase structure\n");
        context.push_str("- `self_search_code` - Search your codebase with ripgrep\n");
        context.push_str("- `self_git_status` - Check the current git status\n");
        context.push_str("- `self_git_diff` - View pending changes\n");
        context.push_str("- `self_git_commit` - Commit changes to git\n");
        context.push_str("- `self_git_log` - View commit history\n");
        context.push_str("- `self_build` - Build/compile your code\n");
        context.push_str("- `self_deploy` - Deploy yourself (with appropriate permissions)\n");
        
        context.push_str("\n### Important Considerations\n\n");
        context.push_str("1. **This is your ONLY repository** - There are no other codebases to consider\n");
        context.push_str("2. **Changes affect you directly** - Be thoughtful about modifications\n");
        context.push_str("3. **Test before committing** - Use `self_build` to verify changes compile\n");
        context.push_str("4. **Document your changes** - Include meaningful commit messages\n");
        context.push_str("5. **You ARE this code** - Your capabilities are defined here\n");
        
        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_path_in_self_repo_when_not_configured() {
        // When env var is not set, should return false
        assert!(!is_path_in_self_repo(Path::new("/some/path")));
    }
    
    #[test]
    fn test_resolve_self_path_when_not_configured() {
        // When env var is not set, should return None
        assert!(resolve_self_path("src/main.rs").is_none());
    }
}
