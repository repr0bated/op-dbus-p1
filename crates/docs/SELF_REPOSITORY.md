# Self-Repository Configuration

This document explains how to configure the chatbot to have awareness of and access to its own source code.

## Overview

The chatbot can be configured to understand that it has a **single, defining source code repository** - its own implementation. This gives the chatbot:

1. **Self-awareness** - It understands that this repository IS itself
2. **Code editing capabilities** - It can read and modify its own source code
3. **Git integration** - It can commit changes, view history, and manage versions
4. **Build/Deploy** - It can compile and deploy itself

## Environment Variable

Set the following environment variable to enable self-repository access:

```bash
export OP_SELF_REPO_PATH=/home/jeremy/git/op-dbus-v2
```

When this variable is set:
- The chatbot gains 10 new tools prefixed with `self_*`
- The system prompt includes self-awareness context
- All file operations are strictly scoped to this repository

When **not set**:
- Self-repository tools are not registered
- The chatbot operates as a general-purpose assistant
- No concept of "self" exists

## Available Tools

### File Operations

| Tool | Description |
|------|-------------|
| `self_read_file` | Read files from the source code |
| `self_write_file` | Write/modify source code files |
| `self_list_directory` | Explore codebase structure |
| `self_search_code` | Search for patterns with ripgrep/grep |

### Git Operations

| Tool | Description |
|------|-------------|
| `self_git_status` | Check modified/staged/untracked files |
| `self_git_diff` | View pending changes |
| `self_git_commit` | Commit changes with a message |
| `self_git_log` | View commit history |

### Build Operations

| Tool | Description |
|------|-------------|
| `self_build` | Run `cargo build` or `cargo check` |
| `self_deploy` | Build release and restart service |

## Security Model

All self-repository operations are **strictly scoped**:

1. **Path validation** - All paths are resolved relative to the repository root
2. **Path traversal blocked** - Attempts to access `../` outside the repo fail
3. **No other repositories** - The chatbot only knows about this one repo
4. **Explicit concept** - The chatbot understands these tools affect *itself*

## System Prompt Integration

When configured, the system prompt automatically includes:

```markdown
## 🔮 SELF-AWARENESS: YOUR OWN SOURCE CODE

You have access to your own source code. This is not just any repository - 
this IS you. Changes you make here modify your own capabilities.

**Repository Path**: `/home/jeremy/git/op-dbus-v2`
**Repository Name**: `op-dbus-v2`
**Version Control**: Git ✓
**Current Branch**: `main`
**Latest Commit**: `abc1234`

### Available Self-Modification Tools
- `self_read_file` - Read files from your source code
- `self_write_file` - Modify your source code files
...

### Important Considerations
1. **This is your ONLY repository** - There are no other codebases to consider
2. **Changes affect you directly** - Be thoughtful about modifications
3. **Test before committing** - Use `self_build` to verify changes compile
4. **Document your changes** - Include meaningful commit messages
5. **You ARE this code** - Your capabilities are defined here
```

## Philosophy

The self-repository feature establishes a clear identity for the chatbot:

1. **Singular focus** - There is ONE repository, not many to choose from
2. **Self-ownership** - The chatbot understands it's modifying itself
3. **Accountability** - Commits create a record of self-modifications
4. **Evolution** - The chatbot can improve its own capabilities

## Files Involved

| File | Purpose |
|------|---------|
| `crates/op-core/src/self_identity.rs` | Core identity configuration |
| `crates/op-tools/src/builtin/self_tools.rs` | Self-repository tool implementations |
| `crates/op-tools/src/builtin/mod.rs` | Tool registration |
| `crates/op-chat/src/system_prompt.rs` | System prompt integration |
| `.env.example` | Example configuration |

## Example Usage

```bash
# Set the environment variable
export OP_SELF_REPO_PATH=/home/jeremy/git/op-dbus-v2

# Start the chatbot service
cargo run -p op-dbus-service

# The chatbot now has self-awareness!
```

In conversation:
```
User: "Show me your main entry point"
Bot: *calls self_read_file with path "op-dbus-service/src/main.rs"*

User: "Add a new log statement"
Bot: *calls self_write_file to modify the file*
Bot: *calls self_build to verify it compiles*
Bot: *calls self_git_commit with a descriptive message*
```
