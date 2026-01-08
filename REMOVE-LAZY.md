# Remove Lazy Patterns

## Overview

Remove all `lazy_static!`, `once_cell::Lazy`, and `OnceCell` patterns from the codebase. Replace with eager initialization.

## Why

1. **Predictable startup** — All initialization happens at startup, failures are immediate
2. **No hidden costs** — No surprise latency on first access
3. **Simpler debugging** — State is explicit, not hidden in statics
4. **Thread safety** — Explicit `Arc<RwLock<T>>` is clearer than magic statics

## Files to Update

Based on the source files provided:

### 1. crates/op-agents/src/unified/registry.rs

**Current:**
```rust
use once_cell::sync::Lazy;

pub static GLOBAL_REGISTRY: Lazy<UnifiedAgentRegistry> = Lazy::new(UnifiedAgentRegistry::new);
```

**Replace with:**
```rust
// Remove global static entirely
// Pass registry explicitly where needed

pub struct UnifiedAgentRegistry {
    agents: RwLock<HashMap<String, Arc<dyn UnifiedAgent>>>,
    factories: HashMap<&'static str, fn() -> Box<dyn UnifiedAgent>>,
}

impl UnifiedAgentRegistry {
    /// Create eagerly at startup
    pub fn new() -> Self {
        let mut factories = HashMap::new();
        
        // Register all factories EAGERLY
        for (id, factory) in EXECUTION_AGENTS.iter() {
            factories.insert(*id, *factory);
        }
        for (id, factory) in PERSONA_AGENTS.iter() {
            factories.insert(*id, *factory);
        }
        for (id, factory) in ORCHESTRATION_AGENTS.iter() {
            factories.insert(*id, *factory);
        }

        Self {
            agents: RwLock::new(HashMap::new()),
            factories,
        }
    }
}

// In application startup:
pub struct App {
    registry: Arc<UnifiedAgentRegistry>,
}

impl App {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(UnifiedAgentRegistry::new()), // EAGER
        }
    }
}
```

### 2. crates/op-agents/src/unified/persona/mod.rs

**Current:**
```rust
use once_cell::sync::Lazy;

pub static PERSONA_AGENTS: Lazy<HashMap<&'static str, fn() -> Box<dyn super::UnifiedAgent>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("django-expert", || Box::new(DjangoExpert::new()));
    // ...
    m
});
```

**Replace with:**
```rust
// Use const fn or regular function instead of Lazy

pub fn persona_agent_factories() -> HashMap<&'static str, fn() -> Box<dyn super::UnifiedAgent>> {
    let mut m = HashMap::new();
    m.insert("django-expert", || Box::new(DjangoExpert::new()) as Box<dyn super::UnifiedAgent>);
    m.insert("fastapi-expert", || Box::new(FastAPIExpert::new()) as Box<dyn super::UnifiedAgent>);
    m.insert("react-expert", || Box::new(ReactExpert::new()) as Box<dyn super::UnifiedAgent>);
    // ...
    m
}
```

### 3. crates/op-tools/src/lazy_factory.rs

This file is about **lazy tool creation**, which is different from lazy statics. The factory pattern is fine — tools are created on-demand. But remove any `Lazy` statics.

**Check for and remove:**
```rust
// BAD
static FACTORIES: Lazy<HashMap<...>> = Lazy::new(...);

// GOOD - pass explicitly
pub struct CompositeToolFactory {
    factories: Arc<RwLock<HashMap<String, Box<dyn ToolFactory>>>>,
}
```

### 4. Any other files with `lazy_static!` or `once_cell`

Search the codebase:
```bash
grep -r "lazy_static" crates/
grep -r "once_cell" crates/
grep -r "OnceCell" crates/
grep -r "Lazy::new" crates/
```

## Pattern Replacements

### Pattern 1: Global Config

**Before:**
```rust
lazy_static! {
    static ref CONFIG: Config = Config::load();
}
```

**After:**
```rust
pub struct App {
    config: Arc<Config>,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load()?; // Fail fast at startup
        Ok(Self { config: Arc::new(config) })
    }
}
```

### Pattern 2: Singleton Service

**Before:**
```rust
static SERVICE: Lazy<MyService> = Lazy::new(|| MyService::new());

fn do_thing() {
    SERVICE.do_something();
}
```

**After:**
```rust
pub struct App {
    service: Arc<MyService>,
}

impl App {
    pub fn new() -> Self {
        Self {
            service: Arc::new(MyService::new()),
        }
    }
    
    pub fn do_thing(&self) {
        self.service.do_something();
    }
}
```

### Pattern 3: Compiled Regex

**Before:**
```rust
lazy_static! {
    static ref EMAIL_RE: Regex = Regex::new(r"...").unwrap();
}
```

**After (option A - const):**
```rust
use regex::Regex;
use std::sync::OnceLock;

fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"...").unwrap())
}
```

**After (option B - pass explicitly):**
```rust
pub struct Validator {
    email_re: Regex,
}

impl Validator {
    pub fn new() -> Self {
        Self {
            email_re: Regex::new(r"...").unwrap(),
        }
    }
}
```

### Pattern 4: Registry with Lazy Loading

**Before:**
```rust
static REGISTRY: Lazy<RwLock<HashMap<String, Tool>>> = Lazy::new(|| RwLock::new(HashMap::new()));
```

**After:**
```rust
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }
}

// Pass Arc<ToolRegistry> to components that need it
```

## Verification

After removing lazy patterns:

```bash
# Should return no results
grep -r "lazy_static" crates/
grep -r "once_cell::sync::Lazy" crates/

# OnceLock for truly const things (like compiled regex) is OK
# but prefer explicit passing
```

## Migration Checklist

- [ ] `crates/op-agents/src/unified/registry.rs` - Remove `GLOBAL_REGISTRY`
- [ ] `crates/op-agents/src/unified/persona/mod.rs` - Remove `PERSONA_AGENTS` static
- [ ] `crates/op-agents/src/unified/execution/mod.rs` - Check for similar patterns
- [ ] `crates/op-agents/src/unified/orchestration/mod.rs` - Check for similar patterns
- [ ] `crates/op-tools/src/` - Check all files
- [ ] `crates/op-chat/src/orchestration/` - Check all files
- [ ] `crates/op-mcp/src/` - Check all files
- [ ] Remove `lazy_static` and `once_cell` from `Cargo.toml` dependencies
- [ ] Run `cargo build` to find any remaining usages
- [ ] Run tests to verify functionality

## Benefits After Migration

1. **Clear ownership** — All state has explicit owners
2. **Testable** — Can inject mocks, no global state
3. **Debuggable** — State visible in debugger
4. **Predictable** — Startup either succeeds or fails, no runtime surprises
