# Fixing .ayeignore for Aye Chat

## Problem

Aye Chat indexes all files by default. Without `.ayeignore`, it tries to read:
- `target/` (gigabytes of build artifacts)
- `package-lock.json` (800+ lines of noise)
- Binary files
- etc.

This causes:
1. Slow indexing
2. Context pollution
3. Token waste

## Solution

Create `.ayeignore` in project root (same location as `Cargo.toml`):

```bash
# The file should already exist after running this chat
cat .ayeignore
```

## Verify It's Working

```bash
# Restart Aye Chat in the project
cd /home/jeremy/git/op-dbus-v2
aye chat

# Check if it mentions skipping files
# Or ask: "How many files are you tracking?"
```

## Common Patterns

| Pattern | What it ignores |
|---------|----------------|
| `target/` | Rust build directory |
| `node_modules/` | Node.js dependencies |
| `*.lock` | Lock files (Cargo.lock, etc.) |
| `package-lock.json` | NPM lock (explicitly) |
| `.git/` | Git internal data |
| `*.log` | Log files |

## Syntax

Same as `.gitignore`:

```
# Comment
target/          # Directory anywhere
/target/         # Directory at root only  
*.log            # All .log files
!important.log   # Except this one
data/**/*.csv    # Nested pattern
```

## If Still Not Working

1. **Check file location**: Must be in project root
2. **Check file name**: Must be exactly `.ayeignore` (with dot)
3. **Restart Aye Chat**: Changes require restart
4. **Check permissions**: `ls -la .ayeignore`

```bash
# Verify file exists and has content
ls -la .ayeignore
head -20 .ayeignore

# Restart aye chat
exit
aye chat
```

## For This Project Specifically

The main offenders are:
- `target/` - Rust builds (GB of data)
- `package-lock.json` - 800+ lines, no value
- `node_modules/` - If using any JS tooling
- `.git/` - Git internals

After adding `.ayeignore`, indexing should be much faster.
