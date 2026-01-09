# .ayeignore Guide for op-dbus

## What is .ayeignore?

The `.ayeignore` file tells Aye Chat which files and directories to skip when building context for AI conversations. This keeps the context focused on relevant source code.

## Why It Matters

1. **Token Efficiency**: AI models have context limits. Ignoring noise means more room for important code.
2. **Relevance**: Build artifacts, logs, and generated files add noise without value.
3. **Security**: Prevents accidental exposure of secrets and credentials.
4. **Speed**: Fewer files to scan means faster context loading.

## What's Ignored

### Build Artifacts
- `target/` - Rust compilation output
- `node_modules/` - Node.js dependencies
- `*.pyc`, `__pycache__/` - Python bytecode

### Sensitive Files
- `.env*` - Environment variables
- `secrets.env` - Secret configuration
- `*.pem`, `*.key` - Certificates and keys

### Large/Binary Files
- `*.db`, `*.sqlite` - Databases
- `*.zip`, `*.tar.gz` - Archives
- `*.png`, `*.jpg` - Images

### IDE/Editor
- `.vscode/`, `.idea/` - IDE settings
- `*.swp`, `*~` - Editor temp files

### Project-Specific
- `archived/` - Old code
- `streaming-logs/` - Log streaming crate (internal)
- `op-chat-ui/` - npm-based UI (not authoritative)

## Syntax

```gitignore
# Comment
pattern           # Ignore files matching pattern
dir/              # Ignore directory
*.ext             # Wildcard for extension
!important.file   # Un-ignore (override previous ignore)
```

## Customizing

Edit `.ayeignore` to:

1. **Add project-specific patterns**:
   ```
   my-large-data/
   generated-reports/
   ```

2. **Un-ignore important files**:
   ```
   !config/example.env
   ```

3. **Ignore specific crates during focused work**:
   ```
   # Temporarily ignore to focus on op-web
   crates/op-blockchain/
   crates/op-cache/
   ```

## Verifying

To see what Aye Chat will read:

```bash
# List files that would be included
aye chat --root . --dry-run
```

## Best Practices

1. **Keep source code**: Never ignore `*.rs`, `*.toml`, core documentation
2. **Ignore generated**: Proto files, build output, coverage reports
3. **Protect secrets**: Always ignore `.env`, credentials, keys
4. **Review periodically**: Update as project structure changes

## op-dbus Specific Notes

### Keep These
- `crates/*/src/**/*.rs` - All Rust source
- `Cargo.toml` - Workspace and crate configs
- `docs/*.md` - Documentation (except very large files)
- `deploy/*.service` - Systemd services
- `scripts/*.sh` - Helper scripts

### Ignore These
- `target/` - Build output (huge)
- `*.db` - SQLite databases
- `node_modules/` - If any npm packages installed
- `.aye/snapshots/` - Snapshot data (verbose)

### Temporarily Ignore for Focused Work

When working on specific crates, temporarily ignore others:

```gitignore
# Focus on op-tools development
crates/op-blockchain/
crates/op-cache/
crates/op-inspector/
crates/op-state/
```

Remove these lines when done to restore full context.
